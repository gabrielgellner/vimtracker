//! The track filter's `Drive`, `Punch`, `Low cut`, `High cut`, `Type` and `Safe` as registry
//! settings (decision 398): a step locks each for its row as it locks `CUT`, a macro and an LFO
//! move the four on a scale, and a lock of the type fades from the filter it leaves rather than
//! cutting to one starting from silence. Unmoved, each is the track's own field, heard as before.

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::lfo::{Lfo, Rate};
use vt_model::macros::{MacroMap, MacroTarget};
use vt_model::modulation::ModSlot;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{FULL_GATE, FilterType, Machine, Phrase, Song, SynthVoice, Track};

use crate::support::rms;

const RATE: u32 = 48_000;
/// Frames in a step at 120 BPM.
const STEP: usize = 6000;

/// One `CSaw` track through a low-pass at `Cut 90`, `Res 60`, playing these rows.
fn song(rows: &[&str]) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    let track = &mut song.tracks[0];
    track.machine = Machine::Plaits(SynthVoice::CSaw);
    track.cutoff = 0x90;
    track.resonance = 0x60;
    track.gate_length = Some(FULL_GATE);
    let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
    track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    song
}

/// The same song with its track changed.
fn with(rows: &[&str], change: impl Fn(&mut Track)) -> Song {
    let mut song = song(rows);
    change(&mut song.tracks[0]);
    song
}

/// The left channel, for this many steps.
fn render(song: &Song, steps: usize) -> Vec<f32> {
    let version = Arc::new(Published::new(
        Arc::new(song.clone()),
        Arc::new(SamplePool::default()),
    ));
    render_frames(version, RATE, steps * STEP)
        .chunks(2)
        .map(|frame| frame[0])
        .collect()
}

/// How far apart two renders are over a step's second half, once a change has settled: the
/// level of their difference against the level of the second, in dB.
fn apart_db(one: &[f32], other: &[f32], step: usize) -> f32 {
    let range = step * STEP + STEP / 2..(step + 1) * STEP;
    let difference: Vec<f32> = range.clone().map(|at| one[at] - other[at]).collect();
    20.0 * (rms(&difference) / rms(&other[range])).log10()
}

/// A change made to the track.
type Change = fn(&mut Track);

/// Each setting, its key on a step, what's set on the track to hear it and the value a lock
/// sets, with the track as each is heard on: `Safe` on a ringing ladder, `Punch` on a ringing
/// clean filter.
fn settings() -> Vec<(&'static str, Change, Change, &'static str)> {
    vec![
        ("drive", |_| {}, |track| track.drive = 0xC0, "C0"),
        (
            "punch",
            |track| track.resonance = 0xE0,
            |track| track.punch = 0xFF,
            "FF",
        ),
        ("lowcut", |_| {}, |track| track.low_cut = 0xA0, "A0"),
        ("highcut", |_| {}, |track| track.high_cut = 0x50, "50"),
        (
            "type",
            |_| {},
            |track| track.filter = FilterType::HighPass,
            "01",
        ),
        (
            "safe",
            |track| {
                track.filter = FilterType::Ladder;
                track.resonance = 0xFF;
            },
            |track| track.safe = true,
            "01",
        ),
    ]
}

#[test]
fn a_lock_at_the_tracks_own_values_changes_nothing() {
    // Every one of them set on the track and locked to the same on every row is the track as
    // it is, to the sample, as a lock only sets what's heard.
    let own = |track: &mut Track| {
        track.drive = 0x60;
        track.punch = 0x40;
        track.low_cut = 0x30;
        track.high_cut = 0xC0;
        track.filter = FilterType::Ladder;
        track.safe = true;
    };
    let row = "C-3 64 drive:60 punch:40 lowcut:30";
    let locked = with(&[row, "C-4 64 highcut:C0 type:04 safe:01"], own);
    let plain = with(&["C-3 64", "C-4 64"], own);
    assert_eq!(render(&locked, 2), render(&plain, 2));
}

#[test]
fn a_step_locks_each_for_its_row() {
    for (key, base, set, value) in settings() {
        let lock = format!("C-4 64 {key}:{value}");
        let on = ["C-3 64", lock.as_str(), "C-3 64", "--- --"];
        let off = ["C-3 64", "C-4 64", "C-3 64", "--- --"];
        let locked = render(&with(&on, base), 4);
        let unlocked = render(&with(&off, base), 4);
        let set_throughout = render(
            &with(&off, |track| {
                base(track);
                set(track);
            }),
            4,
        );
        // Before its row, nothing's moved.
        assert_eq!(locked[..STEP], unlocked[..STEP], "{key}");
        // On its row it's heard as the track set to it is, and far from the track without it.
        let to_set = apart_db(&locked, &set_throughout, 1);
        let to_unset = apart_db(&locked, &unlocked, 1);
        println!("{key}: {to_set:.1} dB from it set, {to_unset:.1} dB from it unset");
        assert!(
            to_set < -30.0,
            "{key}: {to_set} dB from the track set to it"
        );
        assert!(
            to_unset > -20.0,
            "{key}: {to_unset} dB from the track as it was"
        );
        // And the next note is back to the track's own, gliding there as from a lock of `CUT`,
        // until by the row after it's the track as it was to the sample. A ladder ringing by
        // itself at the top of `Res` rings on from where `Safe` left it, at its own level but not
        // in step with the track that never had it.
        if key == "safe" {
            let level = |out: &[f32]| 20.0 * rms(&out[3 * STEP..]).log10();
            let after = level(&locked) - level(&unlocked);
            assert!(after.abs() < 0.5, "{key}: {after} dB after its row");
            continue;
        }
        let after = apart_db(&locked, &unlocked, 2);
        println!("{key}: {after:.1} dB from it unset on the next row");
        assert!(after < -15.0, "{key}: {after} dB after its row");
        assert_eq!(locked[3 * STEP..], unlocked[3 * STEP..], "{key}");
    }
}

fn map(target: SettingId) -> MacroMap {
    MacroMap {
        target: MacroTarget::Setting(target),
        min: 0,
        max: 0xFF00,
        curve: 0,
        s: 0,
        zone: Default::default(),
    }
}

#[test]
fn a_macro_turns_drive_and_low_cut() {
    // Macro 1 holding both over their whole range, turned to `C0`, is the track with both set
    // there.
    let rows = ["C-3 64", "C-4 64"];
    let turned = with(&rows, |track| {
        track.macros[0].maps = vec![map(SettingId::DRIVE), map(SettingId::LOW_CUT)];
        track.macros[0].value = 0xC000;
    });
    let set = with(&rows, |track| {
        track.drive = 0xC0;
        track.low_cut = 0xC0;
    });
    let plain = render(&song(&rows), 2);
    let (turned, set) = (render(&turned, 2), render(&set, 2));
    assert_eq!(turned, set);
    assert!(apart_db(&turned, &plain, 1) > -6.0);
}

/// How far apart two renders are over each quarter of a step, in dB as [`apart_db`] has it,
/// from the second step, once the note's attack is over.
fn apart_by_quarter(one: &[f32], other: &[f32], steps: usize) -> Vec<f32> {
    (4..steps * 4)
        .map(|quarter| {
            let range = quarter * STEP / 4..(quarter + 1) * STEP / 4;
            let difference: Vec<f32> = range.clone().map(|at| one[at] - other[at]).collect();
            20.0 * (rms(&difference) / rms(&other[range])).log10()
        })
        .collect()
}

#[test]
fn an_lfo_moves_drive_and_low_cut() {
    // One long note with a one-hertz sine from an LFO on `Low cut` or `Drive`, each held at `80`:
    // against the note with the setting still, it's far apart where the wave is out at either
    // end and close where it comes back through the middle, twice a second, so it moves the
    // setting up and down rather than setting it somewhere else.
    let rows = [
        "C-3 64", "--- --", "--- --", "--- --", "--- --", "--- --", "--- --", "--- --",
    ];
    let lfo = Lfo {
        rate: Rate::Hertz(100),
        ..Lfo::default()
    };
    let pairs: [(SettingId, Change); 2] = [
        (SettingId::LOW_CUT, |track: &mut Track| track.low_cut = 0x80),
        (SettingId::DRIVE, |track: &mut Track| track.drive = 0x80),
    ];
    for (destination, held) in pairs {
        let still = render(&with(&rows, held), 8);
        let moved = with(&rows, |track| {
            held(track);
            track.mods[3] = ModSlot::lfo(Some(destination), lfo).with_depth(0x60);
        });
        let apart = apart_by_quarter(&render(&moved, 8), &still, 8);
        let (closest, furthest) = apart.iter().fold((f32::MAX, f32::MIN), |(low, high), db| {
            (low.min(*db), high.max(*db))
        });
        println!(
            "{}: {closest:.1} to {furthest:.1} dB from it still",
            destination.name()
        );
        assert!(furthest > -15.0, "{}: {furthest}", destination.name());
        assert!(
            furthest - closest > 6.0,
            "{}: {apart:?}",
            destination.name()
        );
    }
}

/// The sampled sine's period: 240 Hz at `C-3`.
const PERIOD: usize = 200;

/// A sampler playing a sine through this filter type, at `Cut 90` and `Res 60`, as long as the
/// song, whose own second difference is next to nothing, so a step in the filter's output
/// stands out.
fn sine(rows: &[&str], kind: FilterType) -> Vec<f32> {
    let mut song = song(rows);
    song.sample_on(0, "s");
    song.tracks[0].filter = kind;
    let frames = (0..RATE as usize * 4)
        .map(|at| {
            let s = 0.5 * (std::f32::consts::TAU * (at % PERIOD) as f32 / PERIOD as f32).sin();
            [s, s]
        })
        .collect();
    let mut pool = SamplePool::default();
    pool.insert("s", SampleData { frames });
    let version = Arc::new(Published::new(Arc::new(song), Arc::new(pool)));
    render_frames(version, RATE, rows.len() * STEP)
        .chunks(2)
        .map(|frame| frame[0])
        .collect()
}

/// The largest second difference over these frames: what a click adds most of, as a step in
/// level or slope is a spike in it where a sine's own is small and steady.
fn sharpest(out: &[f32], from: usize, to: usize) -> f32 {
    (from.max(2)..to)
        .map(|at| (out[at] - 2.0 * out[at - 1] + out[at - 2]).abs())
        .fold(0.0, f32::max)
}

#[test]
fn a_lock_of_the_type_fades_from_the_filter_it_leaves_without_a_click() {
    // A held sine through one type, then a lock of another on a row without a note: the 10 ms
    // after it are no sharper than the sharper of the two types playing it steadily, within the
    // 3 dB `note_clicks.rs` allows a note change against a tie. The new type starts from
    // silence, so a cut to it would step by the whole of what the old one was giving.
    let pairs = [
        (FilterType::LowPass, "01", FilterType::HighPass),
        (FilterType::HighPass, "00", FilterType::LowPass),
        (FilterType::LowPass, "04", FilterType::Ladder),
        (FilterType::Ladder, "09", FilterType::ObX),
        (FilterType::Diode, "03", FilterType::Notch),
        (FilterType::Notch, "07", FilterType::Ms20HighPass),
    ];
    let plain = ["C-3 64", "--- --", "--- --", "--- --"];
    let window = (2 * STEP, 2 * STEP + RATE as usize / 100);
    for (from, value, to) in pairs {
        let lock = format!("--- -- type:{value}");
        let switched = sine(&["C-3 64", "--- --", lock.as_str(), "--- --"], from);
        let change = sharpest(&switched, window.0, window.1);
        let steady = [from, to]
            .map(|kind| sharpest(&sine(&plain, kind), window.0, window.1))
            .into_iter()
            .fold(0.0, f32::max);
        let db = 20.0 * (change / steady).log10();
        println!(
            "{} to {}: {change:.5} at the change, {steady:.5} steady, {db:+.1} dB",
            from.name(),
            to.name()
        );
        assert!(db < 3.0, "{} to {}: {db:+.1} dB", from.name(), to.name());
        // And the type it locks is what plays once the fade is over.
        let locked = sine(&plain, to);
        assert!(
            apart_db(&switched, &locked, 3) < -30.0,
            "{} to {}",
            from.name(),
            to.name()
        );
    }
}
