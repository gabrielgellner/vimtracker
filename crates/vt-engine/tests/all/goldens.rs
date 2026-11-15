//! A golden render of every effect and every voice that no other file pins (#809), and a check
//! that each effect and each voice has one, so a new one can't come without it.

use std::path::PathBuf;
use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_loop};
use vt_model::insert::{
    Comb, CombMode, Delay, EffectKind, Equaliser, Flanger, FreqShift, GateMode, Insert, Limiter,
    LowPassGate, MOST_SHIFT, Phaser, Pitch, Resonator, Tape, Vowel, Warps,
};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::{MAX_MACHINE_SETTINGS, SettingId, voice_key};
use vt_model::song::{Machine, Phrase, Song, SynthVoice};

use crate::support::golden;

const RATE: u32 = 48_000;
/// Every render's, so whatever draws random numbers draws the same ones.
const SEED: u64 = 1;

/// What an effect's goldens are called: `effect-<key>` here, or the name the file that pinned it
/// first gave it. The check below looks for a golden under it.
fn effect_golden(kind: EffectKind) -> String {
    let named = match kind {
        EffectKind::Distort => "sides-",
        EffectKind::Reverb => "reverb-clouds",
        EffectKind::Plate => "reverb-plate",
        EffectKind::Spring => "reverb-spring",
        EffectKind::Space => "reverb-space",
        EffectKind::Compressor => "comp-",
        EffectKind::Chorus
        | EffectKind::Grains
        | EffectKind::Rotary
        | EffectKind::Exciter
        | EffectKind::Stutter
        | EffectKind::Clouds
        | EffectKind::Beads
        | EffectKind::Echo => kind.key(),
        _ => return format!("effect-{}", kind.key()),
    };
    named.to_owned()
}

/// What a voice's golden is called: `voice-<key>` here, or the name the file that pinned it first
/// gave it.
fn voice_golden(voice: SynthVoice) -> String {
    match voice {
        SynthVoice::Supersaw => "supersaw".to_owned(),
        SynthVoice::FourOp => "four-op".to_owned(),
        _ if voice.is_macro() || voice == SynthVoice::Speech => {
            format!("macro-{}", voice_key(voice))
        }
        _ => format!("voice-{}", voice_key(voice)),
    }
}

/// An effect with its settings moved from where it starts, to where its DSP does more: fed back,
/// swept faster or deeper, driven, tuned away from the note, and partly dry.
fn moved(kind: EffectKind) -> Insert {
    match kind {
        EffectKind::Delay => Insert::Delay(Delay {
            time: 7,
            feedback: 0xB0,
            ping_pong: 0,
            width: 0x60,
            low_cut: 0x40,
            high_cut: 0x90,
            drive: 0x80,
            wow: 0x80,
            mix: 0xC0,
            ..Delay::DEFAULT
        }),
        EffectKind::Eq => Insert::Eq(Equaliser {
            low_frequency: 0x50,
            low: 0xC0,
            mid_frequency: 0x70,
            mid: 0x40,
            mid_q: 0xA0,
            high_frequency: 0xC0,
            high: 0xB0,
            ..Equaliser::DEFAULT
        }),
        EffectKind::Limit => Insert::Limit(Limiter {
            input: 0xC0,
            ceiling: 0x80,
            release: 0x40,
            ..Limiter::DEFAULT
        }),
        EffectKind::Phaser => Insert::Phaser(Phaser {
            rate: 12,
            depth: 0xE0,
            centre: 0x60,
            feedback: 0xE0,
            stages: 2,
            ..Phaser::DEFAULT
        }),
        EffectKind::Flanger => Insert::Flanger(Flanger {
            rate: 13,
            depth: 0xFF,
            delay: 0x40,
            feedback: 0x20,
            ..Flanger::DEFAULT
        }),
        EffectKind::Tape => Insert::Tape(Tape {
            drive: 0xE0,
            tone: 0x50,
            bias: 0x30,
            bump: 0xC0,
            wow: 0xC0,
            flutter: 0xA0,
            ..Tape::DEFAULT
        }),
        EffectKind::Pitch => Insert::Pitch(Pitch {
            shift: MOST_SHIFT + 7,
            fine: 0xA0,
            size: 0x60,
            feedback: 0x60,
            mix: 0xC0,
            ..Pitch::DEFAULT
        }),
        EffectKind::Warps => Insert::Warps(Warps {
            algorithm: 0xD0,
            timbre: 0x40,
            level: 0xE0,
            pitch: MOST_SHIFT - 5,
            shape: 1,
            mix: 0xC0,
            ..Warps::DEFAULT
        }),
        EffectKind::Lpg => Insert::Lpg(LowPassGate {
            mode: GateMode::Filter,
            amount: 0xFF,
            base: 0x20,
            resonance: 0xC0,
            response: 0x90,
            ..LowPassGate::DEFAULT
        }),
        EffectKind::Reso => Insert::Reso(Resonator {
            tune: MOST_SHIFT + 12,
            material: 0xB0,
            bright: 0x40,
            decay: 0xD0,
            position: 0x80,
            width: 0xFF,
            mix: 0xC0,
            ..Resonator::DEFAULT
        }),
        EffectKind::Comb => Insert::Comb(Comb {
            mode: CombMode::Flange,
            tune: MOST_SHIFT - 12,
            feedback: 0xA0,
            tone: 0x40,
            spread: 0x40,
            mix: 0xC0,
            ..Comb::DEFAULT
        }),
        EffectKind::Vowel => Insert::Vowel(Vowel {
            vowel: 0xA0,
            size: 0x50,
            resonance: 0xE0,
            track: 0xFF,
            mix: 0xC0,
            ..Vowel::DEFAULT
        }),
        EffectKind::FreqShift => Insert::FreqShift(FreqShift {
            shift: 0x50,
            spread: 0x60,
            track: 0x80,
            time: 0x30,
            feedback: 0x90,
            mix: 0xC0,
            ..FreqShift::DEFAULT
        }),
        _ => panic!("{kind:?} needs its settings moved here, for its golden"),
    }
}

/// Half a second of two saws a fifth apart and a burst of noise, from a fixed seed: the same on
/// both sides for its first quarter, then the right side apart from the left, and held at a third
/// of its level to the end.
fn pool() -> Arc<SamplePool> {
    let mut seed = 0x1234_5678_u32;
    let frames = (0..RATE as usize / 2)
        .map(|frame| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let noise = seed as f32 / u32::MAX as f32 * 2.0 - 1.0;
            let t = frame as f32 / RATE as f32;
            let saw = |hertz: f32| 2.0 * (t * hertz).fract() - 1.0;
            let body = 0.3 + 0.7 * (-t * 8.0).exp();
            let left = (saw(110.0) + saw(165.0)) * 0.15 * body + noise * 0.2 * (-t * 30.0).exp();
            let right = if frame < RATE as usize / 8 {
                left
            } else {
                (saw(110.5) * 0.2 + saw(220.0) * 0.1) * body
            };
            [left, right]
        })
        .collect();
    let mut pool = SamplePool::default();
    pool.insert("s", SampleData { frames });
    Arc::new(pool)
}

/// Two short notes, then one held to the end of the line.
const NOTES: [&str; 8] = [
    "C-3 -- --- --- ---",
    "--- -- --- --- ---",
    "D#3 50 --- --- ---",
    "--- -- --- --- ---",
    "G-2 -- --- --- ---",
    "--- -- --- --- ---",
    "--- -- --- --- ---",
    "--- -- --- --- ---",
];

/// A loop of the notes through this effect in the first slot.
fn effect_loop(insert: Insert) -> Vec<f32> {
    let mut song = Song::new(1);
    song.seed = SEED;
    for track in 0..song.sound_tracks() {
        song.sample_on(track, "s");
    }
    song.tracks[0].inserts[0] = insert;
    let steps = NOTES.iter().map(|text| text.parse().unwrap()).collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let rendered = render_loop(
        Arc::new(Published::new(Arc::new(song), pool())),
        RATE,
        RenderOptions::default(),
    );
    assert!(
        rendered.iter().all(|sample| sample.is_finite()),
        "{insert:?}"
    );
    rendered
}

#[test]
fn golden_every_effect_stays_the_same() {
    let unpinned = EffectKind::ALL
        .into_iter()
        .filter(|kind| effect_golden(*kind).starts_with("effect-"));
    let dry = effect_loop(Insert::Empty);
    for kind in unpinned {
        let name = effect_golden(kind);
        let fresh = Insert::ALL[kind as usize + 1];
        assert_eq!(fresh.kind(), Some(kind));
        let (fresh, moved) = (effect_loop(fresh), effect_loop(moved(kind)));
        // Moved, each changes the sound and does something its defaults don't. A flat EQ, as it
        // starts, leaves the sound as it is.
        assert!(moved != dry && moved != fresh, "{kind:?}");
        golden(&name, &fresh);
        golden(&format!("{name}-moved"), &moved);
    }
}

/// A short line on a voice, as `macro_voices.rs` plays one: a low note, a high one with its first
/// setting a step can lock moved halfway, and one with its second moved high. With `moved`,
/// every setting starts away from its default too, half its range round from it.
fn voice_loop(voice: SynthVoice, moved: bool) -> Vec<f32> {
    let ids: Vec<SettingId> = (0..MAX_MACHINE_SETTINGS)
        .map_while(|at| SettingId::voice(voice, at))
        .collect();
    let mut lockable = ids.iter().filter(|id| id.entry().label.is_some());
    let (first, second) = (lockable.next().unwrap(), lockable.next().unwrap());
    let steps = [
        "C-1 7F --- --- ---".to_owned(),
        "--- -- --- --- ---".to_owned(),
        format!(
            "G-2 60 {}:{:02X} --- ---",
            first.key(),
            first.entry().kind.top().div_ceil(2)
        ),
        format!("C-2 7F {}:E0 --- ---", second.key()),
    ];
    let mut song = Song::new(1);
    song.seed = SEED;
    song.limiter = false;
    song.tracks[0].machine = Machine::Plaits(voice);
    if moved {
        for (at, id) in ids.iter().enumerate() {
            let entry = id.entry();
            let around = u16::from(entry.kind.top()) + 1;
            let value = (u16::from(entry.default) + around / 2) % around;
            song.tracks[0].set_machine_value(at, value as u8);
        }
    }
    let steps = steps.iter().map(|text| text.parse().unwrap()).collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let rendered = render_loop(
        Arc::new(Published::new(
            Arc::new(song),
            Arc::new(SamplePool::default()),
        )),
        RATE,
        RenderOptions::default(),
    );
    assert!(
        rendered.iter().all(|sample| sample.is_finite()),
        "{voice:?}"
    );
    assert!(rendered.iter().any(|sample| *sample != 0.0), "{voice:?}");
    rendered
}

#[test]
fn golden_every_voice_stays_the_same() {
    let unpinned = SynthVoice::ALL
        .into_iter()
        .filter(|voice| voice_golden(*voice).starts_with("voice-"));
    for voice in unpinned {
        let mut rendered = voice_loop(voice, false);
        let moved = voice_loop(voice, true);
        assert!(moved != rendered, "{voice:?} moved");
        rendered.extend(moved);
        golden(&voice_golden(voice), &rendered);
    }
}

/// The goldens there are, by name without `.hash`.
fn goldens() -> Vec<String> {
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    std::fs::read_dir(folder)
        .unwrap()
        .filter_map(|entry| {
            let name = entry.unwrap().file_name().into_string().unwrap();
            name.strip_suffix(".hash").map(str::to_owned)
        })
        .collect()
}

/// Whether there's a golden by this name, or, for a name ending in `-`, one that starts with it.
fn has_golden(goldens: &[String], name: &str) -> bool {
    goldens.iter().any(|golden| {
        golden == name || golden.starts_with(&format!("{}-", name.trim_end_matches('-')))
    })
}

#[test]
fn every_effect_and_every_voice_has_a_golden() {
    let goldens = goldens();
    let effects: Vec<EffectKind> = EffectKind::ALL
        .into_iter()
        .filter(|kind| !has_golden(&goldens, &effect_golden(*kind)))
        .collect();
    assert!(
        effects.is_empty(),
        "no golden for {effects:?}: give it settings in `moved` and run with VT_BLESS=1"
    );
    let voices: Vec<SynthVoice> = SynthVoice::ALL
        .into_iter()
        .filter(|voice| !has_golden(&goldens, &voice_golden(*voice)))
        .collect();
    assert!(
        voices.is_empty(),
        "no golden for {voices:?}: run with VT_BLESS=1"
    );
}
