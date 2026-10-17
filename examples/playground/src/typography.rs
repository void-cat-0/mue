use super::{controls::*, responsive};
use cosmic_text::{Align, FamilyOwned, Weight};
use mue_macroquad::{
    node::*,
    style::{Styleable, StyleableExt, vh, vw},
};

pub fn page() -> Node {
    let (wrap_mode, a) = choice(
        "Wrap",
        vec![
            ("WordOrGlyph", TextWrap::WordOrGlyph),
            ("Word", TextWrap::Word),
            ("Glyph", TextWrap::Glyph),
        ],
    );
    let (overflow, b) = choice(
        "Overflow",
        vec![
            ("Visible", TextOverflow::Visible),
            ("Clip", TextOverflow::Clip),
            ("End", TextOverflow::EllipsisEnd),
            ("Middle", TextOverflow::EllipsisMiddle),
            ("Start", TextOverflow::EllipsisStart),
        ],
    );
    let (align, c) = choice(
        "Align",
        vec![
            ("Left", Some(Align::Left)),
            ("Center", Some(Align::Center)),
            ("Right", Some(Align::Right)),
            ("Justified", Some(Align::Justified)),
            ("End", Some(Align::End)),
        ],
    );
    let (vertical, d) = choice(
        "Vertical",
        vec![
            ("Top", TextVerticalAlign::Top),
            ("Center", TextVerticalAlign::Center),
            ("Bottom", TextVerticalAlign::Bottom),
        ],
    );
    let (family, e) = choice(
        "Family",
        vec![
            ("Sans", FamilyOwned::SansSerif),
            ("Serif", FamilyOwned::Serif),
            ("Mono", FamilyOwned::Monospace),
        ],
    );
    let (limit, f) = choice(
        "Lines",
        vec![("All", None), ("1", Some(1usize)), ("3", Some(3usize))],
    );
    let (size, g) = toggle("Size");
    let (leading, h) = toggle("Line height");
    let (spacing, i) = toggle("Letter spacing");
    let (paragraphs, j) = toggle("Paragraph spacing");
    let (italic, k) = toggle("Italic");
    let (bold, l) = toggle("Bold");
    let (underline, m) = toggle("Underline");
    let (strike, n) = toggle("Strike");
    let (nowrap, o) = toggle("No wrap");
    let (level, p) = choice(
        "Heading",
        vec![
            ("H1", HeadingLevel::H1),
            ("H2", HeadingLevel::H2),
            ("H3", HeadingLevel::H3),
            ("H4", HeadingLevel::H4),
            ("H5", HeadingLevel::H5),
            ("H6", HeadingLevel::H6),
        ],
    );
    let (marker, q) = choice(
        "List",
        vec![
            ("Bullet", ListMarker::Bullet),
            ("Ordered", ListMarker::Ordered(3)),
            ("None", ListMarker::None),
        ],
    );
    let (tabs, r) = choice("Tabs", vec![("4", 4u16), ("2", 2), ("8", 8)]);
    // Mirror heading defaults here because HeadingLevel::scale is private.
    let heading_scale = level.map(|level| match level {
        HeadingLevel::H1 => 1.8,
        HeadingLevel::H2 => 1.45,
        HeadingLevel::H3 => 1.2,
        HeadingLevel::H4 => 1.,
        HeadingLevel::H5 => 0.85,
        HeadingLevel::H6 => 0.75,
    });
    stack().w_full().min_width(0.).gap(vh(2.75)).children((
        controls(vec![a,b,c,d,e,f,g,h,i,j,k,l,m,n,o,p,q,r]),
        text("The quick brown fox jumps over the lazy dog. 你好，世界。 مرحبا بالعالم.\nA second paragraph with supercalifragilisticexpialidocious and room for a little more text.")
            .w_full().min_width(0.).height(vh(32.5)).min_height(responsive::ui(leading.select(144., 102.))).padding((vw(2.), vh(2.))).background(PANEL).animation(SPRING)
            .text_wrap_mode(wrap_mode).text_overflow(overflow).text_max_lines(limit).text_align(align)
            .text_vertical_align(vertical).font_family(family).font_italic(italic).text_underline(underline)
            .text_strikethrough(strike).text_wrap(nowrap.map(|v| !v)).font_weight(bold.select(Weight::BOLD, Weight::NORMAL))
            .font_size(responsive::font(size.select(30.,20.))).line_height(responsive::ui(leading.select(48.,34.)))
            .letter_spacing(responsive::ui(spacing.select(2.,0.))).paragraph_spacing(responsive::ui(paragraphs.select(24.,0.))),
        heading("Six heading levels").level(level)
            .font_size(responsive::font(heading_scale.map(|scale| 32. * scale)))
            .line_height(responsive::ui(heading_scale.map(|scale| 44. * scale))),
        wrap().w_full().gap(vw(1.)).children((code("let answer = 42;").font_size(responsive::font(32.)).line_height(responsive::ui(44.))
            .padding((responsive::ui(6.), responsive::ui(2.))).radius(responsive::ui(4.)), em("Emphasis").font_size(responsive::font(20.)).line_height(responsive::ui(30.)))),
        blockquote("Somewhere, something incredible is waiting to be known.").font_size(responsive::font(22.)).line_height(responsive::ui(32.)).padding_left(responsive::ui(18.)),
        code_block("fn main() {\n\tprintln!(\"hello\");\n}").tab_width(tabs).font_size(responsive::font(18.)).line_height(responsive::ui(28.))
            .padding((responsive::ui(16.),)).radius(responsive::ui(6.)),
        list().marker(marker).gap(responsive::ui(8.)).font_size(responsive::font(18.)).line_height(responsive::ui(28.)).children((text("One").font_size(responsive::font(18.)).line_height(responsive::ui(28.)), text("Two").font_size(responsive::font(18.)).line_height(responsive::ui(28.)), list_item().children(text("Three").font_size(responsive::font(18.)).line_height(responsive::ui(28.))))),
        document(Document::blocks([Document::heading("Document", HeadingLevel::H3),
            Document::rich([Span::new("Rich "), Span::new("spans").bold(), Span::new(" + "), Span::new("code").monospace()]),
            Document::list([Document::plain("First"), Document::em("Second")], ListMarker::Ordered(1))]))
            .font_size(responsive::font(20.)).line_height(responsive::ui(30.)).paragraph_spacing(responsive::ui(paragraphs.select(12.,0.))),
        markdown("### Markdown\n\n**Bold**, *italic*, `code`, [link](https://example.com).\n\n> A quote.").font_size(responsive::font(20.)).line_height(responsive::ui(30.)),
    )).into_node()
}
