//! Verify actual renderer selection after opening and drawing to a window.
#[cfg(all(target_os = "linux", not(target_env = "ohos")))]
#[path = "../../shared/render_backend.rs"]
mod render_backend;

#[cfg(all(target_os = "linux", not(target_env = "ohos")))]
fn main() {
    use macroquad::prelude::*;
    use miniquad::Backend;

    let expected = match std::env::args().nth(1).as_deref() {
        Some("--expect-vulkan") => Backend::Vulkan,
        Some("--expect-opengl") => Backend::OpenGl,
        _ => panic!("choose --expect-vulkan or --expect-opengl"),
    };
    macroquad::Window::from_config(
        Conf {
            window_title: "Mue renderer startup check".into(),
            window_width: 96,
            window_height: 64,
            platform: render_backend::platform(),
            ..Default::default()
        },
        async move {
            let actual = unsafe { get_internal_gl() }.quad_context.info().backend;
            assert_eq!(actual, expected, "actual window renderer");
            clear_background(BLACK);
            draw_rectangle(0., 0., 32., 32., RED);
            next_frame().await;
            println!("Renderer startup check passed: {actual:?}");
        },
    );
}

#[cfg(not(all(target_os = "linux", not(target_env = "ohos"))))]
fn main() {}
