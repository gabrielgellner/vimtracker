//! A track's six mod slots: a type change glides rather than jumps and goes on as the new type,
//! the first envelope on `Vol` shapes the notes wherever it is, LFOs keep the random waves they had
//! before slots, and an envelope's depth can be locked and moved like an LFO's.
//! A slot moves its setting one way or both, as its polarity says.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::envelope::Adsr;
use vt_model::lfo::{Lfo, Rate, Shape, wave};
use vt_model::modulation::{ModKind, ModSlot, Polarity};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::Song;

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;
/// A quarter note, as ticks.
const QUARTER: u32 = 960;
/// Frames in a millisecond.
const MS: usize = 48;

fn flat() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    pool.insert(
        "s",
        SampleData {
            frames: vec![[1.0, 1.0]; RATE as usize * 20],
        },
    );
    Arc::new(pool)
}

fn version(song: &Song) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song.clone()), flat()))
}

/// One long note on a track held at `CUT 80`, with these steps after it.
fn song(texts: &[&str]) -> Song {
    let mut song = Song::new(1);
    for track in 0..song.sound_tracks() {
        song.sample_on(track, "s");
    }
    song.tracks[0].mods[0].adsr.attack_ms = 0;
    song.tracks[0].mods[0].adsr.release_ms = 0;
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    *phrase.step_mut(0).unwrap() = "C-3 -- --- --- ---".parse().unwrap();
    for (index, text) in texts.iter().enumerate() {
        *phrase.step_mut(index).unwrap() = text.parse().unwrap();
    }
    song.tracks[0].cutoff = 0x80;
    song
}

/// An envelope that goes straight to full and stays there while the note holds.
fn held() -> Adsr {
    Adsr {
        attack_ms: 0,
        decay_ms: 0,
        sustain: 100,
        ..Adsr::decay()
    }
}

/// A square a quarter long, up for two steps and down for two.
fn square() -> Lfo {
    Lfo {
        shape: Shape::Square,
        rate: Rate::Note(QUARTER),
        ..Lfo::default()
    }
}

/// Run an engine on for some frames, in blocks of this many, giving its live cutoff after each.
fn cutoffs(engine: &mut Engine, frames: usize, block: usize) -> Vec<u8> {
    let mut out = vec![0.0; block * 2];
    (0..frames / block)
        .map(|_| {
            engine.process(&mut out);
            engine.live(0).cutoff
        })
        .collect()
}

/// What a track is playing with after some steps from the top.
fn cutoff_after(song: &Song, steps: usize) -> u8 {
    let mut engine = Engine::new(RATE, version(song));
    engine.play_from(0);
    *cutoffs(&mut engine, STEP * steps, STEP).last().unwrap()
}

#[test]
fn a_slots_type_change_glides_from_the_old_type_to_the_new_one_and_goes_on_as_it() {
    // Slot 2 held full on `Cut` at `+40`, so `C0`, and then an LFO, whose square is down in its
    // third step, so `40`.
    let mut song = song(&[]);
    song.tracks[0].mods[1] = ModSlot::adsr(Some(SettingId::CUT), held()).with_depth(0x40);
    song.tracks[0].mods[1].lfo = square();
    let mut lfo_all_along = song.clone();
    lfo_all_along.tracks[0].mods[1].kind = ModKind::Lfo;

    let mut changed = Engine::new(RATE, version(&song));
    let mut always = Engine::new(RATE, version(&lfo_all_along));
    changed.play_from(0);
    always.play_from(0);
    let block = 500;
    let before = cutoffs(&mut changed, STEP * 2 + 2000, block);
    assert_eq!(*before.last().unwrap(), 0xC0);
    assert_eq!(
        *cutoffs(&mut always, STEP * 2 + 2000, block).last().unwrap(),
        0x40
    );

    song.tracks[0].mods[1].kind = ModKind::Lfo;
    changed.swap(version(&song));
    always.swap(version(&lfo_all_along));
    // In blocks of 16 frames over 10 ms: it glides down over the 5 ms a type change takes, `80`
    // in about eight updates of 32 frames, with no jump on the way, and lands within its next
    // update after that.
    let fading = cutoffs(&mut changed, 10 * MS, 16);
    let landed = fading.iter().position(|value| *value == 0x40).unwrap();
    assert!(landed >= 4, "it jumped: {fading:?}");
    assert!(landed * 16 <= 5 * MS + 32, "{fading:?}");
    for pair in fading[..=landed].windows(2) {
        assert!(pair[0] >= pair[1], "it turned back: {fading:?}");
        assert!(pair[0] - pair[1] <= 0x20, "a jump: {fading:?}");
    }
    // Then it's where an LFO that had always been there is, block by block, into the square's
    // next cycle.
    cutoffs(&mut always, 10 * MS, 16);
    let block = 700;
    assert_eq!(
        cutoffs(&mut changed, STEP * 4, block),
        cutoffs(&mut always, STEP * 4, block)
    );
}

#[test]
fn the_first_envelope_on_vol_shapes_the_notes_wherever_it_is() {
    let level = |song: &Song, frame: usize| render_frames(version(song), RATE, STEP)[frame * 2];
    // Slot 1 an LFO, and slot 3 an envelope on `Vol` with a 100 ms attack.
    let mut song = song(&[]);
    song.tracks[0].mods[0] = ModSlot::lfo(None, Lfo::default());
    song.tracks[0].mods[2] = ModSlot::adsr(
        Some(SettingId::VOL),
        Adsr {
            attack_ms: 100,
            ..Adsr::note_shape()
        },
    );
    assert!(level(&song, 10 * MS) < 0.01, "{}", level(&song, 10 * MS));
    assert!((level(&song, STEP - 1) - 1.0).abs() < 1e-3);

    // With slot 1 back to the plain envelope on `Vol` and switched out, notes have the plain shape:
    // the job isn't handed on to slot 3.
    song.tracks[0].mods[0] = ModSlot {
        bypassed: true,
        ..ModSlot::adsr(Some(SettingId::VOL), Adsr::note_shape())
    };
    assert!(
        (level(&song, 10 * MS) - 1.0).abs() < 1e-3,
        "{}",
        level(&song, 10 * MS)
    );
}

#[test]
fn an_lfo_keeps_the_random_wave_it_had_before_slots_and_each_slot_has_its_own() {
    // The value a random LFO in a slot gives in its first cycle, from the live values.
    let first_value = |slot: usize| {
        let mut song = song(&[]);
        song.tracks[0].mods[slot] = ModSlot::lfo(
            Some(SettingId::CUT),
            Lfo {
                shape: Shape::Random,
                rate: Rate::Note(QUARTER),
                ..Lfo::default()
            },
        )
        .with_depth(0x40);
        let mut engine = Engine::new(RATE, version(&song));
        engine.play_from(0);
        let mut out = vec![0.0; STEP * 2];
        engine.process(&mut out);
        (song.seed, engine.live(0).mods[slot].unwrap().value)
    };
    let expected = |key: u64| (wave(Shape::Random, 0.5, key) * f32::from(i16::MAX)).round() as i16;
    // Slot 4 was LFO 1, keyed by the track's index and 0.
    let (seed, value) = first_value(3);
    assert_eq!(value, expected(seed));
    // Slot 1 takes the key after LFO 3's.
    let (seed, value) = first_value(0);
    assert_eq!(value, expected(seed ^ 3));
    assert_ne!(value, expected(seed));
}

#[test]
fn a_step_locks_an_envelopes_depth() {
    // Slot 2 held full on `Cut` with no depth of its own, so it moves nothing until a step gives it
    // some. `9FE0` is a depth of +40.
    let mut locked = song(&["C-3 -- mod2.depth:9FE0 --- ---"]);
    locked.tracks[0].mods[1] = ModSlot::adsr(Some(SettingId::CUT), held());
    assert_eq!(cutoff_after(&locked, 1), 0xC0);
    let mut plain = locked.clone();
    *Arc::make_mut(plain.tracks[0].phrases.get_mut(&0).unwrap())
        .step_mut(0)
        .unwrap() = "C-3 -- --- --- ---".parse().unwrap();
    assert_eq!(cutoff_after(&plain, 1), 0x80);
}

#[test]
fn an_envelope_moves_a_later_envelopes_depth() {
    // Slot 2 gives slot 3, on `Cut` with no depth of its own, a depth of +20.
    let mut moved = song(&[]);
    moved.tracks[0].mods[1] =
        ModSlot::adsr(Some(SettingId::modulator(2, 0).unwrap()), held()).with_depth(0x20);
    moved.tracks[0].mods[2] = ModSlot::adsr(Some(SettingId::CUT), held());
    assert_eq!(cutoff_after(&moved, 1), 0xA0);
    // Pointing back round, from slot 3 to slot 2's depth, does nothing.
    let mut back = song(&[]);
    back.tracks[0].mods[1] = ModSlot::adsr(Some(SettingId::CUT), held());
    back.tracks[0].mods[2] =
        ModSlot::adsr(Some(SettingId::modulator(1, 0).unwrap()), held()).with_depth(0x20);
    assert_eq!(cutoff_after(&back, 1), 0x80);
}

/// An envelope that goes straight to full and then straight to nothing, while the note holds.
fn gone() -> Adsr {
    Adsr {
        attack_ms: 0,
        decay_ms: 0,
        sustain: 0,
        ..Adsr::decay()
    }
}

#[test]
fn a_unipolar_lfo_moves_its_setting_one_way_only() {
    // The square on `Cut` at `+40`: both ways, `C0` while up and `40` while down; one way, `C0`
    // while up and back at `80` while down.
    let mut both = song(&[]);
    both.tracks[0].mods[3] = ModSlot::lfo(Some(SettingId::CUT), square()).with_depth(0x40);
    assert_eq!(
        both.tracks[0].mods[3].polarity(),
        Polarity::Bi,
        "an LFO's own"
    );
    assert_eq!(
        (cutoff_after(&both, 1), cutoff_after(&both, 3)),
        (0xC0, 0x40)
    );
    let mut one = both.clone();
    one.tracks[0].mods[3].polarity = Some(Polarity::Uni);
    assert_eq!((cutoff_after(&one, 1), cutoff_after(&one, 3)), (0xC0, 0x80));
}

#[test]
fn a_bipolar_envelope_is_centred_on_its_setting() {
    // Fallen to nothing, an envelope at `+40` leaves `Cut` at `80` one way, and takes it to `40`
    // both ways; held full, it's `C0` either way.
    let mut one = song(&[]);
    one.tracks[0].mods[1] = ModSlot::adsr(Some(SettingId::CUT), gone()).with_depth(0x40);
    assert_eq!(
        one.tracks[0].mods[1].polarity(),
        Polarity::Uni,
        "an envelope's own"
    );
    assert_eq!(cutoff_after(&one, 1), 0x80);
    let mut both = one.clone();
    both.tracks[0].mods[1].polarity = Some(Polarity::Bi);
    assert_eq!(cutoff_after(&both, 1), 0x40);
    both.tracks[0].mods[1].adsr = held();
    assert_eq!(cutoff_after(&both, 1), 0xC0);
}

#[test]
fn a_step_locks_a_slots_polarity() {
    // The fallen envelope, one way of its own, made both ways for its note by a step.
    let mut locked = song(&["C-3 -- mod2.polarity:01 --- ---"]);
    locked.tracks[0].mods[1] = ModSlot::adsr(Some(SettingId::CUT), gone()).with_depth(0x40);
    assert_eq!(cutoff_after(&locked, 1), 0x40);
    assert_eq!(
        SettingId::modulator(1, 2).unwrap().label(),
        Some("POL2".to_owned())
    );
}

#[test]
fn an_ahd_on_cut_holds_for_its_time_and_plays_through_a_notes_end() {
    use vt_model::envelope::{Ahd, Hold};
    // Slot 2 an AHD on `Cut` at `+40`, held 300 ms, with no attack or decay: `C0` while it holds,
    // past the note's end at a step, and `80` once it's done, though the next note is a while off.
    let mut song = song(&["C-3 -- --- --- ---", "OFF -- --- --- ---"]);
    let punch = Ahd {
        attack_ms: 0,
        hold: Hold::Ms(300),
        decay_ms: 0,
        ..Ahd::default()
    };
    song.tracks[0].mods[1] = ModSlot::ahd(Some(SettingId::CUT), punch).with_depth(0x40);
    let mut engine = Engine::new(RATE, version(&song));
    engine.play_from(0);
    let block = 50 * MS;
    let found = cutoffs(&mut engine, 500 * MS, block);
    assert_eq!(found[1], 0xC0, "{found:?}");
    assert_eq!(found[4], 0xC0, "after the note's end at 125 ms: {found:?}");
    assert_eq!(found[9], 0x80, "{found:?}");
    // Held for the note instead, the note's end starts its decay.
    song.tracks[0].mods[1].ahd.hold = Hold::Note;
    let mut engine = Engine::new(RATE, version(&song));
    engine.play_from(0);
    let found = cutoffs(&mut engine, 500 * MS, block);
    assert_eq!(found[1], 0xC0, "{found:?}");
    assert_eq!(found[4], 0x80, "{found:?}");
}
