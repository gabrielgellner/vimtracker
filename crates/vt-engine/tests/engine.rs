use std::sync::Arc;

use vt_engine::engine::{Engine, Transport, sample_of_tick};
use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_frames, render_loop};
use vt_model::constants::TICKS_PER_STEP;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Chain, ChainEntry, Instrument, Phrase, Song};
use vt_model::step::{Note, Step};

const SECOND: usize = 48_000;

/// A flat sample, so output shows exactly when a voice is sounding.
fn flat(frames: usize, level: f32) -> SampleData {
    SampleData {
        frames: vec![[level, level]; frames],
    }
}

fn pool(entries: &[(&str, SampleData)]) -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    for (name, data) in entries {
        pool.insert(name, data.clone());
    }
    Arc::new(pool)
}

fn instrument(sample: &str) -> Arc<Instrument> {
    Arc::new(Instrument {
        sample: Some(sample.to_owned()),
        attack_ms: 0,
        release_ms: 0,
        ..Instrument::default()
    })
}

/// A song with one sample, and notes at these steps on the first track.
fn song_with_notes(tracks: usize, steps: &[usize]) -> Song {
    let mut song = Song::new(tracks);
    song.instruments.insert(0, instrument("flat"));
    let phrase = Arc::make_mut(song.phrases.get_mut(&0).unwrap());
    for step in steps {
        *phrase.step_mut(*step).unwrap() = Step {
            note: Note::On(60),
            instrument: Some(0),
            ..Step::default()
        };
    }
    song
}

fn version(song: Song, samples: Arc<SamplePool>) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song), samples))
}

fn first_sound(out: &[f32], from_frame: usize) -> Option<usize> {
    out.as_chunks::<2>()
        .0
        .iter()
        .skip(from_frame)
        .position(|f| f[0] != 0.0)
        .map(|p| p + from_frame)
}

fn fnv(out: &[f32]) -> u64 {
    out.iter()
        .flat_map(|s| s.to_le_bytes())
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

#[test]
fn sample_positions_are_exact_and_never_accumulate() {
    // 5512.5 samples per step at 44100 Hz and 120 BPM.
    assert_eq!(sample_of_tick(TICKS_PER_STEP, 44_100, 120_000), 5512);
    assert_eq!(sample_of_tick(TICKS_PER_STEP * 2, 44_100, 120_000), 11_025);
    assert_eq!(sample_of_tick(TICKS_PER_STEP, 48_000, 120_000), 6000);
    // 137.63 BPM: 60 / 137.63 / 4 x 48000 = 5231.4...
    assert_eq!(sample_of_tick(TICKS_PER_STEP, 48_000, 137_630), 5231);
    assert_eq!(
        sample_of_tick(TICKS_PER_STEP * 1000, 48_000, 137_630),
        5_231_417
    );
}

#[test]
fn a_note_starts_on_its_exact_sample() {
    for (rate, millibpm, expected) in [
        (44_100, 120_000, 5512),
        (48_000, 120_000, 6000),
        (48_000, 137_630, 5231),
    ] {
        let mut song = song_with_notes(1, &[1]);
        song.millibpm = millibpm;
        let samples = pool(&[("flat", flat(rate as usize, 1.0))]);
        let out = render_frames(version(song, samples), rate, rate as usize);
        assert_eq!(first_sound(&out, 0), Some(expected), "{rate} {millibpm}");
    }
}

#[test]
fn blocks_dont_change_the_result() {
    let song = song_with_notes(1, &[0, 3, 7]);
    let samples = pool(&[("flat", flat(SECOND, 0.5))]);
    let v = version(song, samples);
    let whole = render_frames(Arc::clone(&v), 48_000, SECOND);
    let mut engine = Engine::new(48_000, v);
    engine.play_from(0);
    let mut pieces = vec![0.0; SECOND * 2];
    for chunk in pieces.chunks_mut(2 * 137) {
        engine.process(chunk);
    }
    assert_eq!(fnv(&whole), fnv(&pieces));
}

#[test]
fn two_renders_match() {
    let samples = pool(&[("flat", flat(SECOND, 0.5))]);
    let a = render_frames(
        version(song_with_notes(1, &[0, 4]), Arc::clone(&samples)),
        48_000,
        SECOND,
    );
    let b = render_frames(
        version(song_with_notes(1, &[0, 4]), samples),
        48_000,
        SECOND,
    );
    assert_eq!(fnv(&a), fnv(&b));
    assert_ne!(fnv(&a), fnv(&vec![0.0; SECOND * 2]));
}

#[test]
fn a_new_note_fades_the_old_one() {
    let mut song = song_with_notes(1, &[0]);
    song.instruments.insert(1, instrument("quiet"));
    let phrase = Arc::make_mut(song.phrases.get_mut(&0).unwrap());
    *phrase.step_mut(1).unwrap() = Step {
        note: Note::On(60),
        instrument: Some(1),
        ..Step::default()
    };
    let samples = pool(&[("flat", flat(SECOND, 1.0)), ("quiet", flat(SECOND, 0.2))]);
    let out = render_frames(version(song, samples), 48_000, 7000);
    let left: Vec<f32> = out.as_chunks::<2>().0.iter().map(|f| f[0]).collect();
    // The change at step 1 ramps over several frames rather than jumping all at once.
    let biggest = left[5990..6100]
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0, f32::max);
    let drop = left[5999] - left[6100];
    assert!(drop > 0.1, "level should fall, fell {drop}");
    assert!(biggest < drop / 3.0, "jumped {biggest} of {drop}");
}

/// Track 0 has 16 steps, with no notes. Track 1 has 7, with a note on its first.
fn sixteen_and_seven() -> Song {
    let mut song = song_with_notes(2, &[]);
    let seven = Phrase::from_steps(
        (0..7)
            .map(|s| {
                if s == 0 {
                    Step {
                        note: Note::On(60),
                        instrument: Some(0),
                        ..Step::default()
                    }
                } else {
                    Step::default()
                }
            })
            .collect(),
    );
    song.phrases.insert(1, Arc::new(seven));
    assert_eq!(song.section_steps(&song.sections[0]), 16);
    song
}

/// Which of the first steps a very short sample sounds on, 6000 frames apart.
fn fired_steps(out: &[f32], steps: usize) -> Vec<usize> {
    (0..steps).filter(|s| out[s * 6000 * 2] != 0.0).collect()
}

#[test]
fn tracks_loop_at_their_own_lengths_past_the_longest() {
    let samples = pool(&[("flat", flat(10, 1.0))]);
    let out = render_frames(version(sixteen_and_seven(), samples), 48_000, 6000 * 30);
    assert_eq!(fired_steps(&out, 30), [0, 7, 14, 21, 28]);
}

#[test]
fn a_reset_sends_every_track_back_to_its_start() {
    let mut song = sixteen_and_seven();
    song.sections[0].reset = Some(16);
    let samples = pool(&[("flat", flat(10, 1.0))]);
    let out = render_frames(version(song, samples), 48_000, 6000 * 24);
    assert_eq!(fired_steps(&out, 24), [0, 7, 14, 16, 23]);
}

#[test]
fn a_reset_can_be_shorter_than_a_track() {
    let mut song = sixteen_and_seven();
    song.sections[0].reset = Some(4);
    let samples = pool(&[("flat", flat(10, 1.0))]);
    let out = render_frames(version(song, samples), 48_000, 6000 * 10);
    assert_eq!(fired_steps(&out, 10), [0, 4, 8]);
}

#[test]
fn changing_the_reset_live_keeps_the_beat_and_turning_it_off_keeps_the_place() {
    let samples = pool(&[("flat", flat(10, 1.0))]);
    let mut song = sixteen_and_seven();
    let free = version(song.clone(), Arc::clone(&samples));
    song.sections[0].reset = Some(4);
    let reset = version(song, samples);
    let mut engine = Engine::new(48_000, free);
    engine.play_from(0);
    let mut out = vec![0.0; 6000 * 10 * 2];
    engine.process(&mut out);
    assert_eq!(engine.position(), 9);

    // On with the tracks past it: they go back on the next step, and that step is on time.
    engine.swap(Arc::clone(&reset));
    assert_eq!(engine.position(), 0);
    let mut out = vec![0.0; 6000 * 3 * 2];
    engine.process(&mut out);
    assert_ne!(out[0], 0.0, "the note fires exactly where step 10 was due");
    assert_eq!(engine.position(), 2);

    // Off: the tracks carry on from where they are.
    let mut off = (*reset.song).clone();
    off.sections[0].reset = None;
    engine.swap(version(off, Arc::clone(&reset.samples)));
    assert_eq!(engine.position(), 2);
}

#[test]
fn swing_stays_on_the_grid_across_an_odd_length() {
    let mut song = song_with_notes(1, &[0]);
    Arc::make_mut(song.phrases.get_mut(&0).unwrap()).set_len(3);
    song.swing = 750;
    let samples = pool(&[("flat", flat(10, 1.0))]);
    let out = render_frames(version(song, samples), 48_000, 6000 * 8);
    // Step 3 is an odd step of the song, though it's the first of the track's second pass.
    assert_eq!(out[6000 * 3 * 2], 0.0);
    assert_ne!(out[(6000 * 3 + 3000) * 2], 0.0);
    assert_ne!(out[6000 * 6 * 2], 0.0);
}

#[test]
fn a_render_captures_every_track_from_its_start() {
    let samples = pool(&[("flat", flat(10, 1.0))]);
    let options = RenderOptions {
        rows: Some(24),
        tail: false,
    };
    let out = render_loop(version(sixteen_and_seven(), samples), 48_000, options);
    assert_eq!(fired_steps(&out, 24), [0, 7, 14, 21]);
}

#[test]
fn a_render_is_as_long_as_the_reset() {
    let mut song = sixteen_and_seven();
    song.sections[0].reset = Some(8);
    let samples = pool(&[("flat", flat(10, 1.0))]);
    let out = render_loop(version(song, samples), 48_000, RenderOptions::default());
    assert_eq!(out.len(), 6000 * 8 * 2);
    assert_eq!(fired_steps(&out, 8), [0, 7]);
}

#[test]
fn chain_transpose_and_step_volume_apply() {
    let mut song = song_with_notes(1, &[0]);
    song.chains.insert(
        0,
        Arc::new(Chain {
            entries: vec![ChainEntry {
                phrase: 0,
                transpose: 12,
            }],
        }),
    );
    let phrase = Arc::make_mut(song.phrases.get_mut(&0).unwrap());
    phrase.step_mut(0).unwrap().velocity = Some(0x40);
    let samples = pool(&[("flat", flat(1000, 1.0))]);
    let out = render_frames(version(song, samples), 48_000, 2000);
    // An octave up plays 1000 frames in about 500.
    let last = out
        .as_chunks::<2>()
        .0
        .iter()
        .rposition(|f| f[0] != 0.0)
        .unwrap();
    assert!((495..=510).contains(&last), "ended at {last}");
    let expected = 64.0_f32 / 127.0;
    assert!((out[0] - expected).abs() < 1e-3, "got {}", out[0]);
}

#[test]
fn replacing_a_sample_doesnt_move_a_playing_voice() {
    let song = song_with_notes(1, &[0]);
    let samples = pool(&[("flat", flat(SECOND, 1.0)), ("other", flat(10, 0.1))]);
    let mut engine = Engine::new(48_000, version(song.clone(), Arc::clone(&samples)));
    engine.play_from(0);
    let mut first = vec![0.0; 2000];
    engine.process(&mut first);
    let mut changed = song;
    changed.instruments.insert(0, instrument("other"));
    engine.swap(version(changed, samples));
    let mut second = vec![0.0; 2000];
    engine.process(&mut second);
    assert_eq!(second[1998], first[0], "the voice keeps its own sample");
}

#[test]
fn reads_stop_at_a_samples_end() {
    let samples = pool(&[("flat", flat(100, 1.0))]);
    let out = render_frames(version(song_with_notes(1, &[0]), samples), 48_000, 1000);
    assert_ne!(out[2 * 99], 0.0);
    assert_eq!(out[2 * 100], 0.0);
}

#[test]
fn a_tempo_change_keeps_the_next_step_in_time() {
    let song = song_with_notes(1, &[]);
    let samples = pool(&[("flat", flat(SECOND, 1.0))]);
    let mut engine = Engine::new(48_000, version(song, Arc::clone(&samples)));
    engine.play_from(0);
    let mut out = vec![0.0; 2 * 3000];
    engine.process(&mut out);
    let mut faster = song_with_notes(1, &[1]);
    faster.millibpm = 240_000;
    engine.swap(version(faster, samples));
    // Half way through step 0 at the old tempo, half a step remains: 1500 frames at 240 BPM.
    let mut rest = vec![0.0; 2 * 3000];
    engine.process(&mut rest);
    assert_eq!(first_sound(&rest, 0), Some(1500));
}

#[test]
fn only_the_loop_plays() {
    let mut song = song_with_notes(1, &[8]);
    let phrase = Arc::make_mut(song.phrases.get_mut(&0).unwrap());
    phrase.set_len(4);
    let samples = pool(&[("flat", flat(SECOND, 1.0))]);
    let silent = render_frames(version(song.clone(), Arc::clone(&samples)), 48_000, SECOND);
    assert_eq!(first_sound(&silent, 0), None, "the note is past the loop");
    let phrase = Arc::make_mut(song.phrases.get_mut(&0).unwrap());
    phrase.slide(8);
    let out = render_frames(version(song, samples), 48_000, SECOND);
    assert_eq!(first_sound(&out, 0), Some(0), "its step is now beat 1");
}

#[test]
fn a_live_slide_keeps_the_beat() {
    let mut song = song_with_notes(1, &[10]);
    Arc::make_mut(song.phrases.get_mut(&0).unwrap()).set_len(4);
    let samples = pool(&[("flat", flat(SECOND, 1.0))]);
    let mut engine = Engine::new(48_000, version(song.clone(), Arc::clone(&samples)));
    engine.play_from(0);
    // Half way through the first step at 6000 frames a step.
    let mut out = vec![0.0; 2 * 3000];
    engine.process(&mut out);
    assert_eq!(first_sound(&out, 0), None);
    Arc::make_mut(song.phrases.get_mut(&0).unwrap()).slide(8);
    engine.swap(version(song, samples));
    let mut rest = vec![0.0; 2 * 12_000];
    engine.process(&mut rest);
    // The note is now on row 02, which plays at frame 12000, the same place in the bar as ever.
    assert_eq!(first_sound(&rest, 0), Some(9000));
}

#[test]
fn transport_states() {
    let samples = pool(&[("flat", flat(SECOND, 1.0))]);
    let mut engine = Engine::new(48_000, version(song_with_notes(1, &[0]), samples));
    assert_eq!(engine.transport(), Transport::Stopped);
    engine.pause_or_resume();
    assert_eq!(engine.transport(), Transport::Playing);
    engine.pause_or_resume();
    assert_eq!(engine.transport(), Transport::Paused);
    engine.pause_or_resume();
    assert_eq!(engine.transport(), Transport::Playing);
    engine.play_from(3);
    assert_eq!(engine.transport(), Transport::Playing);
    assert_eq!(engine.position(), 3);
    engine.play_from(5);
    assert_eq!(engine.transport(), Transport::Playing);
    assert_eq!(engine.position(), 5, "playing again restarts from there");
    engine.pause_or_resume();
    engine.play_from(0);
    assert_eq!(engine.transport(), Transport::Playing);
    engine.return_to_start();
    assert_eq!(engine.transport(), Transport::Stopped);
    assert_eq!(engine.position(), 0);
}

#[test]
fn stopping_fades_and_doesnt_click() {
    let samples = pool(&[("flat", flat(SECOND, 1.0))]);
    let mut engine = Engine::new(48_000, version(song_with_notes(1, &[0]), samples));
    engine.play_from(0);
    let mut before = vec![0.0; 2000];
    engine.process(&mut before);
    engine.return_to_start();
    let mut after = vec![0.0; 2000];
    engine.process(&mut after);
    let full = before[1998];
    assert!(
        after[0] > full * 0.9,
        "should start near full: {}",
        after[0]
    );
    assert_eq!(after[1998], 0.0);
}

#[test]
fn a_preview_plays_without_touching_sequenced_notes() {
    let song = song_with_notes(1, &[0]);
    let samples = pool(&[("flat", flat(SECOND, 0.25))]);
    let mut plain = Engine::new(48_000, version(song.clone(), Arc::clone(&samples)));
    plain.play_from(0);
    let mut alone = vec![0.0; 2000];
    plain.process(&mut alone);
    let mut engine = Engine::new(48_000, version(song, samples));
    engine.play_from(0);
    engine.preview(0, 60, 0xFF);
    let mut both = vec![0.0; 2000];
    engine.process(&mut both);
    assert!(both[1000] > alone[1000], "the preview should add level");
}
