use serde::{Deserialize, Serialize};

/// Structural meaning extracted from an ebook source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BlockKind {
    Heading,
    Paragraph,
    Blockquote,
    ListItem,
    SceneBreak,
    Image,
    Anchor,
}

/// Smallest canonical unit rendered by plain and stealth modes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalBlock {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: BlockKind,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor_id: Option<String>,
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
        let block = CanonicalBlock {
            id: "chapter-1:block-0".into(),
            kind: BlockKind::ListItem,
            text: "Read this".into(),
            level: Some(2),
            image_source: None,
            anchor_id: None,
        };

        assert_eq!(
            serde_json::to_value(block).expect("canonical block should serialize"),
            json!({
                "id": "chapter-1:block-0",
                "type": "list-item",
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

        assert_eq!(block.kind, BlockKind::Image);
        assert_eq!(block.image_source.as_deref(), Some("images/001.jpg"));
        assert_eq!(block.level, None);
    }
}
