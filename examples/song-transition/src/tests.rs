use crate::artwork::{self, Anchor, Surface};
use macroquad::prelude::Image;
use mue_macroquad::math::Rect as UiRect;

#[test]
fn plain_display_geometry_rebases_and_preserves_exact_crop_endpoints() {
    let card = Surface::card(
        UiRect::new(48., 210., 320., 210.),
        UiRect::new(28., 146., 944., 546.),
        1.5,
    );
    let source = Anchor {
        surface: card,
        viewport: UiRect::new(0., 0., 1000., 720.),
    };
    let full = Surface::fullscreen(source.viewport, 1.5);
    assert_eq!(source.in_viewport(source.viewport), card);
    assert_eq!(
        source.in_viewport(source.viewport).lerp(full, 0.).uv,
        card.uv
    );
    assert!((card.lerp(full, 1.).dim - artwork::DETAIL_DIM).abs() < 1e-6);
}

#[test]
fn bundled_artwork_is_valid_and_different() {
    let images = [
        include_bytes!("../assets/solstice.png").as_slice(),
        include_bytes!("../assets/afterglow.png").as_slice(),
        include_bytes!("../assets/tidal.png").as_slice(),
    ]
    .map(|bytes| Image::from_file_with_format(bytes, None).unwrap());
    for image in &images {
        assert_eq!((image.width, image.height), (960, 640));
    }
    assert_ne!(images[0].bytes, images[1].bytes);
    assert_ne!(images[1].bytes, images[2].bytes);
}
