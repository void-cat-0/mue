//! UI sound playback through [sasa](https://github.com/Mivik/sasa) (feature
//! `sound`): cpal on desktop and iOS, oboe on Android. Not available on the
//! web.
//!
//! The audio output opens on the first sound. Each [`SoundCue`] has a short
//! built-in sample synthesized at startup; [`set_sound_sample`] replaces one
//! with the application's own file (any format symphonia decodes: WAV, OGG,
//! FLAC, MP3, AAC).

use std::{cell::RefCell, collections::HashMap, mem::ManuallyDrop};

use sasa::{AudioClip, AudioManager, Frame, PlaySfxParams, Sfx};

use super::feedback::SoundCue;

const RATE: u32 = 44_100;

struct Player {
    manager: AudioManager,
    sfx: HashMap<SoundCue, Sfx>,
    volume: f32,
}

thread_local! {
    /// `None` until the first sound; `Some(None)` if the output failed to open.
    /// Never dropped: the output lives as long as the process, and closing
    /// the audio stream during thread-local destruction aborts on exit.
    static PLAYER: RefCell<Option<Option<ManuallyDrop<Player>>>> = const { RefCell::new(None) };
}

fn open() -> Option<Player> {
    #[cfg(target_os = "android")]
    let backend = sasa::backend::oboe::OboeBackend::new(Default::default());
    #[cfg(not(target_os = "android"))]
    let backend = sasa::backend::cpal::CpalBackend::new(Default::default());
    match AudioManager::new(backend) {
        Ok(manager) => Some(Player {
            manager,
            sfx: HashMap::new(),
            volume: 1.,
        }),
        Err(error) => {
            eprintln!("mue: UI sounds are off, the audio output did not open: {error:#}");
            None
        }
    }
}

fn with_player<R>(f: impl FnOnce(&mut Player) -> R) -> Option<R> {
    PLAYER.with(|player| {
        let mut player = player.borrow_mut();
        let player = player
            .get_or_insert_with(|| open().map(ManuallyDrop::new))
            .as_mut()?;
        // Reopen the output after a device change (headphones unplugged).
        let _ = player.manager.recover_if_needed();
        Some(f(player))
    })
}

/// Play the sample of `cue` (the built-in one unless replaced).
pub fn play_sound(cue: SoundCue) {
    with_player(|player| {
        if !player.sfx.contains_key(&cue) {
            match player.manager.create_sfx(builtin(cue), None) {
                Ok(sfx) => {
                    player.sfx.insert(cue, sfx);
                }
                Err(error) => {
                    eprintln!("mue: could not load the {cue:?} sound: {error:#}");
                    return;
                }
            }
        }
        let amplifier = player.volume;
        if let Some(sfx) = player.sfx.get_mut(&cue) {
            let _ = sfx.play(PlaySfxParams { amplifier });
        }
    });
}

/// Replace the sample of `cue` with an audio file's bytes.
pub fn set_sound_sample(cue: SoundCue, file: Vec<u8>) -> Result<(), String> {
    let clip = AudioClip::new(file).map_err(|error| format!("{error:#}"))?;
    with_player(|player| {
        let sfx = player
            .manager
            .create_sfx(clip, None)
            .map_err(|error| format!("{error:#}"))?;
        player.sfx.insert(cue, sfx);
        Ok(())
    })
    .unwrap_or_else(|| Err("the audio output is not available".into()))
}

/// Volume of all UI sounds, 0 (silent) to 1.
pub fn set_sound_volume(volume: f32) {
    with_player(|player| player.volume = volume.clamp(0., 1.));
}

/// A short synthesized sample per cue: quiet, dry, and under 0.2 s, in the
/// spirit of system UI sounds.
fn builtin(cue: SoundCue) -> AudioClip {
    // (frequency Hz, start s, length s, gain) partials, each with a fast
    // attack and an exponential decay.
    let tones: &[(f32, f32, f32, f32)] = match cue {
        SoundCue::Click => &[(1800., 0., 0.025, 0.25), (3600., 0., 0.012, 0.08)],
        SoundCue::ClickLarge => &[(1100., 0., 0.04, 0.3), (2200., 0., 0.02, 0.1)],
        SoundCue::Toggle => &[(1500., 0., 0.02, 0.22), (2000., 0.045, 0.025, 0.2)],
        SoundCue::Success => &[(880., 0., 0.09, 0.2), (1318.5, 0.07, 0.14, 0.2)],
        SoundCue::Error => &[(330., 0., 0.08, 0.25), (262., 0.09, 0.12, 0.25)],
    };
    let end = tones
        .iter()
        .map(|(_, start, length, _)| start + length * 3.)
        .fold(0., f32::max);
    let count = (end * RATE as f32) as usize;
    let frames = (0..count)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let value: f32 = tones
                .iter()
                .filter(|(_, start, _, _)| t >= *start)
                .map(|(freq, start, length, gain)| {
                    let local = t - start;
                    let attack = (local / 0.002).min(1.);
                    let decay = (-local / length).exp();
                    gain * attack * decay * (std::f32::consts::TAU * freq * local).sin()
                })
                .sum();
            Frame(value, value)
        })
        .collect();
    AudioClip::from_raw(frames, RATE)
}
