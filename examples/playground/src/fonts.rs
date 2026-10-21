use super::responsive;
use mue_components::*;
use std::sync::Arc;

use mue_macroquad::{
    font::{self, fontdb},
    style::{Styleable, vh},
};

const FONTS: &[(&str, &[u8])] = &[
    (
        "NotoSans-Regular.ttf",
        include_bytes!("../assets/fonts/NotoSans-Regular.ttf"),
    ),
    (
        "NotoSans-Bold.ttf",
        include_bytes!("../assets/fonts/NotoSans-Bold.ttf"),
    ),
    (
        "NotoSans-Italic.ttf",
        include_bytes!("../assets/fonts/NotoSans-Italic.ttf"),
    ),
    (
        "NotoSans-BoldItalic.ttf",
        include_bytes!("../assets/fonts/NotoSans-BoldItalic.ttf"),
    ),
    (
        "NotoSerif-Regular.ttf",
        include_bytes!("../assets/fonts/NotoSerif-Regular.ttf"),
    ),
    (
        "NotoSerif-Bold.ttf",
        include_bytes!("../assets/fonts/NotoSerif-Bold.ttf"),
    ),
    (
        "NotoSerif-Italic.ttf",
        include_bytes!("../assets/fonts/NotoSerif-Italic.ttf"),
    ),
    (
        "NotoSerif-BoldItalic.ttf",
        include_bytes!("../assets/fonts/NotoSerif-BoldItalic.ttf"),
    ),
    (
        "NotoSansMono-Regular.ttf",
        include_bytes!("../assets/fonts/NotoSansMono-Regular.ttf"),
    ),
    (
        "NotoSansMono-Bold.ttf",
        include_bytes!("../assets/fonts/NotoSansMono-Bold.ttf"),
    ),
    (
        "NotoSansCJKsc-Regular.otf",
        include_bytes!("../assets/fonts/NotoSansCJKsc-Regular.otf"),
    ),
    (
        "NotoSansCJKsc-Bold.otf",
        include_bytes!("../assets/fonts/NotoSansCJKsc-Bold.otf"),
    ),
    (
        "NotoSansArabic-Regular.ttf",
        include_bytes!("../assets/fonts/NotoSansArabic-Regular.ttf"),
    ),
    (
        "NotoSansArabic-Bold.ttf",
        include_bytes!("../assets/fonts/NotoSansArabic-Bold.ttf"),
    ),
    (
        "NotoSansSymbols2-Regular.ttf",
        include_bytes!("../assets/fonts/NotoSansSymbols2-Regular.ttf"),
    ),
];

const NOTICES: &[&str] = &[
    include_str!("../assets/fonts/NOTICE.txt"),
    include_str!("../assets/fonts/OFL-NotoLatinGreekCyrillic.txt"),
    include_str!("../assets/fonts/OFL-NotoCJK.txt"),
    include_str!("../assets/fonts/OFL-NotoArabic.txt"),
    include_str!("../assets/fonts/OFL-NotoSymbols.txt"),
];

fn database() -> fontdb::Database {
    let mut db = fontdb::Database::new();
    for &(name, bytes) in FONTS {
        let ids = db.load_font_source(fontdb::Source::Binary(Arc::new(bytes)));
        assert_eq!(ids.len(), 1, "invalid bundled font: {name}");
        if name.starts_with("NotoSansMono-") {
            // These upstream static faces omit post.isFixedPitch; cosmic-text indexes this metadata.
            let mut face = db.face(ids[0]).unwrap().clone();
            face.monospaced = true;
            db.remove_face(ids[0]);
            db.push_face_info(face);
        }
    }
    db.set_sans_serif_family("Noto Sans");
    db.set_serif_family("Noto Serif");
    db.set_monospace_family("Noto Sans Mono");
    db.set_cursive_family("Noto Sans");
    db.set_fantasy_family("Noto Sans");
    db
}

pub fn initialize() {
    font::initialize("zh-CN", database()).expect("initialize bundled fonts before creating text");
}

pub fn licenses() -> Node {
    stack()
        .style(mue_macroquad::style! {
        width: 100%;
        min-width: 0px;
        gap: {vh(3.)};
        })
        .children(
            NOTICES
                .iter()
                .map(|notice| {
                    text(*notice)
                        .style(mue_macroquad::style! {
                        width: 100%;
                        font-size: {responsive::font(14.)};
                        line-height: {responsive::ui(21.)};
                        flex: none;
                        })
                        .into_node()
                })
                .collect::<Vec<_>>(),
        )
        .into_node()
}
