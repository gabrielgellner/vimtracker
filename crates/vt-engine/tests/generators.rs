//! Live generators as the engine plays them (decision 288): at rest, what's heard is what's
//! written, sample for sample; a generator whose settings the engine holds elsewhere is rendered
//! again from its `Shape` and heard so, at once, a pass at a time from where the song is.

mod support;

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::gens::{GenHeard, GenReport};
use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_loop};
use vt_model::action::{Action, edit};
use vt_model::generator::Span;
use vt_model::lfo::Shape as Wave;
use vt_model::samples::SamplePool;
use vt_model::selection::StepRange;
use vt_model::settings::SettingId;
use vt_model::shape::{
    Acid, Bits, Context, DEFAULT_RATE, FillKind, Follow, GenParam, Pink, Shape, ShapeColumn,
    Values, Walk, Where,
};
use vt_model::song::{Lane, Machine, Phrase, Song, SynthVoice};
use vt_model::speed::Speed;
use vt_model::step::{FxCommand, Step};

use support::golden;

const RATE: u32 = 48_000;

/// A shape as the panel opens it, with these ends and rows chosen.
fn shape(kind: FillKind, from: u8, to: u8, place: Where) -> Shape {
    Shape {
        from,
        to,
        curve: 0,
        s: 0,
        kind,
        wave: Wave::Sine,
        rate: DEFAULT_RATE,
        phase: 0,
        run: 1,
        values: Values::default(),
        place,
        seed: 7,
        acid: Acid::default(),
        follow: Follow::default(),
        walk: Walk::default(),
        pink: Pink::default(),
        bits: Bits::default(),
        grids: vt_model::shape::Grids::default(),
        random: vt_model::shape::Random::default(),
        vary: 0,
        variation: vt_model::shape::Variation::default(),
    }
}

fn apply(song: Song, action: &Action) -> Song {
    Arc::unwrap_or_clone(edit(&Arc::new(song), action).expect("the edit applies"))
}

/// Keep a shape live on a track's phrase `00`.
fn generate(song: Song, track: usize, column: ShapeColumn, span: Span, shape: Shape) -> Song {
    apply(
        song,
        &Action::Generate {
            track,
            phrase: 0,
            column,
            span,
            shape,
        },
    )
}

fn rows(last: usize) -> Span {
    Span::rows(StepRange::between(0, last))
}

/// A lock of `Cut` in FX column 1, and of `Res` in column 2.
fn cut() -> ShapeColumn {
    ShapeColumn::Fx {
        slot: 0,
        command: FxCommand::Set(SettingId::CUT),
    }
}

fn res() -> ShapeColumn {
    ShapeColumn::Fx {
        slot: 1,
        command: FxCommand::Set(SettingId::RES),
    }
}

/// Two synth tracks with a live generator on every column between them: an `Acid` line and a wave
/// on its `Cut` on the first; on the second, a note on every row with Euclid's velocities, a ramp
/// of nudges, random lengths and a lane of `Res` five steps long.
pub fn moving_song() -> Song {
    let mut song = Song::new(2);
    song.millibpm = 128_000;
    song.seed = 0x5EED;
    let rest: Step = "--- -- --- --- ---".parse().unwrap();
    let hit: Step = "C-3 64 --- --- ---".parse().unwrap();
    song.tracks[0].machine = Machine::Plaits(SynthVoice::Acid);
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(vec![rest; 16])));
    song.tracks[1].machine = Machine::Plaits(SynthVoice::VirtualAnalog);
    let mut hits = Phrase::from_steps(vec![hit; 16]);
    hits.set_lane(
        1,
        Some(Lane {
            start: 0,
            length: 5,
        }),
    );
    song.tracks[1].phrases.insert(0, Arc::new(hits));
    let acid = ShapeColumn::Note {
        scale: song.scale,
        writes: Some(0x64),
    };
    let song = generate(
        song,
        0,
        acid,
        rows(15),
        shape(FillKind::Acid, 36, 48, Where::AllRows),
    );
    let song = generate(
        song,
        0,
        cut(),
        rows(15),
        shape(FillKind::Wave, 0x20, 0xC0, Where::AllRows),
    );
    let euclid = Where::Euclid { hits: 5, rotate: 0 };
    let song = generate(
        song,
        1,
        ShapeColumn::Velocity,
        rows(15),
        shape(FillKind::Ramp, 0x30, 0x7F, euclid),
    );
    let song = generate(
        song,
        1,
        ShapeColumn::Nudge { most: 11 },
        rows(15),
        shape(FillKind::Ramp, 0, 22, Where::AllRows),
    );
    let song = generate(
        song,
        1,
        ShapeColumn::Length,
        rows(15),
        shape(FillKind::Random, 0x20, 0xF0, Where::AllRows),
    );
    generate(
        song,
        1,
        res(),
        Span::Lane,
        shape(FillKind::Random, 0x10, 0x90, Where::AllRows),
    )
}

fn version(song: &Song) -> Arc<Published> {
    Arc::new(Published::new(
        Arc::new(song.clone()),
        Arc::new(SamplePool::default()),
    ))
}

#[test]
fn a_song_whose_generators_rest_renders_as_it_did_before_the_engine_ran_them() {
    let song = moving_song();
    assert_eq!(song.tracks[0].phrases[&0].generators().len(), 2);
    assert_eq!(song.tracks[1].phrases[&0].generators().len(), 4);
    let out = render_loop(version(&song), RATE, RenderOptions::default());
    golden("generators-at-rest", &out);
}

/// A Nudge generator that's only ever late on what's written, so the song's lead as written is 0.
pub fn late_song() -> Song {
    let mut song = Song::new(1);
    let hit: Step = "C-3 64 --- --- ---".parse().unwrap();
    song.tracks[0].machine = Machine::Plaits(SynthVoice::VirtualAnalog);
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(vec![hit; 16])));
    generate(
        song,
        0,
        ShapeColumn::Nudge { most: 11 },
        rows(15),
        shape(FillKind::Ramp, 11, 22, Where::AllRows),
    )
}

#[test]
fn a_nudge_generator_at_rest_renders_as_before_however_early_it_could_go() {
    let out = render_loop(version(&late_song()), RATE, RenderOptions::default());
    golden("generators-late-at-rest", &out);
}

#[test]
fn a_nudge_generator_works_the_engine_ahead_as_far_as_its_tracks_nudges_go() {
    assert_eq!(version(&late_song()).lead, 110, "B 24ths of a sixteenth");
    // On a track at a quarter, a column opened there goes to 2F.
    let at = |speed: Speed, most: i8| {
        let mut song = Song::new(1);
        let hit: Step = "C-3 64 --- --- ---".parse().unwrap();
        song.tracks[0].speed = speed;
        song.tracks[0]
            .phrases
            .insert(0, Arc::new(Phrase::from_steps(vec![hit; 4])));
        let column = ShapeColumn::Nudge { most };
        let shape = shape(FillKind::Ramp, most as u8, 2 * most as u8, Where::AllRows);
        version(&generate(song, 0, column, rows(3), shape)).lead
    };
    assert_eq!(at(Speed::Quarter, 47), 470);
    // A column opened at a quarter on a track since made a 32nd goes no further than a 32nd's.
    assert_eq!(at(Speed::ThirtySecond, 47), 50);
}

/// 5625 frames a step at 128 BPM.
const STEP: usize = 5625;

/// A setting the engine holds a track's generators at.
type Hold = (usize, ShapeColumn, GenParam, i32);

/// Play a song from the top for so many steps, with these held, and what each track's
/// generators were heard as.
fn play(song: &Song, holds: &[Hold], steps: usize) -> (Vec<f32>, Engine) {
    let mut engine = Engine::new(RATE, version(song));
    for (track, column, param, number) in holds {
        engine.hold_generator(*track, *column, *param, Some(*number));
    }
    engine.play_from(0);
    let mut out = vec![0.0; steps * STEP * 2];
    engine.process(&mut out);
    (out, engine)
}

fn heard(engine: &Engine, track: usize) -> Vec<GenHeard> {
    engine.generators_heard(track).collect()
}

/// The setting as the shape has it, and another number for it within its bounds.
fn another(shape: &Shape, column: ShapeColumn, param: GenParam) -> (i32, i32) {
    let context = context(column);
    let (low, high) = param.bounds(&context).unwrap();
    let now = param.number(shape, &context).unwrap();
    let other = low + (now - low + (high - low) / 3 + 1) % (high - low + 1);
    assert_ne!(now, other);
    (now, other)
}

fn context(column: ShapeColumn) -> Context<'static> {
    Context {
        column,
        open: 16,
        generates: false,
        sources: &[],
        source: None,
    }
}

/// The song with one of its generators given a new shape on the edit thread: the setting at this
/// number, written.
fn reshaped(song: &Song, (track, column, param, number): Hold) -> Song {
    let phrase = &song.tracks[track].phrases[&0];
    let generator = phrase
        .generators()
        .iter()
        .position(|generator| generator.column == column)
        .unwrap();
    let mut shape = phrase.generators()[generator].shape;
    param.set_number(&mut shape, &context(column), number);
    apply(
        song.clone(),
        &Action::Reshape {
            track,
            phrase: 0,
            generator,
            shape,
        },
    )
}

/// The generator on a track's column, as written.
fn written(song: &Song, track: usize, column: ShapeColumn) -> Shape {
    let phrase = &song.tracks[track].phrases[&0];
    let found = phrase.generators().iter().find(|g| g.column == column);
    found.unwrap().shape
}

/// A setting held at the shape's own number leaves the generator at rest: nothing renders, and
/// the song plays sample for sample as written.
#[test]
fn a_setting_held_at_its_own_number_leaves_the_generator_at_rest() {
    let song = moving_song();
    let acid = ShapeColumn::Note {
        scale: song.scale,
        writes: Some(0x64),
    };
    let (density, _) = another(&written(&song, 0, acid), acid, GenParam::Density);
    let (written_out, _) = play(&song, &[], 32);
    let (held_out, engine) = play(&song, &[(0, acid, GenParam::Density, density)], 32);
    assert!(
        written_out == held_out,
        "held at its own, it plays as written"
    );
    assert!(
        heard(&engine, 0)
            .iter()
            .all(|run| !run.moving && run.renders == 0)
    );
}

/// Each column's generator, held at another number in the engine, plays sample for sample what
/// the edit thread writes for that number: the same render, from the same code.
#[test]
fn a_held_setting_plays_what_writing_it_would_on_every_column() {
    let song = moving_song();
    let acid = ShapeColumn::Note {
        scale: song.scale,
        writes: Some(0x64),
    };
    let cases = [
        (0, acid, GenParam::Density),
        (0, acid, GenParam::Notes),
        (0, cut(), GenParam::To),
        (1, ShapeColumn::Velocity, GenParam::Hits),
        (1, ShapeColumn::Nudge { most: 11 }, GenParam::From),
        (1, ShapeColumn::Length, GenParam::To),
        (1, ShapeColumn::Length, GenParam::Spread),
        (1, res(), GenParam::From),
        (1, res(), GenParam::Bias),
    ];
    let (at_rest, _) = play(&song, &[], 32);
    for (track, column, param) in cases {
        let (_, other) = another(&written(&song, track, column), column, param);
        let hold = (track, column, param, other);
        let (held, engine) = play(&song, &[hold], 32);
        let (writes, _) = play(&reshaped(&song, hold), &[], 32);
        assert!(
            held != at_rest,
            "{param:?} held at {other} on {column:?} is heard"
        );
        assert!(
            held == writes,
            "{param:?} held at {other} on {column:?} plays as written"
        );
        let moving: Vec<ShapeColumn> = heard(&engine, track)
            .into_iter()
            .filter(|run| run.moving)
            .map(|run| run.column)
            .collect();
        assert_eq!(moving, [column], "only {column:?} moves");
    }
}

/// A run renders when what it's heard with changes, and keeps it while it doesn't: two passes of
/// a held setting are one render, and a new number is one more, heard at once.
#[test]
fn a_moving_generator_renders_once_for_each_change_and_at_once() {
    let song = moving_song();
    let hits = |number| (1, ShapeColumn::Velocity, GenParam::Hits, number);
    let (_, mut engine) = play(&song, &[hits(3)], 24);
    let velocity = |engine: &Engine| {
        heard(engine, 1)
            .into_iter()
            .find(|run| run.column == ShapeColumn::Velocity)
            .unwrap()
    };
    assert_eq!(velocity(&engine).renders, 1);
    assert_eq!(velocity(&engine).pass, 1, "on its second pass");
    // Half way through a pass, a new number is heard from the next row.
    engine.hold_generator(1, ShapeColumn::Velocity, GenParam::Hits, Some(9));
    let mut out = vec![0.0; STEP * 2];
    engine.process(&mut out);
    assert_eq!(velocity(&engine).renders, 2);
    // Given back, it's at rest again, and plays what's written.
    engine.hold_generator(1, ShapeColumn::Velocity, GenParam::Hits, None);
    engine.process(&mut out);
    assert!(heard(&engine, 1).iter().all(|run| !run.moving));
}

/// Two tracks, the first with a 16-row phrase played nine times in its chain, with a live
/// velocity ramp: the second only keeps the section long.
fn nine_passes() -> Song {
    let mut song = Song::new(1);
    song.millibpm = 128_000;
    let hit: Step = "C-3 64 --- --- ---".parse().unwrap();
    song.tracks[0].machine = Machine::Plaits(SynthVoice::VirtualAnalog);
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(vec![hit; 16])));
    let chain = Arc::make_mut(song.tracks[0].chains.get_mut(&0).unwrap());
    let entry = chain.entries[0];
    chain.entries = vec![entry; 9];
    generate(
        song,
        0,
        ShapeColumn::Velocity,
        rows(15),
        shape(FillKind::Ramp, 0x20, 0x7F, Where::AllRows),
    )
}

/// A pass is worked out from where the song is: played from the ninth time through, a generator
/// hears the pass it would have heard playing there from the top.
#[test]
fn a_generators_pass_is_where_the_song_is_not_how_it_got_there() {
    let song = nine_passes();
    let hold = (0, ShapeColumn::Velocity, GenParam::From, 0x40);
    let (_, from_top) = play(&song, &[hold], 8 * 16 + 4);
    let mut engine = Engine::new(RATE, version(&song));
    engine.hold_generator(hold.0, hold.1, hold.2, Some(hold.3));
    engine.play_from(8 * 16);
    let mut out = vec![0.0; 4 * STEP * 2];
    engine.process(&mut out);
    let pass = |engine: &Engine| heard(engine, 0)[0].pass;
    assert_eq!(pass(&from_top), 8);
    assert_eq!(pass(&engine), 8);
}

/// The nine-pass song with its velocities drawn at random, a new seed every `vary` passes.
fn varying(vary: u8) -> Song {
    let song = nine_passes();
    let mut shape = shape(FillKind::Random, 0x20, 0x7F, Where::AllRows);
    shape.vary = vary;
    reshape(&song, shape)
}

fn reshape(song: &Song, shape: Shape) -> Song {
    apply(
        song.clone(),
        &Action::Reshape {
            track: 0,
            phrase: 0,
            generator: 0,
            shape,
        },
    )
}

/// The song with its generator's shape as a pass plays it written in, and `Vary` off.
fn kept(song: &Song, pass: i64) -> Song {
    let mut shape = written(song, 0, ShapeColumn::Velocity).on_pass(pass);
    shape.vary = 0;
    reshape(song, shape)
}

/// Play a song's `pass` of its 16-row phrase alone, from its first row.
fn play_pass(song: &Song, pass: usize) -> (Vec<f32>, Engine) {
    let mut engine = Engine::new(RATE, version(song));
    engine.play_from(pass * 16);
    let mut out = vec![0.0; 16 * STEP * 2];
    engine.process(&mut out);
    (out, engine)
}

/// The passes heard of a track's generators that vary, in the order they were, with the seed
/// each played.
fn passes_heard(engine: &mut Engine, track: usize) -> Vec<(i64, u64)> {
    let mut heard = Vec::new();
    engine.take_gen_reports(|on, report| {
        if let GenReport::Pass(pass) = report
            && on == track
        {
            heard.push((pass.pass, pass.seed));
        }
    });
    heard
}

/// With `Vary` on, each pass plays a seed of its own, the first the one written: pass 0 is what's
/// written, sample for sample with nothing rendered, and every later pass plays, sample for
/// sample, what writing its seed in would.
#[test]
fn a_generator_that_varies_plays_each_pass_as_its_seed_written_would() {
    let song = varying(1);
    let (first, engine) = play_pass(&song, 0);
    assert!(
        first == play_pass(&kept(&song, 0), 0).0,
        "pass 0 is as written"
    );
    assert!(heard(&engine, 0).iter().all(|run| run.renders == 0));
    for pass in [1, 2, 8] {
        let (heard_out, engine) = play_pass(&song, pass);
        let (written_out, _) = play_pass(&kept(&song, pass as i64), pass);
        assert!(heard_out == written_out, "pass {pass} plays its seed");
        let (as_written, _) = play_pass(&kept(&song, 0), pass);
        assert!(heard_out != as_written, "and not the written one");
        let run = heard(&engine, 0)[0];
        assert_eq!((run.pass, run.renders), (pass as i64, 1));
        assert!(run.moving);
    }
    // A render repeats.
    assert!(play_pass(&song, 2).0 == play_pass(&song, 2).0);
}

/// Play a song from the top a block of 256 frames at a time, as the callback does, taking the
/// passes heard after each, as the callback reports them.
fn play_reporting(song: &Song, steps: usize) -> (Vec<(i64, u64)>, Engine) {
    let mut engine = Engine::new(RATE, version(song));
    engine.play_from(0);
    let mut out = [0.0; 512];
    let mut reported = Vec::new();
    for _ in 0..steps * STEP / 256 {
        engine.process(&mut out);
        reported.extend(passes_heard(&mut engine, 0));
    }
    (reported, engine)
}

/// A generator that varies renders once a pass, not once a row, and every `vary` passes only
/// when the seed changes; each pass it's heard on is reported once, with the seed it played.
#[test]
fn a_generator_that_varies_renders_once_a_pass_and_reports_each() {
    let song = varying(1);
    let (reported, mut engine) = play_reporting(&song, 3 * 16 + 2);
    assert_eq!(heard(&engine, 0)[0].renders, 3, "passes 1, 2 and 3");
    let shape = written(&song, 0, ShapeColumn::Velocity);
    let seeds: Vec<(i64, u64)> = (0..4)
        .map(|pass| (pass, shape.on_pass(pass).seed))
        .collect();
    assert_eq!(reported, seeds);
    assert_eq!(passes_heard(&mut engine, 0), [], "each only once");
    let fours = varying(4);
    let (reported, engine) = play_reporting(&fours, 8 * 16 + 2);
    assert_eq!(
        heard(&engine, 0)[0].renders,
        2,
        "passes 4 to 7 share a seed, and 8 has the next"
    );
    let shape = written(&fours, 0, ShapeColumn::Velocity);
    let seeds: Vec<u64> = reported.into_iter().map(|(_, seed)| seed).collect();
    assert_eq!(seeds.len(), 9);
    assert!(seeds[..4].iter().all(|seed| *seed == shape.seed));
    assert!(seeds[4..8].iter().all(|seed| *seed == seeds[4]));
    assert_ne!(seeds[8], seeds[4]);
}

/// Played from the ninth pass, a generator that varies plays and reports what it does there
/// playing from the top.
#[test]
fn a_generator_that_varies_hears_the_pass_where_the_song_is() {
    let song = varying(1);
    let (_, mut from_top) = play(&song, &[], 8 * 16 + 4);
    let mut engine = Engine::new(RATE, version(&song));
    engine.play_from(8 * 16);
    let mut out = vec![0.0; 4 * STEP * 2];
    engine.process(&mut out);
    let last = |engine: &mut Engine| passes_heard(engine, 0).last().copied();
    let here = last(&mut engine);
    assert_eq!(here.map(|(pass, _)| pass), Some(8));
    assert_eq!(last(&mut from_top), here);
}

/// One synth track of sixteen quiet notes, `20`, with accents of `7F` on nine of them from a
/// Euclid kept live on their velocities.
fn accents() -> Song {
    let mut song = Song::new(1);
    song.millibpm = 128_000;
    let quiet: Step = "C-3 20 --- --- ---".parse().unwrap();
    song.tracks[0].machine = Machine::Plaits(SynthVoice::VirtualAnalog);
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(vec![quiet; 16])));
    generate(
        song,
        0,
        ShapeColumn::Velocity,
        rows(15),
        shape(
            FillKind::Alternate,
            0x7F,
            0x7F,
            Where::Euclid { hits: 9, rotate: 0 },
        ),
    )
}

/// A held setting that hits fewer rows plays what's under the generator on the rows it no longer
/// hits, as writing it would, not the accents written there at rest.
#[test]
fn a_held_setting_hitting_fewer_rows_plays_whats_under_on_the_rest() {
    let song = accents();
    let hold = (0, ShapeColumn::Velocity, GenParam::Hits, 3);
    let (held, engine) = play(&song, &[hold], 16);
    let written = reshaped(&song, hold);
    let quiet = written.tracks[0].phrases[&0]
        .steps()
        .iter()
        .filter(|step| step.velocity == Some(0x20))
        .count();
    assert_eq!(quiet, 13, "three accents, the rest as under");
    let (writes, _) = play(&written, &[], 16);
    assert!(
        held == writes,
        "held, it plays as written, with what's under"
    );
    assert!(heard(&engine, 0).iter().any(|run| run.moving));
}

/// The nine-pass song with a line of its own on the Note column, a live `Variation` of it at
/// order 1, a new variation every pass.
fn varying_line() -> Song {
    let mut song = nine_passes();
    let line = [
        "C-2 7F --- --- ---",
        "--- -- --- --- ---",
        "C-2 50 --- --- ---",
        "D#2 50 HLD1 --- --- ---",
        "G-2 50 --- --- ---",
        "--- -- --- --- ---",
        "C-2 7F --- --- ---",
        "C-3 50 --- --- ---",
        "--- -- --- --- ---",
        "A#2 50 HLD1 --- --- ---",
        "C-2 50 --- --- ---",
        "OFF -- --- --- ---",
        "C-2 7F --- --- ---",
        "--- -- --- --- ---",
        "G-2 50 --- --- ---",
        "D#2 7F --- --- ---",
    ];
    let steps: Vec<Step> = line.iter().map(|row| row.parse().unwrap()).collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let mut line = shape(FillKind::Variation, 0, 0, Where::AllRows);
    line.variation = vt_model::shape::Variation {
        order: 1,
        temperature: 100,
    };
    line.vary = 1;
    generate(song, 0, notes(), rows(15), line)
}

fn notes() -> ShapeColumn {
    ShapeColumn::Note {
        scale: vt_model::scale::Scale::default(),
        writes: None,
    }
}

/// A `Variation` draws from its seed, so `Vary` gives each pass a variation of its own, which
/// plays sample for sample as writing that pass's seed would; and holding its `Temperature`
/// plays what writing it would.
#[test]
fn a_variation_varies_by_pass_and_its_temperature_can_be_held() {
    let song = varying_line();
    let kept = |pass: i64| {
        let mut shape = written(&song, 0, notes()).on_pass(pass);
        shape.vary = 0;
        reshape(&song, shape)
    };
    let (first, engine) = play_pass(&song, 0);
    assert!(first == play_pass(&kept(0), 0).0, "pass 0 is as written");
    assert!(heard(&engine, 0).iter().all(|run| run.renders == 0));
    for pass in [1, 2] {
        let (heard_out, _) = play_pass(&song, pass);
        assert!(
            heard_out == play_pass(&kept(pass as i64), pass).0,
            "pass {pass}"
        );
        assert!(
            heard_out != play_pass(&kept(0), pass).0,
            "pass {pass} differs"
        );
    }
    let cold = (0, notes(), GenParam::Temperature, 0);
    let still = kept(0);
    let (held, _) = play(&still, &[cold], 16);
    let (written_out, _) = play(&reshaped(&still, cold), &[], 16);
    assert!(held == written_out, "a held temperature plays as written");
    assert!(held != play(&still, &[], 16).0, "and not as before");
}
