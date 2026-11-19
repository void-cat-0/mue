//! Static catalog and shared, render-thread-owned artwork assets.
use std::rc::Rc;

use crate::artwork::Artwork;

#[derive(Clone)]
pub struct Song {
    pub title: &'static str,
    pub artist: &'static str,
    pub description_key: &'static str,
    pub bpm: u16,
    pub difficulty: &'static str,
    pub level: u8,
    pub artwork: Rc<Artwork>,
}

/// The application retains the catalog; Scenes receive an owning Rc or one Song.
pub struct Catalog {
    songs: Vec<Song>,
}

impl Catalog {
    pub fn songs(&self) -> &[Song] {
        &self.songs
    }

    pub fn load() -> Result<Self, macroquad::Error> {
        let artwork = [
            Artwork::load(include_bytes!("../assets/solstice.png"))?,
            Artwork::load(include_bytes!("../assets/afterglow.png"))?,
            Artwork::load(include_bytes!("../assets/tidal.png"))?,
        ];
        let songs = [
            ("SOLSTICE", "Aster", "genre-progressive", 154, "IN", 13),
            ("AFTERGLOW", "Northbound", "genre-garage", 128, "IN", 12),
            ("TIDAL", "Mira", "genre-drum-bass", 172, "IN", 14),
            (
                "SOLSTICE - dusk edit",
                "Aster",
                "genre-sunset",
                154,
                "HD",
                10,
            ),
            (
                "AFTERGLOW - orbit edit",
                "Northbound",
                "genre-night-drive",
                128,
                "HD",
                9,
            ),
            (
                "TIDAL - shoreline edit",
                "Mira",
                "genre-quiet-currents",
                172,
                "HD",
                11,
            ),
        ]
        .into_iter()
        .enumerate()
        .map(
            |(index, (title, artist, description_key, bpm, difficulty, level))| Song {
                title,
                artist,
                description_key,
                bpm,
                difficulty,
                level,
                artwork: artwork[index % artwork.len()].clone(),
            },
        )
        .collect();
        Ok(Self { songs })
    }
}
