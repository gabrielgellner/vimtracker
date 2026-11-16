//! A track's `Mode`: `Mono`, where each note strikes, as every track did before it could choose,
//! and `Legato`, `Mono` with one rule more: a note arriving while the one sounding still has its
//! gate open ties into it as a `HLD` does (decision 380).

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::{SettingId, glide_value};
use vt_model::song::{FULL_GATE, Machine, Phrase, Song, SynthVoice, VoiceMode};

const RATE: u32 = 48_000;
/// Frames in a step at 120 BPM.
const STEP: usize = 6000;

/// A slow ramp for the sampler to play, so its pitch is how fast it climbs.
fn pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    let frames = RATE as usize * 4;
    let ramp: Vec<[f32; 2]> = (0..frames)
        .map(|at| {
            let level = at as f32 / frames as f32;
            [level, level]
        })
        .collect();
    let fall = ramp.iter().rev().copied().collect();
    pool.insert("ramp", SampleData { frames: ramp });
    pool.insert("fall", SampleData { frames: fall });
    Arc::new(pool)
}

/// One track on this machine playing these rows, in this mode.
fn song(machine: Machine, mode: VoiceMode, rows: &[&str]) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.sample_on(0, "ramp");
    let track = &mut song.tracks[0];
    track.machine = machine;
    track.voice_mode = mode;
    let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
    track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    song
}

/// The left channel, for as many steps as the song has rows, so the phrase doesn't come round.
fn render(song: &Song) -> Vec<f32> {
    let steps = song.tracks[0].phrases[&0].steps().len();
    let version = Arc::new(Published::new(Arc::new(song.clone()), pool()));
    render_frames(version, RATE, steps * STEP)
        .chunks(2)
        .map(|frame| frame[0])
        .collect()
}

/// A holding synth, which carries its voice in `Mono`, a struck one, which starts each note
/// under the last one's fade, and the sampler.
const MACHINES: [Machine; 3] = [
    Machine::Plaits(SynthVoice::VirtualAnalog),
    Machine::Plaits(SynthVoice::Kick),
    Machine::Sampler,
];

/// Two notes, the first held to the second at the machines' own full `Gate`.
const OVERLAPPING: [&str; 4] = ["C-3 7F", "E-3 40", "--- --", "--- --"];
/// The same, tied by a `HLD`.
const HELD: [&str; 4] = ["C-3 7F HLD1", "E-3 40", "--- --", "--- --"];

#[test]
fn mono_is_what_a_track_played_before_it_could_choose() {
    // A song that doesn't set it plays `Mono`, and `Mono` is the engine as it was: its goldens
    // don't move.
    for machine in MACHINES {
        let mut unset = song(machine, VoiceMode::Mono, &OVERLAPPING);
        unset.tracks[0].voice_mode = VoiceMode::default();
        assert_eq!(VoiceMode::default(), VoiceMode::Mono);
        assert_eq!(
            render(&unset),
            render(&song(machine, VoiceMode::Mono, &OVERLAPPING))
        );
    }
}

#[test]
fn legato_ties_a_note_into_one_whose_gate_is_open() {
    // At full `Gate` the first note is still held as the second comes, so `Legato` ties it as a
    // `HLD` does, sample for sample, where `Mono` strikes it.
    for machine in MACHINES {
        let legato = render(&song(machine, VoiceMode::Legato, &OVERLAPPING));
        let held = render(&song(machine, VoiceMode::Mono, &HELD));
        let mono = render(&song(machine, VoiceMode::Mono, &OVERLAPPING));
        assert_eq!(legato, held, "{machine:?} ties as a HLD does");
        assert_ne!(legato, mono, "{machine:?} doesn't strike");
    }
}

#[test]
fn legato_strikes_once_the_gate_has_closed() {
    // A short `LEN` on the row, an `OFF` between the notes, or a short `Gate` on the track lets the
    // note go before the next comes, which then strikes as in `Mono`.
    let short_row = ["C-3 7F gate:40", "E-3 40", "--- --", "--- --"];
    let off = ["C-3 7F", "OFF --", "E-3 40", "--- --"];
    for machine in MACHINES {
        for rows in [&short_row[..], &off[..]] {
            let legato = render(&song(machine, VoiceMode::Legato, rows));
            let mono = render(&song(machine, VoiceMode::Mono, rows));
            assert_eq!(legato, mono, "{machine:?} {rows:?}");
        }
        let short_gate = |mode| {
            let mut short = song(machine, mode, &OVERLAPPING);
            short.tracks[0].gate_length = Some(0x80);
            render(&short)
        };
        assert_eq!(
            short_gate(VoiceMode::Legato),
            short_gate(VoiceMode::Mono),
            "{machine:?} with a short Gate"
        );
    }
}

#[test]
fn ratchets_strike_in_legato() {
    // A `RET`'s hits come while the gate's open, but a ratchet is there to be heard: each strikes
    // as in `Mono`.
    for machine in MACHINES {
        let rows = ["C-3 7F ret:02", "--- --", "--- --"];
        let legato = render(&song(machine, VoiceMode::Legato, &rows));
        let mono = render(&song(machine, VoiceMode::Mono, &rows));
        assert_eq!(legato, mono, "{machine:?}");
    }
}

#[test]
fn a_legato_tie_glides_as_a_hld_does() {
    // With `Glide` set, the tie glides into the new note's pitch as a `HLD`'s does.
    for machine in MACHINES {
        let gliding = |mode, rows: &[&str], ms: f32| {
            let mut gliding = song(machine, mode, rows);
            gliding.tracks[0].glide = glide_value(ms);
            render(&gliding)
        };
        let legato = gliding(VoiceMode::Legato, &OVERLAPPING, 200.0);
        let held = gliding(VoiceMode::Mono, &HELD, 200.0);
        assert_eq!(legato, held, "{machine:?} glides as a HLD does");
        let jumping = render(&song(machine, VoiceMode::Legato, &OVERLAPPING));
        assert_ne!(legato, jumping, "{machine:?} glides");
    }
}

#[test]
fn a_step_can_lock_it_for_its_note() {
    // `VMOD01` on a `Mono` track's row ties that row's note; the note after, with no lock,
    // strikes again.
    for machine in MACHINES {
        let locked = ["C-3 7F", "E-3 40 voicemode:01", "G-3 7F", "--- --"];
        let held = ["C-3 7F HLD1", "E-3 40", "G-3 7F", "--- --"];
        let locked = render(&song(machine, VoiceMode::Mono, &locked));
        let held = render(&song(machine, VoiceMode::Mono, &held));
        assert_eq!(locked, held, "{machine:?}");
    }
}

#[test]
fn a_new_sample_strikes_in_legato() {
    // A tie carries on what's sounding, so a row whose lock plays another entry strikes it.
    let rows = ["C-3 7F", "C-3 7F sampler.sample:02", "--- --", "--- --"];
    let mut two = song(Machine::Sampler, VoiceMode::Legato, &rows);
    assert_eq!(two.sample_on(0, "fall"), Some(2));
    two.tracks[0].set_setting(SettingId::SAMPLE, 1);
    let legato = render(&two);
    two.tracks[0].voice_mode = VoiceMode::Mono;
    assert_eq!(legato, render(&two));
}

#[test]
fn acid_slides_in_legato_only_where_its_notes_overlap() {
    // `Acid`'s own `Gate` lets each note go before the next, so in `Legato` it carries on note to
    // note as in `Mono`; held to the next note, a note ties into it, which on a 303 is a slide.
    let acid = Machine::Plaits(SynthVoice::Acid);
    let legato = render(&song(acid, VoiceMode::Legato, &OVERLAPPING));
    assert_eq!(legato, render(&song(acid, VoiceMode::Mono, &OVERLAPPING)));
    let full = |mode, rows: &[&str]| {
        let mut full = song(acid, mode, rows);
        full.tracks[0].gate_length = Some(FULL_GATE);
        render(&full)
    };
    let legato = full(VoiceMode::Legato, &OVERLAPPING);
    assert_eq!(legato, full(VoiceMode::Mono, &HELD));
    assert_ne!(legato, full(VoiceMode::Mono, &OVERLAPPING));
}
