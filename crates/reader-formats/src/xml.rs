//! Minimal XML reading for EPUB metadata documents.
//!
//! v1 parsed OPF, container, and NCX files into untyped objects with
//! `fast-xml-parser` and then reached into them. Here they are read into small
//! typed structures with `quick-xml`, which makes the fallbacks explicit:
//! namespace prefixes are ignored, attribute and element names are matched on
//! their local part, and anything unrecognized is skipped rather than fatal.

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

/// The XML document could not be read.
#[derive(Debug)]
pub struct XmlError {
    pub detail: String,
}

impl std::fmt::Display for XmlError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.detail)
    }
}

impl std::error::Error for XmlError {}

impl From<quick_xml::Error> for XmlError {
    fn from(error: quick_xml::Error) -> Self {
        Self {
            detail: error.to_string(),
        }
    }
}

/// Whether a qualified XML name has the requested local part.
///
/// EPUB names are ASCII. Comparing their borrowed bytes avoids allocating and
/// lowercasing a `String` for every start and end event in the document.
fn has_local_name(name: &[u8], wanted: &[u8]) -> bool {
    name.rsplit(|byte| *byte == b':')
        .next()
        .is_some_and(|local| local.eq_ignore_ascii_case(wanted))
}

/// An attribute value, matched on its local name.
fn attribute(start: &BytesStart<'_>, wanted: &str) -> Option<String> {
    start.attributes().flatten().find_map(|attribute| {
        if has_local_name(attribute.key.as_ref(), wanted.as_bytes()) {
            attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .ok()
                .map(|value| value.trim().to_owned())
        } else {
            None
        }
    })
}

/// A reader configured the way EPUB documents in the wild need: whitespace-only
/// text dropped, and mismatched end tags tolerated rather than fatal.
fn reader(source: &str) -> Reader<&[u8]> {
    let mut reader = Reader::from_str(source);
    let config = reader.config_mut();
    config.trim_text(true);
    config.check_end_names = false;
    reader
}

/// The OPF path named by `META-INF/container.xml`.
pub fn parse_container(source: &str) -> Result<String, XmlError> {
    let mut reader = reader(source);
    loop {
        match reader.read_event()? {
            Event::Start(start) | Event::Empty(start) => {
                if has_local_name(start.name().as_ref(), b"rootfile")
                    && let Some(path) = attribute(&start, "full-path")
                {
                    return Ok(path);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Err(XmlError {
        detail: "container.xml does not name a root file".to_owned(),
    })
}

/// One `<item>` of the OPF manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestItem {
    pub id: String,
    pub href: String,
    pub media_type: Option<String>,
    pub properties: Option<String>,
}

/// The parts of the OPF package the importer needs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Package {
    pub title: Option<String>,
    pub creator: Option<String>,
    pub manifest: Vec<ManifestItem>,
    /// `idref`s in spine order.
    pub spine: Vec<String>,
    /// The `toc` attribute of `<spine>`, naming the NCX item.
    pub toc_id: Option<String>,
}

impl Package {
    /// The manifest item with this id.
    #[must_use]
    pub fn item(&self, id: &str) -> Option<&ManifestItem> {
        self.manifest.iter().find(|item| item.id == id)
    }

    /// The EPUB3 navigation document, if the manifest declares one.
    #[must_use]
    pub fn nav_item(&self) -> Option<&ManifestItem> {
        self.manifest.iter().find(|item| {
            item.properties
                .as_deref()
                .is_some_and(|properties| properties.contains("nav"))
        })
    }

    /// The NCX item, named by the spine or found by media type.
    #[must_use]
    pub fn ncx_item(&self) -> Option<&ManifestItem> {
        self.toc_id
            .as_deref()
            .and_then(|id| self.item(id))
            .or_else(|| {
                self.manifest.iter().find(|item| {
                    item.media_type
                        .as_deref()
                        .is_some_and(|media| media.contains("ncx"))
                })
            })
    }
}

/// Parse an OPF package document.
pub fn parse_package(source: &str) -> Result<Package, XmlError> {
    let mut reader = reader(source);
    let mut package = Package::default();
    // Metadata text is captured by remembering which element is open.
    #[derive(Clone, Copy)]
    enum MetadataField {
        Title,
        Creator,
    }
    let mut capturing: Option<MetadataField> = None;

    loop {
        match reader.read_event()? {
            Event::Start(start) => {
                let name = start.name();
                let name = name.as_ref();
                if has_local_name(name, b"title") {
                    capturing = Some(MetadataField::Title);
                } else if has_local_name(name, b"creator") {
                    capturing = Some(MetadataField::Creator);
                } else if has_local_name(name, b"item") {
                    push_item(&mut package, &start);
                } else if has_local_name(name, b"itemref") {
                    push_itemref(&mut package, &start);
                } else if has_local_name(name, b"spine") {
                    package.toc_id = attribute(&start, "toc");
                }
            }
            Event::Empty(start) => {
                let name = start.name();
                let name = name.as_ref();
                if has_local_name(name, b"item") {
                    push_item(&mut package, &start);
                } else if has_local_name(name, b"itemref") {
                    push_itemref(&mut package, &start);
                } else if has_local_name(name, b"spine") {
                    package.toc_id = attribute(&start, "toc");
                }
            }
            Event::Text(text) => {
                if let Some(field) = capturing {
                    let value = text.decode().unwrap_or_default().trim().to_owned();
                    if !value.is_empty() {
                        match field {
                            MetadataField::Title if package.title.is_none() => {
                                package.title = Some(value);
                            }
                            MetadataField::Creator if package.creator.is_none() => {
                                package.creator = Some(value);
                            }
                            _ => {}
                        }
                    }
                }
            }
            Event::End(_) => capturing = None,
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(package)
}

fn push_item(package: &mut Package, start: &BytesStart<'_>) {
    let (Some(id), Some(href)) = (attribute(start, "id"), attribute(start, "href")) else {
        return;
    };
    package.manifest.push(ManifestItem {
        id,
        href,
        media_type: attribute(start, "media-type"),
        properties: attribute(start, "properties"),
    });
}

fn push_itemref(package: &mut Package, start: &BytesStart<'_>) {
    if let Some(idref) = attribute(start, "idref") {
        package.spine.push(idref);
    }
}

/// One NCX navigation point, flattened.
#[derive(Debug, Clone, PartialEq)]
pub struct NcxEntry {
    pub label: String,
    pub href: String,
    pub depth: usize,
    /// `playOrder`, or infinity when absent so unordered points sort last.
    pub play_order: f64,
}

/// Parse an NCX navigation map into depth-tagged entries in document order.
pub fn parse_ncx(source: &str) -> Result<Vec<NcxEntry>, XmlError> {
    let mut reader = reader(source);
    let mut entries: Vec<NcxEntry> = Vec::new();

    // A nav point is only emitted once its `content` src is known, so the
    // pending stack holds partially built entries.
    struct Pending {
        depth: usize,
        play_order: f64,
        label: Option<String>,
        href: Option<String>,
    }
    let mut stack: Vec<Pending> = Vec::new();
    let mut in_label_text = false;

    loop {
        match reader.read_event()? {
            Event::Start(start) => {
                let name = start.name();
                let name = name.as_ref();
                if has_local_name(name, b"navpoint") {
                    let depth = stack.len();
                    stack.push(Pending {
                        depth,
                        play_order: attribute(&start, "playorder")
                            .and_then(|value| value.parse().ok())
                            .unwrap_or(f64::INFINITY),
                        label: None,
                        href: None,
                    });
                } else if has_local_name(name, b"text") {
                    in_label_text = true;
                } else if has_local_name(name, b"content") {
                    if let (Some(current), Some(src)) = (stack.last_mut(), attribute(&start, "src"))
                    {
                        current.href = Some(src);
                    }
                }
            }
            Event::Empty(start) => {
                if has_local_name(start.name().as_ref(), b"content")
                    && let (Some(current), Some(src)) = (stack.last_mut(), attribute(&start, "src"))
                {
                    current.href = Some(src);
                }
            }
            Event::Text(text) => {
                if in_label_text
                    && let Some(current) = stack.last_mut()
                    && current.label.is_none()
                {
                    let value = text.decode().unwrap_or_default().trim().to_owned();
                    if !value.is_empty() {
                        current.label = Some(value);
                    }
                }
            }
            Event::End(end) => {
                if has_local_name(end.name().as_ref(), b"text") {
                    in_label_text = false;
                } else if has_local_name(end.name().as_ref(), b"navpoint")
                    && let Some(current) = stack.pop()
                    && let Some(href) = current.href
                {
                    entries.push(NcxEntry {
                        label: current
                            .label
                            .unwrap_or_else(|| "Untitled chapter".to_owned()),
                        href,
                        depth: current.depth,
                        play_order: current.play_order,
                    });
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    // Nested points close before their parents, so restore document order.
    entries.sort_by(|left, right| {
        left.play_order
            .partial_cmp(&right.play_order)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::{parse_container, parse_ncx, parse_package};

    #[test]
    fn container_names_the_package_document() {
        let path = parse_container(
            r#"<?xml version="1.0"?>
            <container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
              <rootfiles>
                <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
              </rootfiles>
            </container>"#,
        )
        .expect("container should parse");
        assert_eq!(path, "OEBPS/content.opf");
    }

    #[test]
    fn a_container_without_a_root_file_is_an_error() {
        assert!(parse_container("<container></container>").is_err());
    }

    #[test]
    fn the_package_yields_metadata_manifest_and_spine() {
        let package = parse_package(
            r#"<?xml version="1.0"?>
            <package version="3.0" xmlns="http://www.idpf.org/2007/opf">
              <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
                <dc:title>Fixture Book</dc:title>
                <dc:creator opf:role="aut">Fixture Author</dc:creator>
              </metadata>
              <manifest>
                <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
                <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
                <item id="ch1" href="text/ch1.xhtml" media-type="application/xhtml+xml"/>
              </manifest>
              <spine toc="ncx">
                <itemref idref="ch1"/>
                <itemref idref="nav" linear="no"/>
              </spine>
            </package>"#,
        )
        .expect("package should parse");

        assert_eq!(package.title.as_deref(), Some("Fixture Book"));
        assert_eq!(package.creator.as_deref(), Some("Fixture Author"));
        assert_eq!(package.manifest.len(), 3);
        assert_eq!(package.spine, vec!["ch1", "nav"]);
        assert_eq!(package.toc_id.as_deref(), Some("ncx"));
        assert_eq!(
            package.nav_item().map(|item| item.href.as_str()),
            Some("nav.xhtml")
        );
        assert_eq!(
            package.ncx_item().map(|item| item.href.as_str()),
            Some("toc.ncx")
        );
        assert_eq!(
            package.item("ch1").map(|item| item.href.as_str()),
            Some("text/ch1.xhtml")
        );
    }

    #[test]
    fn the_ncx_item_is_found_by_media_type_when_the_spine_is_silent() {
        let package = parse_package(
            r#"<package><manifest>
                 <item id="toc" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
               </manifest><spine><itemref idref="toc"/></spine></package>"#,
        )
        .expect("package should parse");
        assert!(package.toc_id.is_none());
        assert_eq!(
            package.ncx_item().map(|item| item.href.as_str()),
            Some("toc.ncx")
        );
    }

    #[test]
    fn a_package_without_metadata_yields_no_title_or_creator() {
        let package =
            parse_package("<package><manifest/><spine/></package>").expect("package should parse");
        assert!(package.title.is_none());
        assert!(package.creator.is_none());
        assert!(package.manifest.is_empty());
        assert!(package.spine.is_empty());
    }

    #[test]
    fn ncx_navigation_points_flatten_with_depth_and_play_order() {
        let entries = parse_ncx(
            r#"<?xml version="1.0"?>
            <ncx xmlns="http://www.daisy.org/z3986/2005/ncx/">
              <navMap>
                <navPoint id="p1" playOrder="1">
                  <navLabel><text>Chapter One</text></navLabel>
                  <content src="text/ch1.xhtml"/>
                  <navPoint id="p1a" playOrder="2">
                    <navLabel><text>Section</text></navLabel>
                    <content src="text/ch1.xhtml#s1"/>
                  </navPoint>
                </navPoint>
                <navPoint id="p2" playOrder="3">
                  <navLabel><text>Chapter Two</text></navLabel>
                  <content src="text/ch2.xhtml"/>
                </navPoint>
              </navMap>
            </ncx>"#,
        )
        .expect("ncx should parse");

        assert_eq!(
            entries
                .iter()
                .map(|entry| (entry.label.as_str(), entry.href.as_str(), entry.depth))
                .collect::<Vec<_>>(),
            vec![
                ("Chapter One", "text/ch1.xhtml", 0),
                ("Section", "text/ch1.xhtml#s1", 1),
                ("Chapter Two", "text/ch2.xhtml", 0),
            ]
        );
    }

    #[test]
    fn nav_points_without_content_are_skipped_and_missing_labels_get_a_fallback() {
        let entries = parse_ncx(
            r#"<ncx><navMap>
                 <navPoint playOrder="1"><navLabel><text>No content</text></navLabel></navPoint>
                 <navPoint playOrder="2"><content src="a.xhtml"/></navPoint>
               </navMap></ncx>"#,
        )
        .expect("ncx should parse");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].label, "Untitled chapter");
    }
}
