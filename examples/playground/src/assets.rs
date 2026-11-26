use std::sync::Arc;

use macroquad::texture::{Image, Texture2D};
use mue_core::prelude::*;
use mue_macroquad::paint::shader::SharedTexture;

/// Decode an image on a blocking thread and upload it to a texture on the UI thread.
///
/// The returned signal starts with a transparent 1x1 placeholder, so callers can
/// build the image node immediately and let the texture swap in when ready.
pub fn background_texture(bytes: &'static [u8]) -> ReadSignal<SharedTexture> {
    let placeholder: SharedTexture = Texture2D::from_rgba8(1, 1, &[0, 0, 0, 0]).into();
    let texture = Signal::new_always(placeholder);
    let resource = AsyncResource::<Arc<Image>, String>::new_blocking(move || {
        Image::from_file_with_format(bytes, None)
            .map(Arc::new)
            .map_err(|error| error.to_string())
    });
    watch_effect(move || match resource.state().get_clone() {
        AsyncState::Loading => {}
        AsyncState::Ready(Ok(image)) => {
            let loaded: SharedTexture = Texture2D::from_image(&image).into();
            texture.set(loaded);
        }
        AsyncState::Ready(Err(error)) => {
            eprintln!("failed to load background texture: {error}");
        }
    });
    *texture
}
