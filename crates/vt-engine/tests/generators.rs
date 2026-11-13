//! Live generators as the engine plays them (decision 288): at rest, what's heard is what's
//! written, sample for sample; a generator whose settings the engine holds elsewhere is rendered
//! again from its `Shape` and heard so, at once, a pass at a time from where the song is.

mod support;

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_loop};
use vt_model::action::{Action, edit};
use vt_model::generator::Span;
use vt_model::lfo::Shape as Wave;
use vt_model::samples::SamplePool;
use vt_model::selection::StepRange;
use vt_model::settings::SettingId;
use vt_model::shape::{
    Acid, DEFAULT_RATE, FillKind, Follow, Pink, Shape, ShapeColumn, Values, Walk, Where,
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
    let song = generate(song, 0, acid, rows(15), shape(FillKind::Acid, 36, 48, Where::AllRows));
    let song = generate(song, 0, cut(), rows(15), shape(FillKind::Wave, 0x20, 0xC0, Where::AllRows));
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
