//! Playback uses Phira's mixer and music cursor. Only --silent uses a host clock.
use std::{
    cell::RefCell,
    error::Error,
    fs, io,
    path::Path,
    rc::{Rc, Weak},
    sync::atomic::{AtomicBool, Ordering::Relaxed},
    time::Instant,
};

pub use sasa::AudioClip;
use sasa::{AudioManager, Music, MusicParams, PlaySfxParams, Sfx};

use super::chart::{Chart, Kind};

pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

pub const HITSOUND_FILES: [&str; 3] = ["click.ogg", "drag.ogg", "flick.ogg"];
pub type HitSounds = [Option<AudioClip>; HITSOUND_FILES.len()];

const MUSIC_GAIN: f32 = 0.78;
const HIT_GAIN: f32 = 0.32;

#[derive(Default)]
pub struct HitCursor(usize);

impl HitCursor {
    pub fn advance(&mut self, chart: &Chart, time: f64) -> [bool; HITSOUND_FILES.len()] {
        let hits = &chart.hits[self.0..];
        let crossed = hits.partition_point(|hit| hit.time <= time);
        self.0 += crossed;
        let mut due = [false; HITSOUND_FILES.len()];
        // Skip stale sounds after a stall; identical samples due this frame share one voice.
        for hit in hits[..crossed]
            .iter()
            .rev()
            .take_while(|hit| hit.time >= time - 0.12)
        {
            let index = match chart.lines[hit.line].notes[hit.note].kind {
                Kind::Tap | Kind::Hold => 0,
                Kind::Drag => 1,
                Kind::Flick => 2,
            };
            due[index] = true;
        }
        due
    }
}

pub fn load_hitsound(path: &Path) -> Result<AudioClip> {
    decode_clip(fs::read(path)?)
}

pub fn decode(bytes: Vec<u8>, cancelled: &AtomicBool) -> Result<AudioClip> {
    let check = || {
        if cancelled.load(Relaxed) {
            Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "cancelled audio decode",
            ))
        } else {
            Ok(())
        }
    };
    check()?;
    // Sasa probes the bytes; ManaBlast's member has a .wav name but contains MP3.
    let clip = decode_clip(bytes)?;
    // Decode itself is synchronous on the loader thread. Don't publish after cancellation.
    check()?;
    Ok(clip)
}

fn decode_clip(bytes: Vec<u8>) -> Result<AudioClip> {
    let clip = AudioClip::new(bytes)?;
    if clip.frame_count() == 0 {
        return Err("audio track contains no samples".into());
    }
    Ok(clip)
}

pub struct Playback {
    duration: f64,
    mode: Mode,
}

enum Mode {
    Silent { since: Option<Instant>, at: f64 },
    Device(Device),
}

struct Device {
    track: Track,
    manager: AudioManager,
    clip: AudioClip,
    hitsounds: HitSounds,
}

struct Track {
    control: Rc<RefCell<MusicControl>>,
    sounds: [Option<Sfx>; HITSOUND_FILES.len()],
    #[cfg(target_os = "android")]
    _mute_owner: std::sync::Arc<()>,
}

struct MusicControl {
    music: Music,
    playing: bool,
}

thread_local! {
    static ACTIVE_TRACK: RefCell<Weak<RefCell<MusicControl>>> = const { RefCell::new(Weak::new()) };
}

impl MusicControl {
    fn play(&mut self) -> Result<()> {
        if !self.playing {
            self.music.play()?;
            self.playing = true;
        }
        Ok(())
    }

    fn pause(&mut self) -> Result<()> {
        if self.playing {
            self.music.pause()?;
            self.playing = false;
        }
        Ok(())
    }
}

/// Native pause notifications also arrive when the window cannot draw an App frame.
pub(super) fn pause_active() -> Result<()> {
    if let Some(track) = ACTIVE_TRACK.with(|active| active.borrow().upgrade()) {
        track.borrow_mut().pause()?;
    }
    Ok(())
}

#[cfg(target_os = "android")]
static HOST_MUTED: AtomicBool = AtomicBool::new(true);

#[cfg(target_os = "android")]
pub(super) fn mute_for_host_pause() {
    HOST_MUTED.store(true, Relaxed);
}

// Java can mute before the render thread receives the native pause message. This
// renderer comes after Music and Sfx in the mixer. The native listener then pauses the
// music cursor; muting alone would only silence its output, not freeze the cursor.
#[cfg(target_os = "android")]
struct HostMute(std::sync::Weak<()>);

#[cfg(target_os = "android")]
impl sasa::Renderer for HostMute {
    fn alive(&self) -> bool {
        self.0.strong_count() != 0
    }

    fn render_mono(&mut self, _: u32, data: &mut [f32]) {
        if HOST_MUTED.load(Relaxed) {
            data.fill(0.);
        }
    }

    fn render_stereo(&mut self, rate: u32, data: &mut [f32]) {
        self.render_mono(rate, data);
    }
}

impl Track {
    fn new(manager: &mut AudioManager, clip: &AudioClip, hitsounds: &HitSounds) -> Result<Self> {
        let music = manager.create_music(
            clip.clone(),
            MusicParams {
                amplifier: MUSIC_GAIN,
                ..Default::default()
            },
        )?;
        let mut sounds = [None, None, None];
        for (sound, clip) in sounds.iter_mut().zip(hitsounds) {
            if let Some(clip) = clip {
                *sound = Some(manager.create_sfx(clip.clone(), None)?);
            }
        }
        #[cfg(target_os = "android")]
        let mute_owner = {
            let owner = std::sync::Arc::new(());
            manager.add_renderer(HostMute(std::sync::Arc::downgrade(&owner)))?;
            owner
        };
        Ok(Self {
            control: Rc::new(RefCell::new(MusicControl {
                music,
                playing: false,
            })),
            sounds,
            #[cfg(target_os = "android")]
            _mute_owner: mute_owner,
        })
    }
}

impl Playback {
    pub fn new(clip: &AudioClip, hitsounds: &HitSounds, silent: bool) -> Result<Self> {
        let mode = if silent {
            Mode::Silent {
                since: None,
                at: 0.,
            }
        } else {
            let mut manager = open_manager()?;
            let track = Track::new(&mut manager, clip, hitsounds)?;
            Mode::Device(Device {
                track,
                manager,
                clip: clip.clone(),
                hitsounds: hitsounds.clone(),
            })
        };
        Ok(Self {
            duration: clip.length(),
            mode,
        })
    }

    pub fn play(&mut self) -> Result<()> {
        match &mut self.mode {
            Mode::Silent { since, .. } => {
                since.get_or_insert_with(Instant::now);
            }
            Mode::Device(device) => {
                #[cfg(target_os = "android")]
                HOST_MUTED.store(false, Relaxed);
                device.track.control.borrow_mut().play()?;
                ACTIVE_TRACK
                    .with(|active| *active.borrow_mut() = Rc::downgrade(&device.track.control));
            }
        }
        Ok(())
    }

    pub fn pause(&mut self) -> Result<()> {
        match &mut self.mode {
            Mode::Silent { since, at } => {
                if let Some(start) = since.take() {
                    *at = (*at + start.elapsed().as_secs_f64()).min(self.duration);
                }
            }
            Mode::Device(device) => device.track.control.borrow_mut().pause()?,
        }
        Ok(())
    }

    pub fn rewind(&mut self) -> Result<()> {
        self.pause()?;
        match &mut self.mode {
            Mode::Silent { at, .. } => *at = 0.,
            // A paused sasa seek does not publish its new cursor. A fresh Music
            // starts paused at zero and cannot inherit the previous EOF.
            Mode::Device(device) => {
                device.track = Track::new(&mut device.manager, &device.clip, &device.hitsounds)?;
            }
        }
        Ok(())
    }

    pub fn position(&self) -> f64 {
        match &self.mode {
            Mode::Silent { since, at } => {
                (*at + since.map_or(0., |start| start.elapsed().as_secs_f64())).min(self.duration)
            }
            Mode::Device(device) => device.track.control.borrow().music.position(),
        }
    }

    pub fn play_hits(&mut self, due: [bool; HITSOUND_FILES.len()]) -> Result<()> {
        if let Mode::Device(device) = &mut self.mode {
            for (due, sound) in due.into_iter().zip(&mut device.track.sounds) {
                if due && let Some(sound) = sound {
                    sound.play(PlaySfxParams {
                        amplifier: HIT_GAIN,
                    })?;
                }
            }
        }
        Ok(())
    }

    pub fn needs_start(&self) -> bool {
        match &self.mode {
            Mode::Silent { since, .. } => since.is_none(),
            Mode::Device(device) => !device.track.control.borrow().playing,
        }
    }

    pub fn recover_if_needed(&mut self) -> Result<()> {
        if let Mode::Device(device) = &mut self.mode {
            device.manager.recover_if_needed()?;
        }
        Ok(())
    }
}

fn open_manager() -> Result<AudioManager> {
    #[cfg(target_os = "android")]
    let backend = {
        use sasa::backend::oboe::{OboeBackend, OboeSettings, PerformanceMode, Usage};
        OboeBackend::new(OboeSettings {
            performance_mode: PerformanceMode::LowLatency,
            usage: Usage::Game,
            ..Default::default()
        })
    };
    #[cfg(target_env = "ohos")]
    let backend = {
        use sasa::backend::ohos::{OhosBackend, OhosSettings};
        OhosBackend::new(OhosSettings {
            sample_rate: Some(48000),
            buffer_size: Some(256),
            channels: 2,
        })
    };
    #[cfg(not(any(target_os = "android", target_env = "ohos")))]
    let backend = {
        use sasa::backend::cpal::{CpalBackend, CpalSettings};
        CpalBackend::new(CpalSettings {
            preferred_sample_rate: Some(48000),
            buffer_size: None,
        })
    };
    Ok(AudioManager::new(backend)?)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn sasa_probes_and_duplicates_mono_wav() {
        let mut wav = b"RIFF\x28\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x40\x1f\0\0\x80\x3e\0\0\x02\0\x10\0data\x04\0\0\0".to_vec();
        wav.extend([0, 0x40, 0, 0xc0]);
        let clip = decode(wav, &AtomicBool::new(false)).unwrap();
        assert_eq!(clip.sample_rate(), 8000);
        assert_eq!(clip.frame_count(), 2);
        let f = clip.frames()[0];
        assert_eq!((f.0, f.1), (0.5, 0.5));
    }

    #[test]
    fn cancelled_decode_stops_before_probing() {
        let err = decode(Vec::new(), &AtomicBool::new(true)).err().unwrap();
        assert_eq!(
            err.downcast_ref::<io::Error>().unwrap().kind(),
            io::ErrorKind::Interrupted
        );
    }

    #[test]
    fn silent_playback_pauses_and_rewinds_without_a_device() {
        let clip = AudioClip::from_raw(vec![sasa::Frame::default(); 10], 1);
        let mut playback = Playback::new(&clip, &HitSounds::default(), true).unwrap();
        assert_eq!(playback.position(), 0.);
        playback.mode = Mode::Silent {
            since: Some(Instant::now() - Duration::from_secs(1)),
            at: 0.,
        };
        playback.pause().unwrap();
        let at = playback.position();
        assert!(at >= 1.);
        assert_eq!(playback.position(), at);
        playback.play().unwrap();
        assert!(playback.position() >= at);
        playback.rewind().unwrap();
        assert_eq!(playback.position(), 0.);
    }
}
