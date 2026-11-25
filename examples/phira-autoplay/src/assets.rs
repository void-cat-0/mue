//! Read local charts and presentation assets without unpacking their archives.
use std::{
    error::Error,
    fs::File,
    io::{self, Cursor, Read},
    path::{Path, PathBuf},
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering::Relaxed},
};

use image::{DynamicImage, RgbaImage, imageops::FilterType};
use macroquad::prelude::{FilterMode, Texture2D};
use serde::Deserialize;
use zip::ZipArchive;

use super::{
    artwork::Artwork,
    audio::{self, AudioClip, HitSounds},
    chart::Chart,
    skin::{self, Skin},
};

pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

pub const DEFAULT_FILES: [&str; 1] = ["ManaBlast.pez"];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub name: String,
    pub composer: String,
    pub charter: String,
    #[serde(default)]
    pub illustrator: String,
    pub level: String,
    chart: String,
    music: String,
    illustration: String,
    #[serde(default)]
    pub offset: f64,
    #[serde(default = "aspect_ratio")]
    pub aspect_ratio: f32,
    #[serde(default = "background_dim")]
    pub background_dim: f32,
    #[serde(default = "line_length")]
    pub line_length: f32,
    #[serde(default)]
    pub force_aspect_ratio: bool,
    #[serde(default)]
    pub negative_length_hold: bool,
}

fn aspect_ratio() -> f32 {
    16. / 9.
}
fn background_dim() -> f32 {
    0.6
}
fn line_length() -> f32 {
    6.
}

pub struct Preview {
    pub path: PathBuf,
    pub info: Info,
    illustration: RgbaImage,
}

pub struct Song {
    pub path: PathBuf,
    pub info: Info,
    pub artwork: Rc<Artwork>,
    pub media: Rc<Media>,
}

impl Preview {
    pub fn upload(self, media: Rc<Media>) -> Rc<Song> {
        Rc::new(Song {
            path: self.path,
            info: self.info,
            artwork: Rc::new(Artwork {
                texture: upload_image(&self.illustration).into(),
            }),
            media,
        })
    }
}

pub struct Media {
    pub skin: Option<Skin>,
    pub sounds: HitSounds,
}

pub struct DecodedMedia {
    skin: Option<skin::Decoded>,
    sounds: HitSounds,
}

impl DecodedMedia {
    pub fn load(directories: &[PathBuf], silent: bool, warnings: &mut Vec<String>) -> Self {
        let skin = optional_file(directories, &skin::FILES, skin::Decoded::load, warnings);
        let sounds = if silent {
            HitSounds::default()
        } else {
            audio::HITSOUND_FILES
                .map(|name| optional_file(directories, &[name], audio::load_hitsound, warnings))
        };
        Self { skin, sounds }
    }

    pub fn upload(self) -> Rc<Media> {
        Rc::new(Media {
            skin: self.skin.map(skin::Decoded::upload),
            sounds: self.sounds,
        })
    }
}

fn optional_file<T>(
    directories: &[PathBuf],
    names: &[&str],
    load: impl FnOnce(&Path) -> Result<T>,
    warnings: &mut Vec<String>,
) -> Option<T> {
    let path = find_file(directories, names)?;
    match load(&path) {
        Ok(value) => Some(value),
        Err(error) => {
            warnings.push(format!("{}：{error}", path.display()));
            None
        }
    }
}

fn find_file(directories: &[PathBuf], names: &[&str]) -> Option<PathBuf> {
    directories
        .iter()
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|path| path.is_file())
}

pub struct Prepared {
    pub chart: Chart,
    pub audio: AudioClip,
}

#[cfg(not(target_os = "android"))]
pub fn desktop_directories() -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Ok(mut exe) = std::env::current_exe() {
        exe.pop();
        directories.push(exe);
    }
    directories.push(PathBuf::from("."));
    let examples = Path::new(env!("CARGO_MANIFEST_DIR"));
    directories.push(examples.join("phira-autoplay/charts"));
    directories.push(examples.join(".."));
    directories
}

#[cfg(not(target_os = "android"))]
pub fn defaults(directories: &[PathBuf]) -> Vec<PathBuf> {
    DEFAULT_FILES
        .into_iter()
        .map(|name| find_file(directories, &[name]).unwrap_or_else(|| PathBuf::from(name)))
        .collect()
}

pub(super) fn member(zip: &mut ZipArchive<File>, name: &str, max_bytes: u64) -> Result<Vec<u8>> {
    let file = zip.by_name(name)?;
    if file.size() > max_bytes {
        return Err(io::Error::other(format!(
            "{name} is larger than the demo's {max_bytes}-byte limit"
        ))
        .into());
    }
    let mut bytes = Vec::with_capacity(file.size() as usize);
    file.take(max_bytes + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_bytes {
        return Err(
            io::Error::other(format!("{name} exceeds the archive member size limit")).into(),
        );
    }
    Ok(bytes)
}

fn manifest(zip: &mut ZipArchive<File>) -> Result<Info> {
    let info: Info = serde_yaml::from_slice(&member(zip, "info.yml", 64 << 10)?)?;
    if !info.aspect_ratio.is_finite() || info.aspect_ratio <= 0. || !info.offset.is_finite() {
        return Err(io::Error::other("invalid aspectRatio or offset in info.yml").into());
    }
    Ok(info)
}

pub(super) fn open(path: &Path) -> Result<ZipArchive<File>> {
    Ok(ZipArchive::new(File::open(path).map_err(|err| {
        io::Error::new(err.kind(), format!("{}: {err}", path.display()))
    })?)?)
}

pub fn preview(path: PathBuf) -> Result<Preview> {
    let mut zip = open(&path)?;
    let info = manifest(&mut zip)?;
    let bytes = member(&mut zip, &info.illustration, 32 << 20)?;
    let illustration = thumbnail(decode_image(&bytes, 8192)?);
    Ok(Preview {
        path,
        info,
        illustration,
    })
}

pub(super) fn decode_image(bytes: &[u8], max_dimension: u32) -> Result<DynamicImage> {
    let mut reader = image::io::Reader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::io::Limits::default();
    limits.max_image_width = Some(max_dimension);
    limits.max_image_height = Some(max_dimension);
    limits.max_alloc = Some(128 << 20);
    reader.limits(limits);
    Ok(reader.decode()?)
}

pub(super) fn upload_image(image: &RgbaImage) -> Texture2D {
    let (w, h) = image.dimensions();
    let tex = Texture2D::from_rgba8(w as u16, h as u16, image.as_raw());
    tex.set_filter(FilterMode::Linear);
    tex
}

fn thumbnail(image: DynamicImage) -> RgbaImage {
    if image.width().max(image.height()) > 2048 {
        image.resize(2048, 2048, FilterType::Triangle).into_rgba8()
    } else {
        image.into_rgba8()
    }
}

pub fn load(path: &Path, cancelled: &AtomicBool) -> Result<Prepared> {
    let mut zip = open(path)?;
    let info = manifest(&mut zip)?;
    check_cancelled(cancelled)?;
    let chart = Chart::parse(&member(&mut zip, &info.chart, 64 << 20)?)?;
    check_cancelled(cancelled)?;
    let audio = audio::decode(member(&mut zip, &info.music, 96 << 20)?, cancelled)?;
    Ok(Prepared { chart, audio })
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<()> {
    if cancelled.load(Relaxed) {
        Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled PEZ load").into())
    } else {
        Ok(())
    }
}

#[cfg(all(test, not(target_os = "android")))]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a local ManaBlast.pez"]
    fn manablast_decodes_at_the_expected_timing() {
        let path = defaults(&desktop_directories()).into_iter().next().unwrap();
        let preview = preview(path.clone()).unwrap();
        assert!(preview.illustration.width() > 0);
        let loaded = load(&path, &AtomicBool::default()).unwrap();
        let chart = loaded.chart;
        assert_eq!(chart.lines.len(), 24);
        assert_eq!(chart.hits.len(), 1440);
        assert!((chart.offset + 0.240).abs() < 1e-6);
        assert!((chart.hits[0].time - 13.2).abs() < 1e-6);
        assert!((chart.last_time - 154.).abs() < 0.5);
        assert!(loaded.audio.length() > chart.last_time + chart.offset);
        super::super::app_log(
            "INFO",
            &format!(
                "{}: {} · {:.1} BPM · {} Hz · {:.2}s · {} autoplay notes",
                preview.info.name,
                chart.format,
                chart.bpm,
                loaded.audio.sample_rate(),
                loaded.audio.length(),
                chart.hits.len(),
            ),
        );
    }

    #[test]
    #[ignore = "requires the locally supplied Live ZIP and three Ogg hitsounds"]
    fn supplied_media_loads_without_opening_devices() {
        let mut warnings = Vec::new();
        let media = DecodedMedia::load(&desktop_directories(), false, &mut warnings);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(media.skin.is_some());
        for clip in media.sounds.map(Option::unwrap) {
            assert_eq!(clip.sample_rate(), 48_000);
            assert!(!clip.frames().is_empty());
        }
    }
}
