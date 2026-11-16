//! View regressions; the graphics helper runs inside the single App test window.
use super::*;
use crate::{
    input::SettingsAction,
    settings::{Settings, Theme},
};
use mue_core::runtime::resource_counts;

pub(crate) async fn actual_song_views_share_theme_and_keep_scrolled_selected_card() {
    use crate::catalog::Catalog;
    use macroquad::prelude::{draw_rectangle, get_screen_data, next_frame};
    use mue_macroquad::layout::with_taffy;

    fn prepare_library(library: &LibraryView, viewport: Rect) {
        let mut roots = Vec::new();
        library.layout(viewport, RootInput::Interactive, &mut roots);
        for root in roots {
            root.node.prepare_layout_in(root.bounds);
        }
    }

    fn absolute_rect(node: NodeRef) -> Rect {
        with_taffy(|taffy| {
            let id = node.layout_id().unwrap();
            let layout = taffy.layout(id).unwrap();
            let mut x = layout.location.x;
            let mut y = layout.location.y;
            let mut parent = taffy.parent(id);
            while let Some(id) = parent {
                let layout = taffy.layout(id).unwrap();
                x += layout.location.x;
                y += layout.location.y;
                parent = taffy.parent(id);
            }
            Rect::new(x, y, layout.size.width, layout.size.height)
        })
    }

    fn dark_pixels(image: &macroquad::prelude::Image, rect: Rect) -> usize {
        let mut count = 0;
        for y in rect.y.max(0.) as u32..rect.bottom().min(image.height as f32) as u32 {
            for x in rect.x.max(0.) as u32..rect.right().min(image.width as f32) as u32 {
                // OpenGL screen readback has a bottom-left origin. UI geometry is top-left.
                let pixel = image.get_pixel(x, image.height as u32 - 1 - y);
                if pixel.r < 0.3 && pixel.g < 0.35 && pixel.b < 0.45 {
                    count += 1;
                }
            }
        }
        count
    }

    // Macroquad permits one native window per test process. Reuse the App test window.
    {
        let settings = Settings::new();
        let read = settings.read();
        let catalog = Catalog::load().unwrap();
        let baseline = resource_counts();
        let viewport = Rect::new(0., 0., 1000., 720.);
        let mut library = LibraryView::new(catalog.songs(), read, Rc::new(Cell::new(None)));
        library.scroll(200.);
        library.select(1);
        prepare_library(&library, viewport);
        let roots = (*library.root, *library.header);
        let offset = library.offset.get();
        let selected = library.selected.get();
        let card_rect = library.card_geometry[1].get();
        assert!(offset.y > 0.);
        assert_eq!(selected, Some(1));
        assert!(card_rect.intersect(&library.clip.get()).is_some());
        let library_counts = resource_counts();
        next_frame().await;

        let detail = DetailsView::new(&catalog.songs()[1], read, Rc::new(Cell::new(None)));
        let panel = detail.root.children().get_clone()[0];
        let title = panel.children().get_clone()[1];
        settings.editor().set_theme(Theme::Light);
        settings.editor().set_reduce_motion(true);
        detail.root.prepare_layout_in(viewport);
        draw_rectangle(
            0.,
            0.,
            viewport.w,
            viewport.h,
            read.theme.get().palette().background.into(),
        );
        detail.root.render();
        let image = get_screen_data();
        assert!(
            dark_pixels(&image, absolute_rect(title)) > 50,
            "detail title must use the shared light-theme foreground"
        );
        next_frame().await;
        drop(detail);
        assert_eq!(
            resource_counts(),
            library_counts,
            "detail projections release independently of the retained library"
        );

        prepare_library(&library, viewport);
        draw_rectangle(
            0.,
            0.,
            viewport.w,
            viewport.h,
            read.theme.get().palette().background.into(),
        );
        library.header.render();
        library.root.render();
        let image = get_screen_data();
        assert!(
            dark_pixels(&image, Rect::new(36., 24., 320., 50.)) > 50,
            "library title must update without reconstruction"
        );
        let sample = image.get_pixel(410, image.height as u32 - 1 - (card_rect.y + 24.) as u32);
        let color = Theme::Light.palette().selected;
        assert!(
            (sample.r - color.r).abs() < 0.02
                && (sample.g - color.g).abs() < 0.02
                && (sample.b - color.b).abs() < 0.02,
            "selected row keeps its reactive background"
        );
        assert!((*library.root, *library.header) == roots);
        assert_eq!(library.offset.get(), offset);
        assert_eq!(library.selected.get(), selected);
        assert_eq!(library.card_geometry[1].get(), card_rect);
        assert!(!read.animations_enabled());
        next_frame().await;
        drop(library);
        assert_eq!(resource_counts(), baseline);
        assert_eq!(read.theme.get(), Theme::Light);
        drop(settings);
        assert_eq!(read.theme.try_get(), None);
    }
}

fn initialize_fonts() {
    let mut fonts = mue_macroquad::font::fontdb::Database::new();
    fonts.load_font_data(
        include_bytes!("../../../playground/assets/fonts/NotoSans-Regular.ttf").to_vec(),
    );
    fonts.set_sans_serif_family("Noto Sans");
    mue_macroquad::font::initialize("en-US", fonts).unwrap();
}

#[test]
fn real_views_release_local_bindings_without_disposing_application_settings() {
    initialize_fonts();
    let settings = Settings::new();
    let read = settings.read();
    let editor = settings.editor();
    let baseline = resource_counts();
    let commands = Rc::new(Cell::new(None));
    let mut library = LibraryView::new(&[], read, commands.clone());
    let roots = (*library.root, *library.header);
    let offset = library.offset;
    let selected = library.selected;
    library.scroll(220.);
    library.select(3);
    let retained = resource_counts();
    assert!(retained.signals > baseline.signals);
    assert!(retained.effects > baseline.effects);

    // Covering the library does not transfer its state into the editor view.
    let settings_commands = Rc::new(Cell::new(None::<SettingsAction>));
    let editor_view = SettingsView::new(read, settings_commands.clone());
    assert_eq!(read.theme.get(), Theme::Dark);
    editor.toggle_theme();
    editor.toggle_reduce_motion();
    assert_eq!(read.theme.get(), Theme::Light);
    assert!(!read.animations_enabled());
    assert!((*library.root, *library.header) == roots);
    assert_eq!(offset.get(), Vector::new(0., 220.));
    assert_eq!(selected.get(), Some(3));

    drop(editor_view);
    assert_eq!(Rc::strong_count(&settings_commands), 1);
    assert_eq!(
        resource_counts(),
        retained,
        "editor UI bindings must not survive pop"
    );
    editor.toggle_theme();
    editor.toggle_reduce_motion();
    assert_eq!(offset.get(), Vector::new(0., 220.));
    assert_eq!(selected.get(), Some(3));

    drop(library);
    assert_eq!(Rc::strong_count(&commands), 1);
    assert_eq!(offset.try_get(), None);
    assert_eq!(selected.try_get(), None);
    assert_eq!(resource_counts(), baseline);
    assert_eq!(read.theme.get(), Theme::Dark);
    drop(settings);
    assert_eq!(read.theme.try_get(), None);
    assert_eq!(read.reduce_motion.try_get(), None);
}

#[test]
fn rebuilding_settings_view_uses_latest_values_and_leaves_no_detached_bindings() {
    initialize_fonts();
    let settings = Settings::new();
    let read = settings.read();
    let editor = settings.editor();
    let baseline = resource_counts();
    for theme in [Theme::Light, Theme::Dark, Theme::Light] {
        let old = SettingsView::new(read, Rc::new(Cell::new(None)));
        editor.set_theme(theme);
        drop(old);
        let replacement = SettingsView::new(read, Rc::new(Cell::new(None)));
        assert_eq!(read.theme.get(), theme);
        drop(replacement);
        assert_eq!(resource_counts(), baseline);
        assert_eq!(read.theme.get(), theme);
    }
}
