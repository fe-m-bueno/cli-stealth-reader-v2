//! Learned reading pace and remaining-time estimates.
//!
//! The model is a mass-weighted average of observed words-per-minute: each
//! sample carries the active milliseconds it was measured over, so a long
//! reading session outweighs a brief one. A cold global model is blended toward
//! [`DEFAULT_WPM`], and a per-book model gradually takes over as the reader
//! spends time in that book.

/// Assumed pace before anything has been observed.
pub const DEFAULT_WPM: f64 = 230.0;
/// Longest gap still counted as reading; anything longer is idle time.
pub const IDLE_MS: f64 = 120_000.0;
/// Active time after which the global model fully replaces the default.
pub const COLD_START_MS: f64 = 240_000.0;
/// Active time after which the per-book model fully replaces the global one.
pub const BOOK_BLEND_MS: f64 = 600_000.0;
/// Samples outside these bounds are discarded as scrolling or skimming.
pub const MIN_INSTANT_WPM: f64 = 50.0;
pub const MAX_INSTANT_WPM: f64 = 800.0;

/// Word count of one chapter, the only chapter data the estimates need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChapterWords {
    pub word_count: usize,
}

impl ChapterWords {
    #[must_use]
    pub const fn new(word_count: usize) -> Self {
        Self { word_count }
    }
}

/// A measured reading burst.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaceSample {
    pub words_advanced: f64,
    pub active_ms: f64,
}

/// Which scope an estimate is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstimateScope {
    Chapter,
    Book,
}

/// The persisted pace model plus the cursor bookkeeping used to derive samples.
#[derive(Debug, Clone, PartialEq)]
pub struct PaceState {
    pub global_wpm: f64,
    pub global_active_ms: f64,
    pub book_id: Option<String>,
    pub book_wpm: f64,
    pub book_active_ms: f64,
    /// Absolute word cursor at the last sample, for forward-only deltas.
    pub last_word_cursor: Option<f64>,
    pub last_sample_at: Option<i64>,
}

impl Default for PaceState {
    fn default() -> Self {
        Self {
            global_wpm: DEFAULT_WPM,
            global_active_ms: 0.0,
            book_id: None,
            book_wpm: DEFAULT_WPM,
            book_active_ms: 0.0,
            last_word_cursor: None,
            last_sample_at: None,
        }
    }
}

fn clamp(value: f64, min: f64, max: f64) -> f64 {
    value.max(min).min(max)
}

fn mass_weighted_wpm(previous_wpm: f64, previous_ms: f64, sample_wpm: f64, sample_ms: f64) -> f64 {
    let total = previous_ms + sample_ms;
    if total <= 0.0 {
        return sample_wpm;
    }
    (previous_wpm * previous_ms + sample_wpm * sample_ms) / total
}

/// Cursor bookkeeping produced alongside a sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SampleMeta {
    pub last_word_cursor: Option<f64>,
    pub last_sample_at: Option<i64>,
}

impl PaceState {
    /// Fold a sample into the model, ignoring idle gaps and implausible rates.
    #[must_use]
    pub fn apply_sample(&self, sample: PaceSample) -> Self {
        let active_ms = clamp(sample.active_ms, 0.0, IDLE_MS);
        let words_advanced = sample.words_advanced.max(0.0);
        if words_advanced <= 0.0 || active_ms <= 0.0 {
            return self.clone();
        }
        let instant_wpm = words_advanced / (active_ms / 60_000.0);
        if !(MIN_INSTANT_WPM..=MAX_INSTANT_WPM).contains(&instant_wpm) {
            return self.clone();
        }
        Self {
            global_wpm: mass_weighted_wpm(
                self.global_wpm,
                self.global_active_ms,
                instant_wpm,
                active_ms,
            ),
            global_active_ms: self.global_active_ms + active_ms,
            book_wpm: mass_weighted_wpm(self.book_wpm, self.book_active_ms, instant_wpm, active_ms),
            book_active_ms: self.book_active_ms + active_ms,
            book_id: self.book_id.clone(),
            last_word_cursor: self.last_word_cursor,
            last_sample_at: self.last_sample_at,
        }
    }

    /// The pace to estimate with: global model blended out of the cold-start
    /// default, then blended toward the current book's own pace.
    #[must_use]
    pub fn effective_wpm(&self) -> f64 {
        let base = if self.global_active_ms < COLD_START_MS {
            let progress = self.global_active_ms / COLD_START_MS;
            (1.0 - progress) * DEFAULT_WPM + progress * self.global_wpm
        } else {
            self.global_wpm
        };
        let book_weight = clamp(self.book_active_ms / BOOK_BLEND_MS, 0.0, 1.0);
        (1.0 - book_weight) * base + book_weight * self.book_wpm
    }

    /// Derive the next sample from a new position.
    ///
    /// When reading is not active the timing window is broken so time spent in
    /// overlays cannot be charged to the next forward move. The word cursor is a
    /// high-water mark, so re-reading the same text never trains the model twice.
    #[must_use]
    pub fn prepare_sample(
        &self,
        now: i64,
        word_cursor: f64,
        reading_active: bool,
    ) -> (Option<PaceSample>, SampleMeta) {
        let advanced_cursor = Some(match self.last_word_cursor {
            None => word_cursor,
            Some(previous) => previous.max(word_cursor),
        });

        if !reading_active {
            return (
                None,
                SampleMeta {
                    last_word_cursor: advanced_cursor,
                    last_sample_at: None,
                },
            );
        }

        let (Some(previous_cursor), Some(previous_at)) =
            (self.last_word_cursor, self.last_sample_at)
        else {
            return (
                None,
                SampleMeta {
                    last_word_cursor: advanced_cursor,
                    last_sample_at: Some(now),
                },
            );
        };

        let sample = PaceSample {
            words_advanced: (word_cursor - previous_cursor).max(0.0),
            active_ms: ((now - previous_at) as f64).max(0.0),
        };
        (
            Some(sample),
            SampleMeta {
                last_word_cursor: advanced_cursor,
                last_sample_at: Some(now),
            },
        )
    }

    /// Replace the cursor bookkeeping after [`Self::prepare_sample`].
    pub fn set_meta(&mut self, meta: SampleMeta) {
        self.last_word_cursor = meta.last_word_cursor;
        self.last_sample_at = meta.last_sample_at;
    }
}

/// Absolute word position at `chapter_progress` through `chapter_index`.
#[must_use]
pub fn absolute_word_cursor(
    chapters: &[ChapterWords],
    chapter_index: usize,
    chapter_progress: f64,
) -> f64 {
    if chapters.is_empty() {
        return 0.0;
    }
    let safe_index = chapter_index.min(chapters.len() - 1);
    let mut words: f64 = chapters[..safe_index]
        .iter()
        .map(|chapter| chapter.word_count as f64)
        .sum();
    words += clamp(chapter_progress, 0.0, 1.0) * chapters[safe_index].word_count as f64;
    words
}

/// Words left in the current chapter.
#[must_use]
pub fn remaining_words_in_chapter(
    chapters: &[ChapterWords],
    chapter_index: usize,
    chapter_progress: f64,
) -> f64 {
    let chapter_words = chapters
        .get(chapter_index)
        .map_or(0.0, |chapter| chapter.word_count as f64);
    (chapter_words * (1.0 - clamp(chapter_progress, 0.0, 1.0))).max(0.0)
}

/// Words left in the whole book.
#[must_use]
pub fn remaining_words_in_book(
    chapters: &[ChapterWords],
    chapter_index: usize,
    chapter_progress: f64,
) -> f64 {
    let mut remaining = remaining_words_in_chapter(chapters, chapter_index, chapter_progress);
    if chapter_index + 1 < chapters.len() {
        remaining += chapters[chapter_index + 1..]
            .iter()
            .map(|chapter| chapter.word_count as f64)
            .sum::<f64>();
    }
    remaining
}

/// Minutes needed to read `remaining_words` at `wpm`.
#[must_use]
pub fn estimate_minutes(remaining_words: f64, wpm: f64) -> f64 {
    if remaining_words <= 0.0 || wpm <= 0.0 {
        return 0.0;
    }
    remaining_words / wpm
}

/// Footer duration text: minutes under an hour, then `Nh` or `Nh Mm`.
#[must_use]
pub fn format_duration(total_seconds: f64) -> String {
    let seconds = total_seconds.max(0.0);
    let total_minutes = (seconds / 60.0).ceil().max(1.0) as i64;
    if total_minutes < 60 {
        return format!("{total_minutes} min");
    }
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    if minutes == 0 {
        format!("{hours}h")
    } else {
        format!("{hours}h {minutes}m")
    }
}

/// Footer estimate text, or an em dash when there is nothing left to estimate.
#[must_use]
pub fn format_time_left(remaining_words: f64, wpm: f64, scope: EstimateScope) -> String {
    if remaining_words <= 0.0 || wpm <= 0.0 {
        return "—".to_owned();
    }
    let label = format_duration(estimate_minutes(remaining_words, wpm) * 60.0);
    match scope {
        EstimateScope::Chapter => format!("{label} left in chapter"),
        EstimateScope::Book => format!("{label} left in book"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BOOK_BLEND_MS, COLD_START_MS, ChapterWords, DEFAULT_WPM, EstimateScope, IDLE_MS,
        PaceSample, PaceState, absolute_word_cursor, format_duration, format_time_left,
        remaining_words_in_book, remaining_words_in_chapter,
    };

    fn chapters(counts: &[usize]) -> Vec<ChapterWords> {
        counts.iter().copied().map(ChapterWords::new).collect()
    }

    #[test]
    fn a_fresh_model_estimates_at_the_default_pace() {
        assert!((PaceState::default().effective_wpm() - DEFAULT_WPM).abs() < 1e-9);
    }

    #[test]
    fn idle_and_outlier_samples_are_ignored() {
        let state = PaceState::default();
        // Zero words, zero time, too slow, and too fast are all rejected.
        for sample in [
            PaceSample {
                words_advanced: 0.0,
                active_ms: 30_000.0,
            },
            PaceSample {
                words_advanced: 300.0,
                active_ms: 0.0,
            },
            PaceSample {
                words_advanced: 10.0,
                active_ms: 60_000.0,
            },
            PaceSample {
                words_advanced: 5_000.0,
                active_ms: 60_000.0,
            },
        ] {
            assert_eq!(state.apply_sample(sample), state);
        }
    }

    #[test]
    fn active_time_is_capped_at_the_idle_bound() {
        let state = PaceState::default();
        let updated = state.apply_sample(PaceSample {
            words_advanced: 600.0,
            active_ms: IDLE_MS * 10.0,
        });
        assert!((updated.global_active_ms - IDLE_MS).abs() < 1e-9);
    }

    #[test]
    fn a_longer_sample_moves_the_model_further() {
        // Both samples observe 400 wpm against an established 200 wpm model; the
        // longer one carries more mass and pulls the average further.
        let established = PaceState {
            global_wpm: 200.0,
            global_active_ms: 120_000.0,
            book_wpm: 200.0,
            book_active_ms: 120_000.0,
            ..PaceState::default()
        };
        let short = established.apply_sample(PaceSample {
            words_advanced: 200.0,
            active_ms: 30_000.0,
        });
        let long = established.apply_sample(PaceSample {
            words_advanced: 800.0,
            active_ms: 120_000.0,
        });
        assert!(short.global_wpm > 200.0);
        assert!(
            long.global_wpm > short.global_wpm,
            "{} should exceed {}",
            long.global_wpm,
            short.global_wpm
        );
    }

    #[test]
    fn the_first_sample_defines_the_model_outright() {
        // With no accumulated mass the weighted average is just the sample.
        let updated = PaceState::default().apply_sample(PaceSample {
            words_advanced: 400.0,
            active_ms: 60_000.0,
        });
        assert!((updated.global_wpm - 400.0).abs() < 1e-9);
        assert!((updated.book_wpm - 400.0).abs() < 1e-9);
    }

    #[test]
    fn the_book_model_takes_over_once_it_has_enough_mass() {
        let state = PaceState {
            global_wpm: 200.0,
            global_active_ms: COLD_START_MS,
            book_wpm: 400.0,
            book_active_ms: BOOK_BLEND_MS,
            ..PaceState::default()
        };
        assert!((state.effective_wpm() - 400.0).abs() < 1e-9);
    }

    #[test]
    fn a_cold_global_model_stays_near_the_default() {
        let state = PaceState {
            global_wpm: 600.0,
            global_active_ms: 0.0,
            ..PaceState::default()
        };
        assert!((state.effective_wpm() - DEFAULT_WPM).abs() < 1e-9);
    }

    #[test]
    fn word_cursors_and_remaining_counts_are_bounded() {
        let book = chapters(&[100, 200, 300]);
        assert!((absolute_word_cursor(&book, 1, 0.5) - 200.0).abs() < 1e-9);
        assert!((absolute_word_cursor(&book, 99, 1.0) - 600.0).abs() < 1e-9);
        assert!((absolute_word_cursor(&book, 0, -5.0) - 0.0).abs() < 1e-9);
        assert!((absolute_word_cursor(&[], 0, 1.0) - 0.0).abs() < 1e-9);
        assert!((remaining_words_in_chapter(&book, 1, 0.25) - 150.0).abs() < 1e-9);
        assert!((remaining_words_in_book(&book, 1, 0.5) - 400.0).abs() < 1e-9);
        assert!((remaining_words_in_book(&book, 2, 1.0) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn durations_round_up_to_whole_minutes() {
        assert_eq!(format_duration(0.0), "1 min");
        assert_eq!(format_duration(61.0), "2 min");
        assert_eq!(format_duration(3_600.0), "1h");
        assert_eq!(format_duration(3_660.0), "1h 1m");
        assert_eq!(format_duration(-10.0), "1 min");
    }

    #[test]
    fn time_left_names_its_scope_and_degrades_to_a_dash() {
        assert_eq!(
            format_time_left(460.0, 230.0, EstimateScope::Chapter),
            "2 min left in chapter"
        );
        assert_eq!(
            format_time_left(460.0, 230.0, EstimateScope::Book),
            "2 min left in book"
        );
        assert_eq!(format_time_left(0.0, 230.0, EstimateScope::Book), "—");
        assert_eq!(format_time_left(100.0, 0.0, EstimateScope::Book), "—");
    }

    #[test]
    fn inactive_reading_breaks_the_timing_window() {
        let state = PaceState {
            last_word_cursor: Some(100.0),
            last_sample_at: Some(1_000),
            ..PaceState::default()
        };

        let (sample, meta) = state.prepare_sample(5_000, 150.0, false);

        assert!(sample.is_none());
        assert_eq!(meta.last_sample_at, None);
        assert_eq!(meta.last_word_cursor, Some(150.0));
    }

    #[test]
    fn the_first_active_position_only_opens_the_window() {
        let state = PaceState::default();
        let (sample, meta) = state.prepare_sample(1_000, 40.0, true);
        assert!(sample.is_none());
        assert_eq!(meta.last_sample_at, Some(1_000));
        assert_eq!(meta.last_word_cursor, Some(40.0));
    }

    #[test]
    fn moving_backward_never_produces_words_or_lowers_the_high_water_cursor() {
        let state = PaceState {
            last_word_cursor: Some(500.0),
            last_sample_at: Some(1_000),
            ..PaceState::default()
        };

        let (sample, meta) = state.prepare_sample(2_000, 200.0, true);

        let sample = sample.expect("an active move always yields a sample");
        assert!((sample.words_advanced - 0.0).abs() < 1e-9);
        assert_eq!(meta.last_word_cursor, Some(500.0));
    }

    #[test]
    fn forward_movement_measures_words_and_elapsed_time() {
        let mut state = PaceState {
            last_word_cursor: Some(100.0),
            last_sample_at: Some(1_000),
            ..PaceState::default()
        };

        let (sample, meta) = state.prepare_sample(61_000, 330.0, true);

        let sample = sample.expect("an active move always yields a sample");
        assert!((sample.words_advanced - 230.0).abs() < 1e-9);
        assert!((sample.active_ms - 60_000.0).abs() < 1e-9);

        state.set_meta(meta);
        let updated = state.apply_sample(sample);
        assert!((updated.global_wpm - 230.0).abs() < 1e-9);
        assert!((updated.book_active_ms - 60_000.0).abs() < 1e-9);
    }
}
