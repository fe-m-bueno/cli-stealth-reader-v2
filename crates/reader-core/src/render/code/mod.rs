//! Code-disguise rendering.
//!
//! Prose is wrapped, then each line is rewritten as a plausible statement in the
//! chosen language. Everything is derived from the block index and line index
//! through [`super::text::line_hash`], so the same book always disguises the
//! same way — that determinism is what makes the disguise unnoticeable while
//! scrolling.

mod python;
mod rust;
mod typescript;

use crate::book::CanonicalBlock;
use crate::settings::{CodeDensity, CodeLanguage};
use crate::style::{Span, Style, StyledLine};
use crate::theme::Palette;

/// Syntax roles a disguised token can take, each mapped to a palette color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    /// Language keyword.
    Keyword,
    /// Function or macro name.
    Function,
    /// String literal.
    Literal,
    /// Comment.
    Comment,
    /// Identifier.
    Ident,
    /// Type name.
    Type,
    /// Operator or punctuation.
    Operator,
    /// De-emphasized identifier, such as a struct field.
    Dim,
    /// Plain text with no styling, used for indentation.
    Raw,
}

impl Role {
    fn style(self, palette: &Palette) -> Style {
        match self {
            Self::Keyword => Style::fg(palette.keyword),
            Self::Function => Style::fg(palette.accent).bold(),
            Self::Literal => Style::fg(palette.code_string),
            Self::Comment => Style::fg(palette.subtle),
            Self::Ident => Style::fg(palette.foreground),
            Self::Type => Style::fg(palette.accent_muted),
            Self::Operator => Style::fg(palette.border),
            Self::Dim => Style::fg(palette.dim),
            Self::Raw => Style::new(),
        }
    }
}

/// Accumulates a disguised line as role-tagged fragments.
pub(crate) struct LineBuilder<'a> {
    palette: &'a Palette,
    line: StyledLine,
}

impl<'a> LineBuilder<'a> {
    pub(crate) fn new(palette: &'a Palette) -> Self {
        Self {
            palette,
            line: StyledLine::empty(),
        }
    }

    /// Start a line with literal indentation.
    pub(crate) fn indented(palette: &'a Palette, indent: &str) -> Self {
        let mut builder = Self::new(palette);
        builder.push(Role::Raw, indent);
        builder
    }

    pub(crate) fn push(&mut self, role: Role, text: impl AsRef<str>) -> &mut Self {
        let text = text.as_ref();
        if !text.is_empty() {
            self.line.push(Span::new(text, role.style(self.palette)));
        }
        self
    }

    /// Append an already-built line, keeping its styling.
    pub(crate) fn extend(&mut self, other: &StyledLine) -> &mut Self {
        for span in &other.spans {
            self.line.push(span.clone());
        }
        self
    }

    /// A single unstyled space, as v1 emitted between colored tokens.
    pub(crate) fn space(&mut self) -> &mut Self {
        self.push(Role::Raw, " ")
    }

    /// Take the finished line. Consuming the builder avoids cloning every span
    /// on a path that runs once per rendered line.
    pub(crate) fn build(&mut self) -> StyledLine {
        std::mem::take(&mut self.line)
    }
}

/// One-line shorthand for a comment-style line.
pub(crate) fn comment_line(palette: &Palette, text: impl AsRef<str>) -> StyledLine {
    StyledLine::single(text.as_ref(), Role::Comment.style(palette))
}

/// The shared heading, scene-break, and image handling: every language keeps the
/// canonical meaning of these blocks and only changes the comment syntax.
pub(crate) fn structural_block(
    block: &CanonicalBlock,
    palette: &Palette,
    comment_prefix: &str,
    scene_break: &str,
) -> Option<Vec<StyledLine>> {
    match block {
        CanonicalBlock::Heading { text, .. } => Some(vec![StyledLine::single(
            format!("{comment_prefix} {}", text.to_uppercase()),
            Style::fg(palette.accent).bold(),
        )]),
        CanonicalBlock::SceneBreak { .. } => Some(vec![comment_line(palette, scene_break)]),
        CanonicalBlock::Image { text, .. } => {
            let label = if text.is_empty() {
                "[image]".to_owned()
            } else {
                format!("[image: {text}]")
            };
            Some(vec![comment_line(
                palette,
                format!("{comment_prefix} {label}"),
            )])
        }
        _ => None,
    }
}

/// Render one block as disguised code in `language`.
#[must_use]
pub fn render_code(
    block: &CanonicalBlock,
    width: usize,
    palette: &Palette,
    block_index: usize,
    language: CodeLanguage,
    density: CodeDensity,
) -> Vec<StyledLine> {
    match language {
        CodeLanguage::TypeScript => typescript::render(block, width, palette, block_index, density),
        CodeLanguage::Python => python::render(block, width, palette, block_index),
        CodeLanguage::Rust => rust::render(block, width, palette, block_index),
    }
}

/// The prose a block contributes to the disguise, or `None` for structural blocks.
pub(crate) fn block_text(block: &CanonicalBlock) -> Option<&str> {
    match block {
        CanonicalBlock::Paragraph { text, .. }
        | CanonicalBlock::Blockquote { text, .. }
        | CanonicalBlock::ListItem { text, .. }
        | CanonicalBlock::Anchor { text, .. } => Some(text),
        CanonicalBlock::Heading { .. }
        | CanonicalBlock::SceneBreak { .. }
        | CanonicalBlock::Image { .. } => None,
    }
}

pub(crate) use structural_block as render_structural;
