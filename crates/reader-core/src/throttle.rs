//! Coalescing repeated writes.
//!
//! Scrolling changes the reading position on every keypress, and writing each
//! one would put the database in the middle of the reader's hot loop. This
//! throttle keeps the first write of a window and the last, dropping everything
//! between: the reader sees an immediate save when they stop, and the disk sees
//! one write per window instead of dozens.
//!
//! There is no timer here. The caller drives it with [`WriteThrottle::tick`] on
//! whatever loop it already has, which keeps the whole thing deterministic.

/// How long a write window lasts.
pub const POSITION_FLUSH_INTERVAL_MS: i64 = 1_500;

/// A write coalescer over values of type `T`.
#[derive(Debug, Clone)]
pub struct WriteThrottle<T> {
    interval_ms: i64,
    last_flush_at: Option<i64>,
    pending: Option<T>,
}

impl<T> WriteThrottle<T> {
    /// A throttle with the given window.
    #[must_use]
    pub const fn new(interval_ms: i64) -> Self {
        Self {
            interval_ms,
            last_flush_at: None,
            pending: None,
        }
    }

    /// Whether a write is waiting for the window to close.
    #[must_use]
    pub const fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Offer a value to write.
    ///
    /// Returns it when the window is open — the leading edge, which makes the
    /// first change of a burst immediate. Inside the window it is held, replacing
    /// anything held before it, since only the newest position matters.
    pub fn schedule(&mut self, value: T, now: i64) -> Option<T> {
        let window_open = self
            .last_flush_at
            .is_none_or(|last| now - last >= self.interval_ms);
        if window_open {
            self.last_flush_at = Some(now);
            self.pending = None;
            return Some(value);
        }
        self.pending = Some(value);
        None
    }

    /// Write now regardless of the window, dropping anything pending.
    ///
    /// Used where a write must not be lost — closing a book, switching to
    /// another, leaving the reader.
    pub fn schedule_immediate(&mut self, value: T, now: i64) -> Option<T> {
        self.last_flush_at = Some(now);
        self.pending = None;
        Some(value)
    }

    /// Release the pending write if its window has closed.
    ///
    /// Call this on every loop tick; it returns `None` until there is something
    /// to write and the window allows it.
    pub fn tick(&mut self, now: i64) -> Option<T> {
        if !self.has_pending() {
            return None;
        }
        let window_closed = self
            .last_flush_at
            .is_none_or(|last| now - last >= self.interval_ms);
        if window_closed {
            self.last_flush_at = Some(now);
            return self.pending.take();
        }
        None
    }

    /// Release the pending write now, whatever the window says.
    pub fn flush(&mut self, now: i64) -> Option<T> {
        let pending = self.pending.take()?;
        self.last_flush_at = Some(now);
        Some(pending)
    }
}

impl<T> Default for WriteThrottle<T> {
    fn default() -> Self {
        Self::new(POSITION_FLUSH_INTERVAL_MS)
    }
}

#[cfg(test)]
mod tests {
    use super::{POSITION_FLUSH_INTERVAL_MS, WriteThrottle};

    /// Collect what a sequence of calls actually wrote.
    fn writes(calls: impl IntoIterator<Item = Option<&'static str>>) -> Vec<&'static str> {
        calls.into_iter().flatten().collect()
    }

    #[test]
    fn the_first_write_of_a_window_happens_immediately() {
        let mut throttle: WriteThrottle<&str> = WriteThrottle::default();
        assert_eq!(throttle.schedule("a", 1_000), Some("a"));
        assert!(!throttle.has_pending());
    }

    #[test]
    fn writes_inside_the_window_are_coalesced_to_the_newest() {
        let mut throttle: WriteThrottle<&str> = WriteThrottle::new(1_500);
        let results = writes([
            throttle.schedule("a", 0),
            throttle.schedule("b", 500),
            throttle.schedule("c", 900),
        ]);

        assert_eq!(results, vec!["a"], "only the leading edge is written");
        assert!(throttle.has_pending());
        assert_eq!(
            throttle.flush(1_000),
            Some("c"),
            "the newest value survives"
        );
        assert!(!throttle.has_pending());
        assert_eq!(
            throttle.flush(1_100),
            None,
            "a drained throttle has nothing"
        );
    }

    #[test]
    fn a_write_happens_again_once_the_window_has_passed() {
        let mut throttle: WriteThrottle<&str> = WriteThrottle::new(1_500);
        assert_eq!(throttle.schedule("a", 0), Some("a"));
        assert_eq!(
            throttle.schedule("b", 1_400),
            None,
            "still inside the window"
        );
        assert_eq!(
            throttle.schedule("c", 1_500),
            Some("c"),
            "the window reopened"
        );
        assert!(!throttle.has_pending(), "the held value was superseded");
    }

    #[test]
    fn ticking_releases_the_pending_write_when_the_window_closes() {
        let mut throttle: WriteThrottle<&str> = WriteThrottle::new(1_500);
        throttle.schedule("a", 0);
        throttle.schedule("b", 100);

        assert_eq!(throttle.tick(200), None, "too soon");
        assert_eq!(throttle.tick(1_499), None, "still too soon");
        assert_eq!(throttle.tick(1_500), Some("b"), "the window closed");
        assert_eq!(throttle.tick(3_000), None, "and there is nothing left");
    }

    #[test]
    fn ticking_an_idle_throttle_does_nothing() {
        let mut throttle: WriteThrottle<&str> = WriteThrottle::new(1_500);
        assert_eq!(throttle.tick(10_000), None);
    }

    #[test]
    fn an_immediate_write_bypasses_the_window_and_clears_what_was_held() {
        let mut throttle: WriteThrottle<&str> = WriteThrottle::new(1_500);
        throttle.schedule("a", 0);
        throttle.schedule("b", 100);

        assert_eq!(throttle.schedule_immediate("c", 200), Some("c"));
        assert!(
            !throttle.has_pending(),
            "the held write is dropped, not written twice"
        );
        assert_eq!(throttle.flush(300), None);
    }

    #[test]
    fn an_immediate_write_restarts_the_window() {
        let mut throttle: WriteThrottle<&str> = WriteThrottle::new(1_500);
        throttle.schedule_immediate("a", 1_000);
        assert_eq!(throttle.schedule("b", 1_100), None, "inside the new window");
        assert_eq!(throttle.schedule("c", 2_500), Some("c"));
    }

    #[test]
    fn a_burst_of_scrolling_writes_a_handful_of_times_not_once_per_key() {
        // 100 keypresses at 30ms apart — three seconds of steady scrolling,
        // which spans two full windows.
        let mut throttle: WriteThrottle<usize> = WriteThrottle::default();
        let mut written: Vec<usize> = Vec::new();

        for offset in 0..100usize {
            let now = offset as i64 * 30;
            if let Some(value) = throttle.schedule(offset, now) {
                written.push(value);
            }
            if let Some(value) = throttle.tick(now) {
                written.push(value);
            }
        }
        if let Some(value) = throttle.flush(100 * 30) {
            written.push(value);
        }

        assert_eq!(
            written,
            vec![0, 50, 99],
            "the leading edge, one window boundary, and the final place"
        );
        assert_eq!(POSITION_FLUSH_INTERVAL_MS, 1_500);
    }

    #[test]
    fn stopping_mid_window_still_saves_the_place_on_the_next_tick() {
        // The reader scrolls, then stops. Nothing more is scheduled, so only a
        // tick can release the held position.
        let mut throttle: WriteThrottle<usize> = WriteThrottle::default();
        assert_eq!(throttle.schedule(1, 0), Some(1));
        assert_eq!(throttle.schedule(2, 100), None);
        assert_eq!(throttle.schedule(3, 200), None);

        assert_eq!(throttle.tick(700), None, "the window is still open");
        assert_eq!(throttle.tick(1_600), Some(3), "the last place is saved");
        assert!(!throttle.has_pending());
    }
}
