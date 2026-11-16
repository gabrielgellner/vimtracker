//! A track's `Glide mode`: at `Ties`, as every track played before it could choose, only a tie
//! glides; at `Always`, a struck note's pitch glides too, from the note still sounding on the same
//! synth or sample, as a tracker's `3xx` does, while its envelope strikes as `Mode` and `Reset` say
//! (decision 382).

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::{SettingId, glide_value};
use vt_model::song::{FULL_GATE, GlideMode, Machine, Phrase, Song, SynthVoice, VoiceMode};

const RATE: u32 = 48_000;
/// Frames per step at 120 BPM.
const STEP: usize = 6000;
/// Frames in a ms.
const MS: usize = RATE as usize / 1000;
/// The ramp the sample plays, so a note's pitch is how fast it climbs.
const RAMP_FRAMES: usize = RATE as usize * 8;
/// A glide's time constant is a third of its time, so it's halfway there in this share of it.
const HALFWAY: f64 = std::f64::consts::LN_2 / 3.0;
/// `C-4` and `C-5`, in hertz.
const LOW: f64 = 261.63;
const HIGH: f64 = 523.25;

/// A holding synth, which carries its voice, a struck one, which starts each note under the last
/// one's fade, and the sampler.
const MACHINES: [Machine; 3] = [
    Machine::Plaits(SynthVoice::VirtualAnalog),
    Machine::Plaits(SynthVoice::Kick),
    Machine::Sampler,
];

/// Two struck notes an octave apart, the first held to the second.
const STRUCK: [&str; 4] = ["C-3 7F", "C-4 7F", "--- --", "--- --"];
/// The same, tied by a `HLD`.
const HELD: [&str; 4] = ["C-3 7F HLD1", "C-4 7F", "--- --", "--- --"];
/// The first let go by an `OFF`, and the second after silence.
const AFTER_SILENCE: [&str; 5] = ["C-3 7F", "OFF --", "--- --", "C-4 7F", "--- --"];

fn pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    let ramp: Vec<[f32; 2]> = (0..RAMP_FRAMES)
        .map(|at| {
            let level = at as f32 / RAMP_FRAMES as f32;
            [level, level]
        })
        .collect();
    let fall = ramp.iter().rev().copied().collect();
    pool.insert("ramp", SampleData { frames: ramp });
    pool.insert("fall", SampleData { frames: fall });
    Arc::new(pool)
}

/// One track on this machine playing these rows in this glide mode, gliding for 100 ms. Notes
/// start and stop at once, so only the pitch moves.
fn song(machine: Machine, mode: GlideMode, rows: &[&str]) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.sample_on(0, "ramp");
    let track = &mut song.tracks[0];
    track.machine = machine;
    track.glide_mode = mode;
    track.glide = glide_value(100.0);
    track.mods[0].adsr.attack_ms = 0;
    track.mods[0].adsr.release_ms = 0;
    track.gate_length = Some(FULL_GATE);
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

/// How far above `C-3` the ramp plays around a frame, in semitones, by how fast it climbs.
fn semitones_at(out: &[f32], at: usize) -> f64 {
    let (from, to) = (at - 12, at + 12);
    let climb = f64::from(out[to] - out[from]) / (to - from) as f64;
    12.0 * (climb * RAMP_FRAMES as f64).log2()
}

/// The frequency a synth plays between two frames, by how far apart it rises through zero.
fn hertz_between(out: &[f32], from: usize, to: usize) -> f64 {
    let crossings: Vec<f64> = (from..to)
        .filter(|at| out[*at] < 0.0 && out[at + 1] >= 0.0)
        .map(|at| at as f64 + f64::from(-out[at] / (out[at + 1] - out[at])))
        .collect();
    assert!(crossings.len() > 2, "too few crossings to hear a pitch");
    let (first, last) = (crossings[0], crossings[crossings.len() - 1]);
    RATE as f64 * (crossings.len() - 1) as f64 / (last - first)
}

#[test]
fn ties_is_what_a_track_played_before_it_could_choose() {
    // A song that doesn't set it plays `Ties`, and with `Glide` off `Always` has nothing to glide
    // by, so both are the engine as it was, bit for bit.
    assert_eq!(GlideMode::default(), GlideMode::Ties);
    for machine in MACHINES {
        let ties = render(&song(machine, GlideMode::Ties, &STRUCK));
        let mut unset = song(machine, GlideMode::Ties, &STRUCK);
        unset.tracks[0].glide_mode = GlideMode::default();
        assert_eq!(render(&unset), ties, "{machine:?}");
        let off = |mode| {
            let mut off = song(machine, mode, &STRUCK);
            off.tracks[0].glide = 0;
            render(&off)
        };
        assert_eq!(off(GlideMode::Always), off(GlideMode::Ties), "{machine:?}");
    }
}

#[test]
fn always_glides_a_struck_sample() {
    let ties = render(&song(Machine::Sampler, GlideMode::Ties, &STRUCK));
    let always = render(&song(Machine::Sampler, GlideMode::Always, &STRUCK));
    // `Ties` strikes the note at its pitch; `Always` strikes it at the last note's and is halfway
    // in pitch at the time constant's share, then lands on it.
    let halfway = STEP + (100.0 * HALFWAY * MS as f64) as usize;
    assert!((semitones_at(&ties, halfway) - 12.0).abs() < 0.01);
    let at_halfway = semitones_at(&always, halfway);
    assert!((at_halfway - 6.0).abs() < 0.3, "{at_halfway}");
    let landed = semitones_at(&always, STEP + 300 * MS);
    assert!((landed - 12.0).abs() < 0.01, "{landed}");
}

#[test]
fn always_glides_a_struck_synth_note_carried_or_not() {
    // `VA` carries its voice, with `Reset`'s dip; `String` starts a voice of its own under the
    // last one's fade. Either glides its pitch as it strikes.
    for voice in [SynthVoice::VirtualAnalog, SynthVoice::String] {
        let machine = Machine::Plaits(voice);
        let ties = render(&song(machine, GlideMode::Ties, &STRUCK));
        let always = render(&song(machine, GlideMode::Always, &STRUCK));
        let early = |out: &[f32]| hertz_between(out, STEP + 5 * MS, STEP + 20 * MS);
        // A plucked string rises through zero by its partials too, so it's heard against the
        // same note struck at `Ties`.
        assert!(
            early(&always) > LOW * 1.05 && early(&always) < early(&ties) * 0.85,
            "{voice:?}: {} Hz gliding, {} Hz struck",
            early(&always),
            early(&ties)
        );
    }
    let landed = render(&song(
        Machine::Plaits(SynthVoice::VirtualAnalog),
        GlideMode::Always,
        &STRUCK,
    ));
    let late = hertz_between(&landed, STEP + 200 * MS, STEP + 240 * MS);
    assert!((late / HIGH - 1.0).abs() < 0.01, "{late} Hz");
}

#[test]
fn no_glide_after_silence() {
    // Let go and rung out, the first note leaves nothing sounding, so the next starts at its own
    // pitch, as the first note of all does.
    let sampled = render(&song(Machine::Sampler, GlideMode::Always, &AFTER_SILENCE));
    assert!(semitones_at(&sampled, 12).abs() < 0.01, "the first note");
    let after = semitones_at(&sampled, 3 * STEP + 300);
    assert!((after - 12.0).abs() < 0.01, "{after}");
    for machine in MACHINES {
        assert_eq!(
            render(&song(machine, GlideMode::Always, &AFTER_SILENCE)),
            render(&song(machine, GlideMode::Ties, &AFTER_SILENCE)),
            "{machine:?}"
        );
    }
}

#[test]
fn no_glide_onto_another_sample() {
    // A row whose lock plays another entry has nothing of its own sounding to glide from.
    let rows = ["C-3 7F", "C-4 7F sampler.sample:02", "--- --", "--- --"];
    let two = |mode| {
        let mut two = song(Machine::Sampler, mode, &rows);
        assert_eq!(two.sample_on(0, "fall"), Some(2));
        two.tracks[0].set_setting(SettingId::SAMPLE, 1);
        render(&two)
    };
    assert_eq!(two(GlideMode::Always), two(GlideMode::Ties));
}

#[test]
fn ties_glide_as_before_in_either_mode() {
    // A `HLD` glides at `Ties` already, and at `Always` the same, bit for bit; so does a `Legato`
    // tie, its gate still open at the full `Gate`.
    for machine in MACHINES {
        assert_eq!(
            render(&song(machine, GlideMode::Always, &HELD)),
            render(&song(machine, GlideMode::Ties, &HELD)),
            "{machine:?} HLD"
        );
        let legato = |mode| {
            let mut legato = song(machine, mode, &STRUCK);
            legato.tracks[0].voice_mode = VoiceMode::Legato;
            render(&legato)
        };
        assert_eq!(
            legato(GlideMode::Always),
            legato(GlideMode::Ties),
            "{machine:?} Legato"
        );
    }
}

#[test]
fn in_legato_the_notes_that_strike_glide_too() {
    // A short `Gate` lets the first note go before the second comes, so in `Legato` it strikes:
    // at `Ties` at its own pitch, at `Always` gliding there from the note still releasing.
    let short = |mode| {
        let mut short = song(Machine::Sampler, mode, &STRUCK);
        short.tracks[0].voice_mode = VoiceMode::Legato;
        short.tracks[0].gate_length = Some(0x80);
        short.tracks[0].mods[0].adsr.release_ms = 200;
        render(&short)
    };
    let halfway = STEP + (100.0 * HALFWAY * MS as f64) as usize;
    assert!((semitones_at(&short(GlideMode::Ties), halfway) - 12.0).abs() < 0.01);
    let at_halfway = semitones_at(&short(GlideMode::Always), halfway);
    assert!((at_halfway - 6.0).abs() < 0.3, "{at_halfway}");
}

#[test]
fn a_step_can_lock_it_for_its_note() {
    // `GMOD01` on a `Ties` track's row glides that row's struck note, as `Always` would.
    let locked = ["C-3 7F", "C-4 7F glidemode:01", "--- --", "--- --"];
    let locked = render(&song(Machine::Sampler, GlideMode::Ties, &locked));
    assert_eq!(
        locked,
        render(&song(Machine::Sampler, GlideMode::Always, &STRUCK))
    );
}

#[test]
fn acid_slides_by_its_own_rules_alone() {
    // A 303 slides only where a note is tied, so `Always` leaves `Acid` as it was.
    let acid = Machine::Plaits(SynthVoice::Acid);
    for rows in [&STRUCK, &HELD] {
        assert_eq!(
            render(&song(acid, GlideMode::Always, rows)),
            render(&song(acid, GlideMode::Ties, rows))
        );
    }
}
