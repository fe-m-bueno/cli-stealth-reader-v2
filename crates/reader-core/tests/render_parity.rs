//! Cross-language render parity.
//!
//! `tests/golden/render-parity.json` is produced from the TypeScript v1 by
//! `tools/generate-render-golden.mjs`. Each case names a rendering configuration
//! and records the stripped text of every line v1 emitted. Styling is verified by
//! the unit tests in `reader-core`; this suite pins the text itself, which is what
//! the reader actually shows.

use std::collections::BTreeMap;

use reader_core::book::CanonicalBlock;
use reader_core::render::{RenderOptions, render_blocks};
use reader_core::settings::{CodeDensity, CodeLanguage, LineSpacing, RenderMode};
use reader_core::theme::{AppearanceThemeId, ColorSchemeId, Theme};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Golden {
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    lines: Vec<String>,
}

fn blocks() -> BTreeMap<&'static str, CanonicalBlock> {
    let mut map = BTreeMap::new();
    map.insert(
        "prose",
        CanonicalBlock::Paragraph {
            id: "prose".into(),
            text: r#"She said "hello" before crossing the narrow bridge under C:\moonlight"#.into(),
        },
    );
    map.insert(
        "longProse",
        CanonicalBlock::Paragraph {
            id: "long".into(),
            text: "The lantern swung once over the quiet harbour, and the whole sandstone town \
                   leaned in to listen while the tide argued with the breakwater about who had \
                   arrived first."
                .into(),
        },
    );
    map.insert(
        "shortProse",
        CanonicalBlock::Paragraph {
            id: "short".into(),
            text: "Dawn.".into(),
        },
    );
    map.insert(
        "emptyProse",
        CanonicalBlock::Paragraph {
            id: "empty".into(),
            text: String::new(),
        },
    );
    map.insert(
        "dialogue",
        CanonicalBlock::Paragraph {
            id: "dialogue".into(),
            text: "— Come in, she said. “The night is «cold» and he didn't answer.”".into(),
        },
    );
    map.insert(
        "heading",
        CanonicalBlock::Heading {
            id: "heading".into(),
            text: "A quiet chapter".into(),
            level: Some(2),
        },
    );
    map.insert(
        "sceneBreak",
        CanonicalBlock::SceneBreak {
            id: "break".into(),
            text: String::new(),
        },
    );
    map.insert(
        "image",
        CanonicalBlock::Image {
            id: "image".into(),
            text: "Map of Arrakis".into(),
            image_source: Some("images/1.jpg".into()),
        },
    );
    map.insert(
        "bareImage",
        CanonicalBlock::Image {
            id: "bare-image".into(),
            text: String::new(),
            image_source: None,
        },
    );
    map.insert(
        "listItem",
        CanonicalBlock::ListItem {
            id: "list".into(),
            text: "first item of the list".into(),
        },
    );
    map.insert(
        "blockquote",
        CanonicalBlock::Blockquote {
            id: "quote".into(),
            text: "Remember the harbour, and remember the cold that came after it.".into(),
        },
    );
    map.insert(
        "anchor",
        CanonicalBlock::Anchor {
            id: "anchor".into(),
            text: "anchor text".into(),
            anchor_id: Some("top".into()),
        },
    );
    map
}

fn run_of_blocks(catalogue: &BTreeMap<&'static str, CanonicalBlock>) -> Vec<CanonicalBlock> {
    [
        "heading",
        "prose",
        "blockquote",
        "listItem",
        "sceneBreak",
        "longProse",
        "image",
    ]
    .into_iter()
    .map(|name| catalogue[name].clone())
    .collect()
}

fn language(name: &str) -> CodeLanguage {
    CodeLanguage::from_id(name).unwrap_or_else(|| panic!("unknown language {name}"))
}

fn spacing(name: &str) -> LineSpacing {
    LineSpacing::from_id(name).unwrap_or_else(|| panic!("unknown spacing {name}"))
}

/// Reproduce one golden case's configuration and render it.
fn render_case(name: &str, catalogue: &BTreeMap<&'static str, CanonicalBlock>) -> Vec<String> {
    let theme = Theme::resolve(ColorSchemeId::Codex, AppearanceThemeId::Dark);
    let parts: Vec<&str> = name.split('/').collect();
    let options = match parts.as_slice() {
        ["code", language_name, density, block_name, block_index] => {
            let density = density
                .strip_prefix('d')
                .and_then(|value| value.parse::<u8>().ok())
                .and_then(CodeDensity::new)
                .expect("golden density should be 1..=5");
            let options = RenderOptions::new(&theme.palette, 80)
                .with_code(language(language_name), density)
                .with_block_offset(block_index.parse().expect("golden block index"))
                .with_trailing_spacing(false);
            return render(&[catalogue[*block_name].clone()], &options);
        }
        ["code-width", language_name, width, block_index] => {
            RenderOptions::new(&theme.palette, width.parse().expect("golden width"))
                .with_code(language(language_name), CodeDensity::DEFAULT)
                .with_block_offset(block_index.parse().expect("golden block index"))
                .with_trailing_spacing(false)
        }
        ["plain", highlight, width, block_name] => {
            let options = RenderOptions::new(&theme.palette, width.parse().expect("golden width"))
                .with_plain_highlight(*highlight == "highlight")
                .with_trailing_spacing(false);
            return render(&[catalogue[*block_name].clone()], &options);
        }
        ["spacing", target, spacing_name, trailing] => {
            let trailing = *trailing == "true";
            let base = RenderOptions::new(&theme.palette, 80)
                .with_line_spacing(spacing(spacing_name))
                .with_trailing_spacing(trailing);
            let options = if let Some(language_name) = target.strip_prefix("code-") {
                base.with_code(language(language_name), CodeDensity::DEFAULT)
                    .with_block_offset(3)
            } else {
                base
            };
            return render(&run_of_blocks(catalogue), &options);
        }
        ["search", mode] => {
            let mode = if *mode == "plain" {
                RenderMode::Plain
            } else {
                RenderMode::Code
            };
            RenderOptions::new(&theme.palette, 80)
                .with_mode(mode)
                .with_search(Some("harbour"))
                .with_block_offset(5)
                .with_trailing_spacing(false)
        }
        _ => panic!("unrecognized golden case {name}"),
    };

    // Cases that fall through here all render the long paragraph.
    render(&[catalogue["longProse"].clone()], &options)
}

fn render(blocks: &[CanonicalBlock], options: &RenderOptions<'_>) -> Vec<String> {
    render_blocks(blocks, options)
        .iter()
        .map(reader_core::style::StyledLine::text)
        .collect()
}

/// Cases where v2 deliberately renders differently from v1.
///
/// v1 folded a list item's bullet into its text before wrapping, and wrapping
/// splits on whitespace — so the indent was dropped and a wrapped item ran back
/// to column zero. v2 keeps the marker out of the wrapped text, which restores
/// the indent and hangs continuation lines under the first word.
/// `improves_on_v1_for_list_items` below asserts the new shape.
fn is_intentional_improvement(name: &str) -> bool {
    name.starts_with("plain/") && name.ends_with("/listItem") || name.starts_with("spacing/plain/")
}

#[test]
fn rendering_matches_the_v1_golden_output() {
    let raw = include_str!("golden/render-parity.json");
    let golden: Golden = serde_json::from_str(raw).expect("golden fixture should parse");
    assert!(
        golden.cases.len() > 10_000,
        "the golden fixture lost coverage: {} cases",
        golden.cases.len()
    );

    let catalogue = blocks();
    let mut mismatches: Vec<String> = Vec::new();
    let mut improved = 0usize;
    for case in &golden.cases {
        let actual = render_case(&case.name, &catalogue);
        if actual == case.lines {
            continue;
        }
        if is_intentional_improvement(&case.name) {
            improved += 1;
            continue;
        }
        mismatches.push(format!(
            "{}\n  v1: {:?}\n  v2: {:?}",
            case.name, case.lines, actual
        ));
    }

    assert!(
        mismatches.is_empty(),
        "{} of {} cases differ from v1:\n{}",
        mismatches.len(),
        golden.cases.len(),
        mismatches
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        improved > 0,
        "the list-item improvement is no longer visible in the fixture"
    );
}

#[test]
fn improves_on_v1_for_list_items() {
    let raw = include_str!("golden/render-parity.json");
    let golden: Golden = serde_json::from_str(raw).expect("golden fixture should parse");
    let catalogue = blocks();

    // The narrowest fixture width is where v1's dropped indent showed worst.
    let case = golden
        .cases
        .iter()
        .find(|case| case.name == "plain/highlight/40/listItem")
        .expect("the fixture covers a narrow list item");
    let actual = render_case(&case.name, &catalogue);

    assert_eq!(
        case.lines[0], "· first item of the list",
        "v1 lost the indent"
    );
    assert_eq!(
        actual[0], "  · first item of the list",
        "v2 keeps the indent"
    );
}
