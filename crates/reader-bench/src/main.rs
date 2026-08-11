//! The v2 half of the performance comparison.
//!
//! Measures the same things `bench/v1-baseline.mjs` does, against the same
//! generated corpus, so the two reports can be read side by side:
//!
//! ```text
//! cargo run --release -p reader-bench -- --json
//! ```
//!
//! Startup is measured as a cold process, because that is what a reader waits
//! for; everything else is measured in process with the same repeat counts as
//! the v1 harness.

use std::path::{Path, PathBuf};
use std::time::Instant;

use reader_core::render::{RenderOptions, render_blocks};
use reader_core::{AppSettings, CanonicalBook, CodeLanguage, RenderMode, Theme};
use reader_storage::{AppPaths, Storage};

const STARTUP_RUNS: usize = 10;
const IMPORT_RUNS: usize = 5;
const RENDER_RUNS: usize = 20;
/// Width the v1 harness rendered at.
const RENDER_WIDTH: usize = 100;

/// Median, min, and max of a set of measurements, in milliseconds.
#[derive(Debug, Clone, Copy)]
struct Stats {
    runs: usize,
    median: f64,
    min: f64,
    max: f64,
}

impl Stats {
    fn of(mut samples: Vec<f64>) -> Self {
        samples.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
        let middle = samples.len() / 2;
        let median = if samples.is_empty() {
            0.0
        } else if samples.len() % 2 == 0 {
            (samples[middle - 1] + samples[middle]) / 2.0
        } else {
            samples[middle]
        };
        Self {
            runs: samples.len(),
            median: round(median),
            min: round(samples.first().copied().unwrap_or(0.0)),
            max: round(samples.last().copied().unwrap_or(0.0)),
        }
    }

    fn to_json(self) -> serde_json::Value {
        serde_json::json!({
            "runs": self.runs,
            "median": self.median,
            "min": self.min,
            "max": self.max,
        })
    }
}

fn round(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn time<T>(runs: usize, mut task: impl FnMut() -> T) -> Stats {
    let mut samples = Vec::with_capacity(runs);
    for _ in 0..runs {
        let start = Instant::now();
        let result = task();
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
        // Keep the work from being optimized away.
        std::hint::black_box(result);
    }
    Stats::of(samples)
}

fn fixture_dir() -> PathBuf {
    std::env::var_os("BENCH_FIXTURE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("stealth-reader-bench-fixtures"))
}

/// Peak resident memory of this process, in megabytes.
///
/// Read from `/proc`, which is where the kernel already tracks it; a platform
/// without it reports zero rather than guessing.
fn peak_rss_mb() -> f64 {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return 0.0;
    };
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|value| value.split_whitespace().next())
        .and_then(|kilobytes| kilobytes.parse::<f64>().ok())
        .map_or(0.0, |kilobytes| round(kilobytes / 1024.0))
}

/// Measure a cold start: the binary opening its library and exiting.
fn measure_startup() -> Option<Stats> {
    let binary = std::env::current_exe()
        .ok()?
        .parent()?
        .join("stealth-reader");
    if !binary.is_file() {
        return None;
    }
    let scratch = std::env::temp_dir().join("stealth-reader-bench-startup");
    std::fs::remove_dir_all(&scratch).ok();
    std::fs::create_dir_all(&scratch).ok()?;

    let mut samples = Vec::with_capacity(STARTUP_RUNS);
    for _ in 0..STARTUP_RUNS {
        let start = Instant::now();
        let status = std::process::Command::new(&binary)
            .arg("--version")
            .env("XDG_DATA_HOME", &scratch)
            .env("XDG_CACHE_HOME", &scratch)
            .stdout(std::process::Stdio::null())
            .status()
            .ok()?;
        if !status.success() {
            return None;
        }
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    std::fs::remove_dir_all(&scratch).ok();
    Some(Stats::of(samples))
}

/// Open a library database from scratch, which is the rest of a real startup.
fn measure_storage_open() -> Stats {
    let scratch = std::env::temp_dir().join("stealth-reader-bench-storage");
    std::fs::remove_dir_all(&scratch).ok();
    let paths = AppPaths::from_roots(&scratch.join("data"), &scratch.join("cache"));

    let stats = time(STARTUP_RUNS, || {
        Storage::open(&paths).expect("the library should open")
    });
    std::fs::remove_dir_all(&scratch).ok();
    stats
}

fn measure_discovery(directory: &Path) -> Stats {
    time(STARTUP_RUNS, || {
        reader_formats::discover_books(directory).unwrap_or_default()
    })
}

fn measure_import(path: &Path) -> Option<(Stats, CanonicalBook)> {
    if !path.is_file() {
        return None;
    }
    let stats = time(IMPORT_RUNS, || reader_formats::import_file(path));
    let book = reader_formats::import_file(path).ok()?;
    Some((stats, book))
}

fn render_options<'a>(theme: &'a Theme, settings: &AppSettings) -> RenderOptions<'a> {
    RenderOptions {
        mode: settings.render_mode,
        width: RENDER_WIDTH,
        palette: &theme.palette,
        code_language: settings.code_language,
        code_density: settings.code_density,
        plain_highlight: settings.plain_highlight,
        line_spacing: settings.line_spacing,
        block_index_offset: 0,
        include_trailing_spacing: true,
        search_query: None,
    }
}

fn main() {
    let fixtures = fixture_dir();
    let theme = Theme::default();

    let startup = measure_startup();
    let storage_open = measure_storage_open();
    let discovery = measure_discovery(&fixtures);

    let mut imports = serde_json::Map::new();
    let mut large_book: Option<CanonicalBook> = None;
    for (name, file) in [
        ("smallEpub", "small.epub"),
        ("largeEpub", "large.epub"),
        ("cbz", "comic.cbz"),
        ("pdf", "doc.pdf"),
    ] {
        let path = fixtures.join(file);
        let Some((stats, book)) = measure_import(&path) else {
            continue;
        };
        let mut entry = stats.to_json();
        if let Some(object) = entry.as_object_mut() {
            object.insert(
                "fileBytes".to_owned(),
                serde_json::json!(std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0)),
            );
        }
        imports.insert(name.to_owned(), entry);
        if name == "largeEpub" {
            large_book = Some(book);
        }
    }

    let mut render = serde_json::Map::new();
    let mut corpus = serde_json::Map::new();
    if let Some(book) = large_book.as_ref() {
        let widest = book
            .chapters
            .iter()
            .max_by_key(|chapter| chapter.blocks.len())
            .expect("the corpus has chapters");
        corpus.insert(
            "largeEpubChapters".to_owned(),
            serde_json::json!(book.chapters.len()),
        );
        corpus.insert(
            "largeEpubWords".to_owned(),
            serde_json::json!(book.chapters.iter().map(|c| c.word_count).sum::<usize>()),
        );
        render.insert(
            "chapterBlocks".to_owned(),
            serde_json::json!(widest.blocks.len()),
        );

        let plain_settings = AppSettings {
            render_mode: RenderMode::Plain,
            ..AppSettings::default()
        };
        let code_settings = AppSettings {
            render_mode: RenderMode::Code,
            code_language: CodeLanguage::TypeScript,
            ..AppSettings::default()
        };

        render.insert(
            "plainChapterMs".to_owned(),
            time(RENDER_RUNS, || {
                render_blocks(&widest.blocks, &render_options(&theme, &plain_settings))
            })
            .to_json(),
        );
        render.insert(
            "codeChapterMs".to_owned(),
            time(RENDER_RUNS, || {
                render_blocks(&widest.blocks, &render_options(&theme, &code_settings))
            })
            .to_json(),
        );
        // What a repaint costs once the chapter is rendered — the cost the reader
        // actually pays per keypress, as opposed to the cold render above.
        let widest_index = book
            .chapters
            .iter()
            .enumerate()
            .max_by_key(|(_, chapter)| chapter.blocks.len())
            .map_or(0, |(index, _)| index);
        let mut reader = reader_app::ReaderState::new(plain_settings);
        reader.current_book = Some(book.clone());
        reader.chapter_index = widest_index;
        std::hint::black_box(reader.chapter_lines(RENDER_WIDTH as u16).len());
        // A single cached repaint rounds to zero at this resolution, so a hundred
        // of them are timed together — a hundred keypresses of scrolling.
        render.insert(
            "cachedRepaintsPer100Ms".to_owned(),
            time(RENDER_RUNS, || {
                (0..100)
                    .map(|_| reader.chapter_lines(RENDER_WIDTH as u16).len())
                    .sum::<usize>()
            })
            .to_json(),
        );

        render.insert(
            "wholeBookPlainMs".to_owned(),
            time((RENDER_RUNS / 4).max(3), || {
                let options = render_options(&theme, &plain_settings);
                book.chapters
                    .iter()
                    .map(|chapter| render_blocks(&chapter.blocks, &options).len())
                    .sum::<usize>()
            })
            .to_json(),
        );

        // Memory is measured after the heaviest work, matching the v1 probe.
        let options = render_options(&theme, &code_settings);
        for chapter in &book.chapters {
            std::hint::black_box(render_blocks(&chapter.blocks, &options));
        }
    }

    let report = serde_json::json!({
        "runtime": "v2-rust",
        "fixtureDir": fixtures.display().to_string(),
        "corpus": corpus,
        "startup": match startup {
            Some(stats) => serde_json::json!({ "processWallMs": stats.to_json() }),
            None => serde_json::json!({ "processWallMs": null }),
        },
        "storageOpenMs": storage_open.to_json(),
        "discoveryMs": discovery.to_json(),
        "imports": imports,
        "render": render,
        "memory": { "peakRssMb": peak_rss_mb() },
    });

    println!(
        "{}",
        serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_owned())
    );
}
