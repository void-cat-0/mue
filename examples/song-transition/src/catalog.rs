//! Static catalog and shared, render-thread-owned artwork assets.
use std::rc::Rc;

use crate::artwork::Artwork;
use anyhow::Result;

#[derive(Clone)]
pub struct Song {
    pub title: &'static str,
    pub artist: &'static str,
    pub level: &'static str,
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

    pub fn load() -> Result<Self> {
        let artwork = [
            Artwork::load(include_bytes!("../assets/solstice.png"))?,
            Artwork::load(include_bytes!("../assets/afterglow.png"))?,
            Artwork::load(include_bytes!("../assets/tidal.png"))?,
        ];
        let songs = [
            ("SOLSTICE", "Aster / melodic progressive", "154 BPM / IN 13"),
            ("AFTERGLOW", "Northbound / future garage", "128 BPM / IN 12"),
            ("TIDAL", "Mira / ambient drum & bass", "172 BPM / IN 14"),
            (
                "SOLSTICE - dusk edit",
                "Aster / sunset sessions",
                "154 BPM / HD 10",
            ),
            (
                "AFTERGLOW - orbit edit",
                "Northbound / night drive",
                "128 BPM / HD 9",
            ),
            (
                "TIDAL - shoreline edit",
                "Mira / quiet currents",
                "172 BPM / HD 11",
            ),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (title, artist, level))| Song {
            title,
            artist,
            level,
            artwork: artwork[index % artwork.len()].clone(),
        })
        .collect();
        Ok(Self { songs })
    }
}
