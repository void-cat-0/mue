//! Real App/resource teardown coverage stays in tests, never in the application loop.
use super::*;
use crate::{
    artwork::{Anchor, Surface},
    effects,
    settings::{ReadSettings, Theme},
};
use macroquad::prelude::{Conf, get_screen_data};
use mue_core::{runtime::resource_counts, signal::Access};
use mue_macroquad::{Scene, font, math::Rect};
use std::cell::Cell;

#[test]
#[ignore = "requires a graphics display; exercises the real application owner and Scene UI"]
fn application_resources_outlive_pop_replace_and_pending_transition_teardown() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue song settings ownership regression".into(),
            window_width: 1000,
            window_height: 720,
            high_dpi: false,
            ..Default::default()
        },
        async {
            let mut fonts = font::fontdb::Database::new();
            fonts.load_font_data(
                include_bytes!("../../../playground/assets/fonts/NotoSans-Regular.ttf").to_vec(),
            );
            fonts.set_sans_serif_family("Noto Sans");
            font::initialize("en-US", fonts).unwrap();
            // Warm the thread-global viewport signal once; it is not an App-owned resource.
            struct Empty;
            impl Scene for Empty {}
            let mut warmup = App::with_scene(Empty);
            warmup.try_frame().unwrap();
            next_frame().await;
            drop(warmup);
            let baseline = resource_counts();
            let mut application = Application::new().unwrap();
            let read = application._settings.read();
            let editor = application._settings.editor();
            let catalog = Rc::downgrade(&application._catalog);
            let artwork = application
                ._catalog
                .songs()
                .iter()
                .map(|song| Rc::downgrade(&song.artwork))
                .collect::<Vec<_>>();
            application.app.try_frame().unwrap();
            next_frame().await;
            let library_counts = resource_counts();

            application
                .app
                .push_scene(SettingsScene::new(editor))
                .without_transition();
            application.app.try_frame().unwrap();
            let editor_counts = resource_counts();
            assert!(editor_counts.signals > library_counts.signals);
            next_frame().await;
            editor.set_theme(Theme::Light);
            editor.set_reduce_motion(true);
            application.app.try_frame().unwrap();
            let image = get_screen_data();
            let corner = image.get_pixel(4, 4);
            assert!(corner.r > 0.9 && corner.g > 0.9 && corner.b > 0.9);
            next_frame().await;
            application.app.pop_scene().without_transition();
            application.app.try_frame().unwrap();
            assert_eq!(application.app.scene_count(), 1);
            assert_eq!(resource_counts(), library_counts);
            assert_eq!(read.theme.get(), Theme::Light);
            assert!(!read.animations_enabled());
            next_frame().await;

            application
                .app
                .push_scene(SettingsScene::new(editor))
                .without_transition();
            application.app.try_frame().unwrap();
            next_frame().await;
            application
                .app
                .replace_scene(SettingsScene::new(editor))
                .without_transition();
            application.app.try_frame().unwrap();
            assert_eq!(application.app.scene_count(), 2);
            assert_eq!(resource_counts(), editor_counts);
            editor.toggle_theme();
            assert_eq!(read.theme.get(), Theme::Dark);
            next_frame().await;
            application.app.pop_scene().without_transition();
            application.app.try_frame().unwrap();
            assert_eq!(resource_counts(), library_counts);
            next_frame().await;

            struct PendingConsumer {
                _scene: SettingsScene,
                settings: ReadSettings,
                dropped: Rc<Cell<bool>>,
            }
            impl Scene for PendingConsumer {}
            impl Drop for PendingConsumer {
                fn drop(&mut self) {
                    // Actual Application field order must keep the global owner alive.
                    assert_eq!(self.settings.theme.get(), Theme::Dark);
                    assert!(self.settings.reduce_motion.get());
                    self.dropped.set(true);
                }
            }
            let dropped = Rc::new(Cell::new(false));
            let viewport = Rect::new(0., 0., 1000., 720.);
            let image = application._catalog.songs()[0].artwork.clone();
            let origin = Anchor {
                surface: Surface::card(Rect::new(52., 200., 320., 210.), viewport, image.aspect()),
                viewport,
            };
            // Host requests deliberately test teardown while a destination/effect is queued.
            application
                .app
                .push_scene(PendingConsumer {
                    _scene: SettingsScene::new(editor),
                    settings: read,
                    dropped: dropped.clone(),
                })
                .transition(effects::expand(image, origin));
            application.app.try_frame().unwrap();
            assert!(application.app.is_transitioning());
            assert_eq!(application.app.scene_count(), 1);
            assert!(!dropped.get());
            next_frame().await;
            drop(application);
            assert!(dropped.get());
            assert!(catalog.upgrade().is_none());
            assert!(artwork.iter().all(|image| image.upgrade().is_none()));
            assert_eq!(read.theme.try_get(), None);
            assert_eq!(read.reduce_motion.try_get(), None);
            assert_eq!(resource_counts(), baseline);
            crate::view::tests::actual_song_views_share_theme_and_keep_scrolled_selected_card()
                .await;
            assert_eq!(resource_counts(), baseline);
        },
    );
}
