//! Cross-language import parity.
//!
//! `tests/fixtures/` holds EPUB and CBZ files built by
//! `tools/generate-import-golden.mjs`, and `tests/golden/import-parity.json`
//! records the canonical book v1 extracted from each. Both are committed, so this
//! suite needs no Node runtime.
//!
//! `id` and `source_path` are excluded because they hash the absolute path;
//! `reader_formats::ids` covers those derivations directly.

use std::path::{Path, PathBuf};

use reader_core::{CanonicalBlock, CanonicalBook, DiagnosticSeverity};
use reader_formats::{ImportError, import_file};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Golden {
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    fixture: String,
    result: Outcome,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Outcome {
    ok: bool,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    import_hash: Option<String>,
    #[serde(default)]
    parser_version: Option<u32>,
    #[serde(default)]
    diagnostics: Vec<GoldenDiagnostic>,
    #[serde(default)]
    chapters: Vec<GoldenChapter>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct GoldenDiagnostic {
    severity: String,
    message: String,
    #[serde(default)]
    context: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoldenChapter {
    id: String,
    index: usize,
    title: String,
    href: String,
    depth: usize,
    word_count: usize,
    blocks: Vec<serde_json::Value>,
}

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn severity_name(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Warning => "warning",
        DiagnosticSeverity::Error => "error",
    }
}

/// The v1 JSON shape of one block, for a field-by-field comparison.
fn block_json(block: &CanonicalBlock) -> serde_json::Value {
    serde_json::to_value(block).expect("blocks serialize to the v1 shape")
}

fn compare(book: &CanonicalBook, expected: &Outcome, fixture: &str, failures: &mut Vec<String>) {
    let mut mismatch = |detail: String| failures.push(format!("{fixture}: {detail}"));

    if Some(&book.title) != expected.title.as_ref() {
        mismatch(format!("title {:?} != {:?}", book.title, expected.title));
    }
    if Some(&book.author) != expected.author.as_ref() {
        mismatch(format!("author {:?} != {:?}", book.author, expected.author));
    }
    if Some(&book.import_hash) != expected.import_hash.as_ref() {
        mismatch(format!(
            "import hash {:?} != {:?}",
            book.import_hash, expected.import_hash
        ));
    }
    if book.parser_version != expected.parser_version {
        mismatch(format!(
            "parser version {:?} != {:?}",
            book.parser_version, expected.parser_version
        ));
    }

    let diagnostics: Vec<GoldenDiagnostic> = book
        .diagnostics
        .iter()
        .map(|diagnostic| GoldenDiagnostic {
            severity: severity_name(diagnostic.severity).to_owned(),
            message: diagnostic.message.clone(),
            context: diagnostic.context.clone(),
        })
        .collect();
    if diagnostics != expected.diagnostics {
        mismatch(format!(
            "diagnostics\n  v1: {:?}\n  v2: {diagnostics:?}",
            expected.diagnostics
        ));
    }

    if book.chapters.len() != expected.chapters.len() {
        mismatch(format!(
            "{} chapters != {}\n  v1: {:?}\n  v2: {:?}",
            book.chapters.len(),
            expected.chapters.len(),
            expected
                .chapters
                .iter()
                .map(|chapter| &chapter.title)
                .collect::<Vec<_>>(),
            book.chapters
                .iter()
                .map(|chapter| &chapter.title)
                .collect::<Vec<_>>()
        ));
        return;
    }

    for (chapter, golden) in book.chapters.iter().zip(&expected.chapters) {
        let label = format!("chapter {} ({:?})", golden.index, golden.title);
        if chapter.id != golden.id {
            mismatch(format!("{label}: id {:?} != {:?}", chapter.id, golden.id));
        }
        if chapter.index != golden.index
            || chapter.title != golden.title
            || chapter.href != golden.href
            || chapter.depth != golden.depth
        {
            mismatch(format!(
                "{label}: metadata ({}, {:?}, {:?}, {}) != ({}, {:?}, {:?}, {})",
                chapter.index,
                chapter.title,
                chapter.href,
                chapter.depth,
                golden.index,
                golden.title,
                golden.href,
                golden.depth
            ));
        }
        if chapter.word_count != golden.word_count {
            mismatch(format!(
                "{label}: word count {} != {}",
                chapter.word_count, golden.word_count
            ));
        }
        let blocks: Vec<serde_json::Value> = chapter.blocks.iter().map(block_json).collect();
        if blocks != golden.blocks {
            mismatch(format!(
                "{label}: blocks\n  v1: {:?}\n  v2: {blocks:?}",
                golden.blocks
            ));
        }
    }
}

#[test]
fn imports_match_the_v1_golden_books() {
    let raw = include_str!("golden/import-parity.json");
    let golden: Golden = serde_json::from_str(raw).expect("golden fixture should parse");
    assert_eq!(golden.cases.len(), 10, "the fixture set changed");

    let mut failures: Vec<String> = Vec::new();
    for case in &golden.cases {
        let path = fixture_path(&case.fixture);
        match (import_file(&path), case.result.ok) {
            (Ok(book), true) => compare(&book, &case.result, &case.fixture, &mut failures),
            (Err(error), false) => {
                let expected = case.result.message.as_deref().unwrap_or_default();
                if error.to_string() != expected {
                    failures.push(format!(
                        "{}: error {:?} != {expected:?}",
                        case.fixture,
                        error.to_string()
                    ));
                }
            }
            (Ok(_), false) => failures.push(format!(
                "{}: v2 imported it but v1 failed with {:?}",
                case.fixture, case.result.message
            )),
            (Err(error), true) => failures.push(format!(
                "{}: v2 failed with {error} but v1 imported it",
                case.fixture
            )),
        }
    }

    assert!(
        failures.is_empty(),
        "{} import mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn a_missing_file_reports_io_rather_than_a_malformed_book() {
    let error = import_file(&fixture_path("does-not-exist.epub")).expect_err("missing file");
    assert!(matches!(error, ImportError::Io(_)));
}
