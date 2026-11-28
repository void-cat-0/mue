use super::controls::*;
use cosmic_text::{Align, FamilyOwned, Weight};
use mue_components::*;
use mue_macroquad::style::{Styleable, StyleableExt};

const PAGE: &str = "Typography";

const SAMPLE: &str = "The quick brown fox jumps over the lazy dog. 敏捷的棕色狐狸跳过了懒狗。 supercalifragilisticexpialidocious\nA second paragraph shows paragraph spacing, line limits, and ellipsis on long text that keeps going.";

fn layout_card() -> Node {
    let (wrap_mode, wrap_control) = segmented(
        "Wrap",
        vec![
            ("Word or glyph", TextWrap::WordOrGlyph),
            ("Word", TextWrap::Word),
            ("Glyph", TextWrap::Glyph),
        ],
    );
    let (overflow, overflow_control) = segmented(
        "Overflow",
        vec![
            ("Visible", TextOverflow::Visible),
            ("Clip", TextOverflow::Clip),
            ("… end", TextOverflow::EllipsisEnd),
            ("… middle", TextOverflow::EllipsisMiddle),
            ("… start", TextOverflow::EllipsisStart),
        ],
    );
    let (lines, lines_control) = segmented(
        "Max lines",
        vec![
            ("All", None),
            ("1", Some(1usize)),
            ("2", Some(2)),
            ("3", Some(3)),
        ],
    );
    let (align, align_control) = segmented(
        "Align",
        vec![
            ("Left", Some(Align::Left)),
            ("Center", Some(Align::Center)),
            ("Right", Some(Align::Right)),
            ("Justify", Some(Align::Justified)),
        ],
    );
    let (vertical, vertical_control) = segmented(
        "Vertical",
        vec![
            ("Top", TextVerticalAlign::Top),
            ("Center", TextVerticalAlign::Center),
            ("Bottom", TextVerticalAlign::Bottom),
        ],
    );
    let (family, family_control) = segmented(
        "Family",
        vec![
            ("Sans", FamilyOwned::SansSerif),
            ("Serif", FamilyOwned::Serif),
            ("Mono", FamilyOwned::Monospace),
        ],
    );
    let (size, size_control) = segmented("Size", vec![("15", 15.), ("20", 20.), ("28", 28.)]);
    let (bold, bold_control) = option_switch("Bold", false);
    let (italic, italic_control) = option_switch("Italic", false);
    let (underline, underline_control) = option_switch("Underline", false);
    let (strike, strike_control) = option_switch("Strikethrough", false);
    let (spacing, spacing_control) = option_switch("Letter spacing", false);
    let (paragraphs, paragraph_control) = option_switch("Paragraph spacing", false);
    card(
        "Text layout",
        (
            wrap_control,
            overflow_control,
            lines_control,
            align_control,
            vertical_control,
            family_control,
            size_control,
            row((bold_control, italic_control, underline_control)),
            row((strike_control, spacing_control, paragraph_control)),
            text(SAMPLE)
                .style(mue_macroquad::style! {
                    width: 100%; min-width: 0px;
                    height: {u(190.)};
                    padding: {(u(12.),)};
                    border-radius: {u(10.)};
                    background: {raised()};
                    color: {foreground()};
                    text-wrap-mode: {wrap_mode};
                    text-max-lines: {lines};
                    text-align: {align};
                    text-vertical-align: {vertical};
                    font-family: {family};
                    font-italic: {italic};
                    text-underline: {underline};
                    text-strikethrough: {strike};
                    font-weight: {bold.select(Weight::BOLD, Weight::NORMAL)};
                    font-size: {size.map(|size| mue_macroquad::style::FontSize::Rem(size / 16.))};
                    line-height: 1.45;
                    letter-spacing: {super::responsive::ui(spacing.select(2., 0.))};
                    paragraph-spacing: {super::responsive::ui(paragraphs.select(14., 0.))};
                })
                .text_overflow(overflow),
        ),
    )
}

fn scripts_card() -> Node {
    let sample = |label: &'static str, content: &'static str, rtl: bool| {
        hstack()
            .style(mue_macroquad::style! {
                width: 100%; min-width: 0px;
                gap: {u(12.)};
                padding: {(u(10.), u(6.))};
                border-radius: {u(8.)};
                background: {raised()};
                align-items: center;
            })
            .children((
                text(label).style(mue_macroquad::style! {
                    flex: none; width: {u(84.)};
                    font-size: {f(12.)}; line-height: {u(18.)};
                    color: {muted()};
                }),
                text(content).style(if rtl {
                    mue_macroquad::style! {
                        flex: 1; min-width: 0px;
                        font-size: {f(18.)}; line-height: {u(28.)};
                        text-align: right;
                        color: {foreground()};
                    }
                } else {
                    mue_macroquad::style! {
                        flex: 1; min-width: 0px;
                        font-size: {f(18.)}; line-height: {u(28.)};
                        color: {foreground()};
                    }
                }),
            ))
            .into_node()
    };
    card(
        "Scripts and font fallback",
        stack()
            .style(mue_macroquad::style! { width: 100%; min-width: 0px; gap: {u(6.)}; })
            .children((
                sample(
                    "Latin",
                    "Sphinx of black quartz, judge my vow. 0123456789",
                    false,
                ),
                sample(
                    "Greek, Cyrillic",
                    "Ζεύς · Съешь же ещё этих мягких булок",
                    false,
                ),
                sample("Chinese", "你好，世界。排版、换行与标点。", false),
                sample("Japanese", "いろはにほへと カタカナ 漢字", false),
                sample("Korean", "다람쥐 헌 쳇바퀴에 타고파", false),
                sample(
                    "Arabic (RTL)",
                    "مرحبا بالعالم — سطر من اليمين إلى اليسار",
                    true,
                ),
                sample("Mixed bidi", "Price: 42 درهم (about ¥80) today", false),
                sample("Symbols", "← → ↑ ↓ ✓ ✗ ★ ♪ ⌘ ⌥ ∞ ≠ ≤ ≥", false),
            )),
    )
}

fn headings_card() -> Node {
    let level = |level: HeadingLevel, name: &'static str, size: f32| {
        heading(name).level(level).style(mue_macroquad::style! {
            font-size: {f(size)};
            line-height: {u(size * 1.3)};
            color: {foreground()};
        })
    };
    card(
        "Headings",
        stack()
            .style(mue_macroquad::style! { width: 100%; min-width: 0px; gap: {u(4.)}; })
            .children((
                level(HeadingLevel::H1, "Heading 1", 32.),
                level(HeadingLevel::H2, "Heading 2", 26.),
                level(HeadingLevel::H3, "Heading 3", 22.),
                level(HeadingLevel::H4, "Heading 4", 18.),
                level(HeadingLevel::H5, "Heading 5", 15.),
                level(HeadingLevel::H6, "Heading 6", 13.),
            )),
    )
}

fn rich_card() -> Node {
    let (marker, marker_control) = segmented(
        "List marker",
        vec![
            ("Bullet", ListMarker::Bullet),
            ("Ordered", ListMarker::Ordered(1)),
            ("None", ListMarker::None),
        ],
    );
    let (tabs, tabs_control) = segmented("Tab width", vec![("4", 4u16), ("2", 2), ("8", 8)]);
    let body = || {
        mue_macroquad::style! {
            font-size: {f(15.)}; line-height: {u(22.)};
            color: {foreground()};
        }
    };
    card(
        "Rich text",
        (
            row((
                code("let answer = 42;").style(mue_macroquad::style! {
                    font-size: {f(14.)}; line-height: {u(22.)};
                    padding: {(u(6.), u(2.))};
                    border-radius: {u(4.)};
                }),
                em("Emphasis").style(body()),
                link("A link").style(body()).on_activate(|| log_event("link activated")),
            )),
            blockquote("Somewhere, something incredible is waiting to be known.").style(
                mue_macroquad::style! {
                    font-size: {f(16.)}; line-height: {u(24.)};
                    padding-left: {u(14.)};
                    color: {foreground()};
                },
            ),
            tabs_control,
            code_block("fn main() {\n\tlet greeting = \"hello\";\n\tprintln!(\"{greeting}\");\n}")
                .tab_width(tabs)
                .style(mue_macroquad::style! {
                    width: 100%;
                    font-size: {f(14.)}; line-height: {u(21.)};
                    padding: {(u(12.),)};
                    border-radius: {u(6.)};
                }),
            marker_control,
            list().marker(marker).style(mue_macroquad::style! {
                gap: {u(4.)};
                font-size: {f(15.)}; line-height: {u(22.)};
            }).children((
                text("First item").style(body()),
                text("Second item").style(body()),
                list_item().children(text("Third item").style(body())),
            )),
            document(Document::blocks([
                Document::heading("Document", HeadingLevel::H4),
                Document::rich([
                    Span::new("Rich "),
                    Span::new("spans").bold(),
                    Span::new(" with "),
                    Span::new("code").monospace(),
                ]),
                Document::list(
                    [Document::plain("Plain"), Document::em("Emphasized")],
                    ListMarker::Ordered(1),
                ),
            ]))
            .style(body()),
            markdown("#### Markdown\n\n**Bold**, *italic*, `code`, and [a link](https://example.com).\n\n> A quote.\n\n- one\n- two")
                .style(body()),
        ),
    )
}

pub fn page() -> Node {
    page_body(
        (
            layout_card(),
            scripts_card(),
            headings_card(),
            rich_card(),
            checklist(
                PAGE,
                &[
                    "Every script renders without missing-glyph boxes",
                    "Arabic runs right to left, the mixed line in order",
                    "Each ellipsis mode cuts at its end, middle, or start",
                    "Bold, italic, underline, and strikethrough render",
                ],
            ),
        ),
    )
}
