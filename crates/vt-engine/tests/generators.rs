//! Live generators as the engine plays them (decision 288): at rest, what's heard is what's
//! written, sample for sample; a generator whose settings the engine holds elsewhere is rendered
//! again from its `Shape` and heard so, at once, a pass at a time from where the song is.

mod support;

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::gens::GenHeard;
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
        ShapeColumn::Nudge,
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
        ShapeColumn::Nudge,
        rows(15),
        shape(FillKind::Ramp, 11, 22, Where::AllRows),
    )
}

#[test]
fn a_nudge_generator_at_rest_renders_as_before_however_early_it_could_go() {
    let out = render_loop(version(&late_song()), RATE, RenderOptions::default());
    golden("generators-late-at-rest", &out);
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
        (1, ShapeColumn::Nudge, GenParam::From),
        (1, ShapeColumn::Length, GenParam::To),
        (1, res(), GenParam::From),
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
