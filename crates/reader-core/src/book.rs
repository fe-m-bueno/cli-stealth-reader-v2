use serde::{Deserialize, Serialize};

/// Smallest canonical unit rendered by plain and stealth modes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum CanonicalBlock {
    Heading {
        id: String,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        level: Option<u8>,
    },
    Paragraph {
        id: String,
        text: String,
    },
    Blockquote {
        id: String,
        text: String,
    },
    ListItem {
        id: String,
        text: String,
    },
    SceneBreak {
        id: String,
        text: String,
    },
    Image {
        id: String,
        text: String,
        #[serde(rename = "imageSource", skip_serializing_if = "Option::is_none")]
        image_source: Option<String>,
    },
    Anchor {
        id: String,
        text: String,
        #[serde(rename = "anchorId", skip_serializing_if = "Option::is_none")]
        anchor_id: Option<String>,
    },
}

impl CanonicalBlock {
    #[must_use]
    pub const fn kind(&self) -> BlockKind {
        match self {
            Self::Heading { .. } => BlockKind::Heading,
            Self::Paragraph { .. } => BlockKind::Paragraph,
            Self::Blockquote { .. } => BlockKind::Blockquote,
            Self::ListItem { .. } => BlockKind::ListItem,
            Self::SceneBreak { .. } => BlockKind::SceneBreak,
            Self::Image { .. } => BlockKind::Image,
            Self::Anchor { .. } => BlockKind::Anchor,
        }
    }
}

/// Structural meaning extracted from an ebook source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Heading,
    Paragraph,
    Blockquote,
    ListItem,
    SceneBreak,
    Image,
    Anchor,
}

/// A reading-order chapter, including nested table-of-contents depth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalChapter {
    pub id: String,
    pub index: usize,
    pub title: String,
    pub href: String,
    pub depth: usize,
    pub blocks: Vec<CanonicalBlock>,
    pub word_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDiagnostic {
    pub severity: DiagnosticSeverity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

/// Format-independent book consumed by storage and presentation adapters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalBook {
    pub id: String,
    pub title: String,
    pub author: String,
    pub source_path: String,
    pub import_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parser_version: Option<u32>,
    pub diagnostics: Vec<ImportDiagnostic>,
    pub chapters: Vec<CanonicalChapter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_path: Option<String>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{BlockKind, CanonicalBlock};

    #[test]
    fn canonical_block_preserves_the_v1_json_contract() {
        let block = CanonicalBlock::Heading {
            id: "chapter-1:block-0".into(),
            text: "Read this".into(),
            level: Some(2),
        };

        assert_eq!(
            serde_json::to_value(block).expect("canonical block should serialize"),
            json!({
                "id": "chapter-1:block-0",
                "type": "heading",
                "text": "Read this",
                "level": 2
            })
        );
    }

    #[test]
    fn canonical_block_accepts_optional_v1_fields() {
        let block: CanonicalBlock = serde_json::from_value(json!({
            "id": "chapter-1:image-0",
            "type": "image",
            "text": "Page 1",
            "imageSource": "images/001.jpg"
        }))
        .expect("v1 canonical JSON should deserialize");

        assert_eq!(block.kind(), BlockKind::Image);
        assert!(matches!(
            block,
            CanonicalBlock::Image {
                image_source: Some(source),
                ..
            } if source == "images/001.jpg"
        ));
    }
}
