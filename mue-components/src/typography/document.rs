//! Read-only rich text and a CommonMark adapter for cosmic-text.
// Preserve the original public document module's Markdown entry points.
pub use super::markdown::{MarkdownBuilder, markdown};
use cosmic_text::{FamilyOwned, Weight};
use mue_core::prop::{Prop, PropValue};
use mue_macroquad::style::Style;
use pulldown_cmark::{Event, Parser, Tag, TagEnd};

pub use mue_macroquad::node::text::{Span, SpanStyle};

/// Component-level typography helpers for the renderer's text spans.
pub trait SpanExt {
    /// Apply the same proportions as the standalone heading component.
    fn heading(self, level: super::heading::HeadingLevel) -> Self;
}

impl SpanExt for Span {
    fn heading(mut self, level: super::heading::HeadingLevel) -> Self {
        self.style.weight = Some(Weight::BOLD);
        self.style.scale = level.scale();
        self
    }
}

/// Newlines delimit paragraphs. Blank lines add paragraph spacing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Document {
    pub spans: Vec<Span>,
}

impl PropValue for Document {}

/// Content accepted by the document node, including reactive documents.
pub trait IntoDocumentProp {
    fn into_document_prop(self) -> Prop<Document>;
}

impl IntoDocumentProp for Document {
    fn into_document_prop(self) -> Prop<Document> {
        Prop::Static(self)
    }
}
impl IntoDocumentProp for &str {
    fn into_document_prop(self) -> Prop<Document> {
        Document::plain(self).into()
    }
}
impl IntoDocumentProp for String {
    fn into_document_prop(self) -> Prop<Document> {
        Document::plain(self).into()
    }
}
impl IntoDocumentProp for Prop<Document> {
    fn into_document_prop(self) -> Prop<Document> {
        self
    }
}
impl IntoDocumentProp for mue_core::signal::ReadSignal<Document> {
    fn into_document_prop(self) -> Prop<Document> {
        self.into()
    }
}
impl IntoDocumentProp for mue_core::signal::Signal<Document> {
    fn into_document_prop(self) -> Prop<Document> {
        (*self).into()
    }
}

impl Document {
    /// Join document blocks with a blank line, retaining each run's style.
    pub fn blocks(blocks: impl IntoIterator<Item = Document>) -> Self {
        let mut spans = Vec::new();
        for block in blocks {
            if block.spans.is_empty() {
                continue;
            }
            if !spans.is_empty() {
                spans.push(Span::new("\n\n"));
            }
            spans.extend(block.spans);
        }
        Self { spans }
    }

    pub fn heading(text: impl Into<String>, level: super::heading::HeadingLevel) -> Self {
        Self::rich([Span::new(text).heading(level)])
    }

    pub fn code(text: impl Into<String>) -> Self {
        Self::rich([Span::new(text).monospace()])
    }

    pub fn em(text: impl Into<String>) -> Self {
        Self::rich([Span::new(text).italic()])
    }

    /// A rich-text quotation. Use the standalone node for a painted rule.
    pub fn blockquote(text: impl Into<String>) -> Self {
        Self::em(text)
    }

    /// A text list using the same marker labels as the standalone list node.
    pub fn list(
        items: impl IntoIterator<Item = Document>,
        marker: super::list::ListMarker,
    ) -> Self {
        let mut spans = Vec::new();
        for (index, item) in items.into_iter().enumerate() {
            if index > 0 {
                spans.push(Span::new("\n"));
            }
            let label = marker.label(index);
            if !label.is_empty() {
                spans.push(Span::new(format!("{label} ")));
            }
            spans.extend(item.spans);
        }
        Self { spans }
    }

    /// Create unstyled text using the document node's font defaults.
    pub fn plain(text: impl Into<String>) -> Self {
        Self::rich([Span::new(text)])
    }
    /// Combine styled spans; newlines separate paragraphs.
    pub fn rich(spans: impl IntoIterator<Item = Span>) -> Self {
        Self {
            spans: spans.into_iter().collect(),
        }
    }

    /// CommonMark paragraphs, headings, emphasis, strong text, code, quotes and
    /// lists. Links/images display their labels; HTML is displayed as literal text.
    /// This is read-only text, without links, images, tables or syntax highlighting.
    pub fn markdown(source: &str) -> Self {
        let mut spans = Vec::new();
        let mut style = SpanStyle::default();
        let mut stack = Vec::new();
        let mut lists: Vec<Option<u64>> = Vec::new();
        let mut pending_break = 0;
        for event in Parser::new(source) {
            let mut text = None;
            match event {
                Event::Start(tag) => {
                    stack.push(style.clone());
                    match tag {
                        Tag::Strong => style.weight = Some(Weight::BOLD),
                        Tag::Emphasis => style.italic = true,
                        Tag::Heading { level, .. } => {
                            style.weight = Some(Weight::BOLD);
                            use super::heading::HeadingLevel;
                            style.scale = match level as u8 {
                                1 => HeadingLevel::H1,
                                2 => HeadingLevel::H2,
                                3 => HeadingLevel::H3,
                                4 => HeadingLevel::H4,
                                5 => HeadingLevel::H5,
                                _ => HeadingLevel::H6,
                            }
                            .scale();
                        }
                        Tag::CodeBlock(_) => style.family = Some(FamilyOwned::Monospace),
                        Tag::BlockQuote(_) => style.italic = true,
                        Tag::List(start) => lists.push(start),
                        Tag::Item => {
                            text = Some(match lists.last_mut() {
                                Some(Some(n)) => {
                                    let label = format!(
                                        "{} ",
                                        super::list::ListMarker::Ordered(*n).label(0)
                                    );
                                    *n = n.saturating_add(1);
                                    label
                                }
                                _ => format!("{} ", super::list::ListMarker::Bullet.label(0)),
                            });
                        }
                        _ => {}
                    }
                }
                Event::End(tag) => {
                    style = stack.pop().unwrap_or_default();
                    match tag {
                        TagEnd::Paragraph
                        | TagEnd::Heading(_)
                        | TagEnd::CodeBlock
                        | TagEnd::BlockQuote(_) => {
                            pending_break = if lists.is_empty() { 2 } else { 1 };
                        }
                        TagEnd::Item => pending_break = 1,
                        TagEnd::List(_) => {
                            lists.pop();
                            pending_break = 2;
                        }
                        _ => {}
                    }
                }
                Event::Text(value) | Event::Html(value) | Event::InlineHtml(value) => {
                    text = Some(value.into_string())
                }
                Event::Code(value) => {
                    if pending_break > 0 && !spans.is_empty() {
                        spans.push(Span::new("\n".repeat(pending_break)));
                    }
                    pending_break = 0;
                    let mut code = style.clone();
                    code.family = Some(FamilyOwned::Monospace);
                    spans.push(Span {
                        text: value.into_string(),
                        style: code,
                    });
                }
                Event::SoftBreak => text = Some(" ".into()),
                Event::HardBreak => text = Some("\n".into()),
                Event::Rule => pending_break = 2,
                _ => {}
            }
            if let Some(text) = text {
                if pending_break > 0 && !spans.is_empty() {
                    spans.push(Span::new("\n".repeat(pending_break)));
                }
                pending_break = 0;
                spans.push(Span {
                    text,
                    style: style.clone(),
                });
            }
        }
        Self { spans }
    }
}

/// Display wrapping rich text. Paragraphs are separated by newlines.
/// Font and layout styles supply defaults for spans without overrides.
pub fn document(content: impl IntoDocumentProp) -> DocumentViewBuilder {
    document_view(content.into_document_prop())
}

#[mue_macros::node]
fn document_view(style: &mut Style, content: Document) {
    super::text::render_text(style, content);
}
