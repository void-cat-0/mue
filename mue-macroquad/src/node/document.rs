//! Read-only rich text and a CommonMark adapter for cosmic-text.
use crate::style::Style;
use cosmic_text::{Attrs, FamilyOwned, Metrics, Style as FontStyle, Weight};
use mue_core::prop::{Prop, PropValue};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use std::rc::Rc;

/// Optional span attributes inherit the document node's defaults.
#[derive(Clone, Debug, PartialEq)]
pub struct SpanStyle {
    pub family: Option<FamilyOwned>,
    pub weight: Option<Weight>,
    pub italic: bool,
    /// Relative to the node's font size and line height.
    pub scale: f32,
    pub color: Option<cosmic_text::Color>,
}

impl Default for SpanStyle {
    fn default() -> Self {
        Self {
            family: None,
            weight: None,
            italic: false,
            scale: 1.,
            color: None,
        }
    }
}

impl SpanStyle {
    pub fn attrs<'a>(&'a self, defaults: &Attrs<'a>, metrics: Metrics) -> Attrs<'a> {
        let mut attrs = defaults.clone();
        if let Some(family) = &self.family {
            attrs = attrs.family(family.as_family());
        }
        if let Some(weight) = self.weight {
            attrs = attrs.weight(weight);
        }
        if self.italic {
            attrs = attrs.style(FontStyle::Italic);
        }
        if let Some(color) = self.color {
            attrs = attrs.color(color);
        }
        if self.scale > 0. && self.scale != 1. {
            attrs = attrs.metrics(metrics.scale(self.scale));
        }
        attrs
    }
}

/// A text run with optional font and color overrides.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Span {
    pub text: String,
    pub style: SpanStyle,
}

impl Span {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: SpanStyle::default(),
        }
    }
    pub fn bold(mut self) -> Self {
        self.style.weight = Some(Weight::BOLD);
        self
    }
    pub fn italic(mut self) -> Self {
        self.style.italic = true;
        self
    }
    pub fn monospace(mut self) -> Self {
        self.style.family = Some(FamilyOwned::Monospace);
        self
    }
    pub fn color(mut self, color: cosmic_text::Color) -> Self {
        self.style.color = Some(color);
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
                            style.scale = match level as u8 {
                                1 => 1.8,
                                2 => 1.45,
                                _ => 1.2,
                            };
                        }
                        Tag::CodeBlock(_) => style.family = Some(FamilyOwned::Monospace),
                        Tag::BlockQuote(_) => style.italic = true,
                        Tag::List(start) => lists.push(start),
                        Tag::Item => {
                            text = Some(match lists.last_mut() {
                                Some(Some(n)) => {
                                    let label = format!("{n}. ");
                                    *n += 1;
                                    label
                                }
                                _ => "• ".into(),
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

/// Display CommonMark as read-only rich text with reactive content and styles.
#[mue_macros::node]
pub fn markdown(style: &mut Style, source: Rc<str>) {
    super::text::render_text(style, source.map(|source| Document::markdown(&source)));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn markdown_preserves_paragraphs_and_nested_styles() {
        let doc = Document::markdown(
            "# Field notes\n\nA **bold and *italic*** word with `code`.\n\nNext paragraph.",
        );
        let text: String = doc.spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(
            text,
            "Field notes\n\nA bold and italic word with code.\n\nNext paragraph."
        );
        assert_eq!(doc.spans[0].style.scale, 1.8);
        let italic = doc.spans.iter().find(|s| s.text == "italic").unwrap();
        assert!(italic.style.italic);
        assert_eq!(italic.style.weight, Some(Weight::BOLD));
        let code = doc.spans.iter().find(|s| s.text == "code").unwrap();
        assert_eq!(code.style.family, Some(FamilyOwned::Monospace));
    }
    #[test]
    fn lists_links_and_unicode_remain_readable() {
        let doc = Document::markdown("3. 日本語\n4. [مرحبا](https://example.com)\n\nFinal");
        let text: String = doc.spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(text, "3. 日本語\n4. مرحبا\n\nFinal");
    }
}
