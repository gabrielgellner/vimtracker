//! A mod slot's own times, levels and rates locked by a step (decision 407): a lock on a note's
//! row is heard by that note's start of the slot, exactly as the slot written that way, through a
//! `MAC` lock on a macro holding one too; the next note ends it, as any lock; and a render repeats.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::envelope::Adsr;
use vt_model::lfo::{Lfo, LfoMode, Rate, Shape};
use vt_model::macros::{MacroMap, MacroTarget};
use vt_model::modulation::{ModField, ModKind, ModSlot};
use vt_model::settings::SettingId;
use vt_model::song::Song;

use crate::support::steady::flat;

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;

/// One track of a flat sample playing these steps, an empty text a step left empty, with these
/// slots in its first mods.
fn song(texts: &[&str], mods: &[ModSlot]) -> Song {
    let mut song = Song::new(1);
    song.sample_on(0, "s");
    let track = &mut song.tracks[0];
    track.cutoff = 0x80;
    track.mods[..mods.len()].copy_from_slice(mods);
    let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
    for (index, text) in texts
        .iter()
        .enumerate()
        .filter(|(_, text)| !text.is_empty())
    {
        *phrase.step_mut(index).unwrap() = text.parse().unwrap();
    }
    song
}

fn version(song: &Song) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song.clone()), flat()))
}

fn render(song: &Song, frames: usize) -> Vec<f32> {
    render_frames(version(song), RATE, frames)
}

/// Where a slot's own setting is among its type's, by its field.
fn place(kind: ModKind, field: ModField) -> usize {
    field.own(kind).unwrap()
}

/// A slot's own setting of its type, by its field.
fn own(slot: usize, kind: ModKind, field: ModField) -> SettingId {
    SettingId::mod_own(slot, place(kind, field)).unwrap()
}

/// A slot with one of its own settings written at a value on the registry's scale.
fn written(slot: ModSlot, field: ModField, value: f32) -> ModSlot {
    let mut slot = slot;
    slot.set_own(place(slot.kind, field), value);
    slot
}

/// The notes' shape: an ADSR on `Vol` in Mod 1, falling to nothing.
fn amp() -> ModSlot {
    ModSlot::adsr(
        Some(SettingId::VOL),
        Adsr {
            attack_ms: 0,
            decay_ms: 40,
            sustain: 0,
            release_ms: 10,
            ..Adsr::default()
        },
    )
}

/// An ADSR on `Cut` in Mod 2, rising slowly.
fn filter() -> ModSlot {
    ModSlot::adsr(
        Some(SettingId::CUT),
        Adsr {
            attack_ms: 10,
            decay_ms: 200,
            sustain: 30,
            release_ms: 100,
            ..Adsr::default()
        },
    )
    .with_depth(0x60)
}

fn refs(texts: &[String; 16]) -> [&str; 16] {
    texts.each_ref().map(String::as_str)
}

/// The first frame two renders differ at, if they do.
fn first_difference(a: &[f32], b: &[f32]) -> Option<usize> {
    a.iter()
        .zip(b)
        .position(|(a, b)| a.to_bits() != b.to_bits())
        .map(|at| at / 2)
}

/// A lock of one of Mod `slot`'s own settings on the first of two notes eight rows apart is heard
/// through the first note exactly as the slot written that way, and differs from the slot as it
/// is; the second note, with no lock, is the song's own again.
fn assert_lock_is_its_notes(slots: &[ModSlot], slot: usize, field: ModField, value: u8) {
    let id = own(slot, slots[slot].kind, field);
    assert!(id.lockable(), "{} has a label", id.key());
    let note = "C-3 -- --- --- ---";
    let lock = format!("C-3 -- {}:{value:02X} --- ---", id.key());
    let texts = |first: &str| {
        let mut texts = [""; 16];
        texts[0] = first;
        texts[8] = note;
        texts.map(str::to_owned)
    };
    let (locked, plain) = (texts(&lock), texts(note));
    let mut wrote = slots.to_vec();
    wrote[slot] = written(slots[slot], field, f32::from(value));
    let frames = 16 * STEP;
    let heard = render(&song(&refs(&locked), slots), frames);
    let meant = render(&song(&refs(&plain), &wrote), frames);
    let unlocked = render(&song(&refs(&plain), slots), frames);
    let first = 8 * STEP;
    assert_eq!(
        first_difference(&heard[..first * 2], &meant[..first * 2]),
        None,
        "{}'s lock is the first note's, from its first frame",
        id.key()
    );
    assert!(
        first_difference(&heard[..first * 2], &unlocked[..first * 2]).is_some(),
        "{}'s lock moves nothing",
        id.key()
    );
    // The second note ends the lock: from a little after its start, once the first note's tail and
    // the smoothers have settled, it's as the song has it.
    let settled = first + STEP / 2;
    let most = heard[settled * 2..]
        .iter()
        .zip(&unlocked[settled * 2..])
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f32::max);
    assert!(
        most < 1e-4,
        "{}'s lock outlives its note by {most}",
        id.key()
    );
}

/// The commonest p-lock: the notes' decay per step.
#[test]
fn a_decay_lock_shapes_its_own_note() {
    assert_lock_is_its_notes(&[amp()], 0, ModField::Decay, 0xA0);
    assert_lock_is_its_notes(&[amp(), filter()], 1, ModField::Decay, 0x60);
}

#[test]
fn an_attack_lock_shapes_its_own_note() {
    assert_lock_is_its_notes(&[amp(), filter()], 1, ModField::Attack, 0xB0);
    assert_lock_is_its_notes(&[amp()], 0, ModField::Attack, 0x90);
}

#[test]
fn sustain_and_release_locks_shape_their_own_note() {
    let held = ModSlot::adsr(
        Some(SettingId::VOL),
        Adsr {
            sustain: 50,
            ..amp().adsr
        },
    );
    assert_lock_is_its_notes(&[held, filter()], 1, ModField::Sustain, 80);
    assert_lock_is_its_notes(&[held], 0, ModField::Sustain, 10);
}

/// An LFO's `Rate` locked on a note's row runs at that rate from the row, its cycle started there
/// by the note, as the LFO written with that rate; before the row it's as it was.
#[test]
fn a_rate_lock_runs_an_lfo_at_its_rate_from_its_row() {
    let lfo = ModSlot::lfo(
        Some(SettingId::CUT),
        Lfo {
            shape: Shape::Ramp,
            rate: Rate::Note(3840),
            mode: LfoMode::Trig,
            ..Lfo::default()
        },
    )
    .with_depth(0x40);
    let mods = [lfo];
    let id = own(0, ModKind::Lfo, ModField::Rate);
    assert_eq!(id.label().as_deref(), Some("RATE1"));
    let value: u8 = 0xC0;
    let note = "C-3 -- --- --- ---";
    let lock = format!("C-3 -- {}:{value:02X} --- ---", id.key());
    let mut texts = [""; 12];
    texts[0] = note;
    texts[4] = &lock;
    texts[8] = note;
    let locked = song(&texts, &mods);
    let mut plain = [""; 12];
    plain[0] = note;
    plain[4] = note;
    plain[8] = note;
    let unlocked = song(&plain, &mods);
    let mut wrote = mods;
    wrote[0] = written(lfo, ModField::Rate, f32::from(value));
    let as_written = song(&plain, &wrote);
    assert_ne!(wrote[0].lfo.rate, lfo.lfo.rate);
    let mut engines = [&locked, &unlocked, &as_written].map(|song| {
        let mut engine = Engine::new(RATE, version(song));
        engine.play_from(0);
        engine
    });
    let mut played = 0;
    for at in 1..12 * 16 {
        let frame = at * STEP / 16;
        let mut out = vec![0.0; (frame + 1 - played) * 2];
        let [heard, plain, meant] = engines.each_mut().map(|engine| {
            engine.process(&mut out);
            engine.live(0).mods[0].expect("the LFO runs").phase
        });
        played = frame + 1;
        match frame {
            frame if (4 * STEP..8 * STEP).contains(&frame) => {
                assert_eq!(heard, meant, "{frame}: locked against written");
            }
            _ => assert_eq!(heard, plain, "{frame}: locked against the song's own"),
        }
    }
}

/// A `MAC` lock on a macro holding a slot's decay is heard by the note on its row exactly, as a
/// lock of the decay itself at the value the macro puts there is.
#[test]
fn a_macro_lock_on_a_decay_is_its_own_notes() {
    let decay = own(0, ModKind::Adsr, ModField::Decay);
    let map = MacroMap {
        target: MacroTarget::Setting(decay),
        min: 0x4000,
        max: 0xC000,
        curve: 0,
        s: 0,
        zone: Default::default(),
    };
    assert_eq!(map.at(255.0), 0xC000);
    let with_macro = |row: &str| {
        let mut song = song(&["C-3 -- --- --- ---", "", "", "", row], &[amp()]);
        song.tracks[0].macros[0].maps = vec![map];
        song
    };
    // On the second of two notes, so it's no start of play's that puts it.
    let frames = 12 * STEP;
    let [turned, locked, plain] = [
        "C-3 -- macro1:FF --- ---",
        "C-3 -- mod1.decay:C0 --- ---",
        "C-3 -- --- --- ---",
    ]
    .map(|row| render(&with_macro(row), frames));
    assert_eq!(first_difference(&turned, &locked), None);
    assert!(
        first_difference(&turned, &plain).is_some_and(|at| (4 * STEP..5 * STEP).contains(&at)),
        "the lock moves its own note from its row"
    );
}

/// Each of a slot's own times, levels and rates locks, labelled with the slot's number.
#[test]
fn every_own_setting_of_a_slot_is_labelled() {
    let labels = [
        (ModKind::Adsr, ModField::Attack, "EATK2"),
        (ModKind::Adsr, ModField::Decay, "EDEC2"),
        (ModKind::Adsr, ModField::Sustain, "ESUS2"),
        (ModKind::Adsr, ModField::Release, "EREL2"),
        (ModKind::Drum, ModField::Body, "BODY2"),
        (ModKind::Lpg, ModField::Colour, "CLR2"),
        (ModKind::Steps, ModField::Length, "LEN2"),
        (ModKind::Steps, ModField::Mutate, "MUT2"),
        (ModKind::Steps, ModField::Smooth, "SMTH2"),
        (ModKind::Steps, ModField::Rate, "RATE2"),
        (ModKind::Steps, ModField::Jump, "JMP2"),
        (ModKind::Steps, ModField::Spread, "SPR2"),
        (ModKind::Steps, ModField::Bias, "BIAS2"),
    ];
    for (kind, field, label) in labels {
        let id = own(1, kind, field);
        assert!(id.lockable(), "{label}");
        assert_eq!(id.label().as_deref(), Some(label));
    }
}

#[test]
fn renders_with_locked_times_repeat() {
    let lock = "C-3 -- mod2.decay:60 mod1.attack:40 ---";
    let texts = [lock, "", "", "", "C-3 -- --- --- ---"];
    let song = song(&texts, &[amp(), filter()]);
    let frames = 8 * STEP;
    assert_eq!(render(&song, frames), render(&song, frames));
}
