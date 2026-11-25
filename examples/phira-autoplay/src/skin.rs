//! The note textures and hold slices from a Phira skin archive.
use super::app_log;
use std::path::Path;

use image::RgbaImage;
use macroquad::{math::Rect, texture::Texture2D};
use serde::Deserialize;

use super::{assets, chart::Kind};

pub const FILES: [&str; 2] = ["live 测试5.zip", "live-skin.zip"];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Info {
    name: String,
    author: String,
    hold_atlas: [u32; 2],
    #[serde(rename = "holdAtlasMH")]
    hold_atlas_mh: [u32; 2],
    #[serde(default)]
    hold_keep_head: bool,
    #[serde(default)]
    hold_compact: bool,
}

pub struct Decoded {
    info: Info,
    normal: Set<RgbaImage>,
    chord: Set<RgbaImage>,
}

struct Set<T> {
    click: T,
    drag: T,
    flick: T,
    hold: T,
    slices: HoldSlices,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct HoldSlices {
    head: Rect,
    body: Rect,
    tail: Rect,
}

impl HoldSlices {
    fn new(width: u32, height: u32, [tail, head]: [u32; 2]) -> assets::Result<Self> {
        if u64::from(tail) + u64::from(head) >= u64::from(height) {
            return Err("hold atlas caps leave no room for the body".into());
        }
        let width = width as f32;
        Ok(Self {
            head: Rect::new(0., (height - head) as f32, width, head as f32),
            body: Rect::new(0., tail as f32, width, (height - tail - head) as f32),
            tail: Rect::new(0., 0., width, tail as f32),
        })
    }
}

impl Decoded {
    pub fn load(path: &Path) -> assets::Result<Self> {
        let mut zip = assets::open(path)?;
        let info: Info = serde_yaml::from_slice(&assets::member(&mut zip, "info.yml", 64 << 10)?)?;
        let mut load_set = |suffix, atlas| {
            let mut png = |stem| -> assets::Result<RgbaImage> {
                let name = format!("{stem}{suffix}.png");
                let bytes = assets::member(&mut zip, &name, 16 << 20)?;
                Ok(assets::decode_image(&bytes, 4096)?.into_rgba8())
            };
            let hold = png("hold")?;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(Set {
                click: png("click")?,
                drag: png("drag")?,
                flick: png("flick")?,
                slices: HoldSlices::new(hold.width(), hold.height(), atlas)?,
                hold,
            })
        };
        let normal = load_set("", info.hold_atlas)?;
        let chord = load_set("_mh", info.hold_atlas_mh)?;
        Ok(Self {
            info,
            normal,
            chord,
        })
    }

    pub fn upload(self) -> Skin {
        app_log(
            "INFO",
            &format!("Note skin: {} · {}", self.info.name, self.info.author),
        );
        Skin {
            normal: self.normal.upload(),
            chord: self.chord.upload(),
            keep_head: self.info.hold_keep_head,
            compact: self.info.hold_compact,
        }
    }
}

impl Set<RgbaImage> {
    fn upload(self) -> Set<Texture2D> {
        Set {
            click: assets::upload_image(&self.click),
            drag: assets::upload_image(&self.drag),
            flick: assets::upload_image(&self.flick),
            hold: assets::upload_image(&self.hold),
            slices: self.slices,
        }
    }
}

pub struct Skin {
    normal: Set<Texture2D>,
    chord: Set<Texture2D>,
    pub keep_head: bool,
    pub compact: bool,
}

#[derive(Clone, Copy)]
pub struct Sprite<'a> {
    pub texture: &'a Texture2D,
    pub source: Rect,
}

impl Sprite<'_> {
    pub fn height(self, width: f32) -> f32 {
        width * self.source.h / self.source.w
    }
}

impl Skin {
    fn set(&self, highlighted: bool) -> &Set<Texture2D> {
        if highlighted {
            &self.chord
        } else {
            &self.normal
        }
    }

    pub fn point(&self, kind: Kind, highlighted: bool) -> Option<Sprite<'_>> {
        let set = self.set(highlighted);
        let texture = match kind {
            Kind::Tap => &set.click,
            Kind::Drag => &set.drag,
            Kind::Flick => &set.flick,
            Kind::Hold => return None,
        };
        Some(Sprite {
            texture,
            source: Rect::new(0., 0., texture.width(), texture.height()),
        })
    }

    pub fn hold(&self, highlighted: bool) -> [Sprite<'_>; 3] {
        let set = self.set(highlighted);
        [set.slices.head, set.slices.body, set.slices.tail].map(|source| Sprite {
            texture: &set.hold,
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hold_caps_are_pixels_from_the_bottom_and_top() {
        let slices = HoldSlices::new(1000, 850, [35, 60]).unwrap();
        assert_eq!(slices.tail, Rect::new(0., 0., 1000., 35.));
        assert_eq!(slices.body, Rect::new(0., 35., 1000., 755.));
        assert_eq!(slices.head, Rect::new(0., 790., 1000., 60.));
        assert!(HoldSlices::new(1000, 90, [35, 60]).is_err());
    }
}
