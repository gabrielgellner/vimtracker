//! `vimtracker probe`: play one voice or one effect with no song, offline, from a spec that the
//! reference side in `tools/compare/` reads too, so a comparison is written once and rendered by
//! both (#186, decision 110).
//!
//! A spec is TOML. Its `[ours]` names a machine and the effects in a track's slots, and sets them
//! by the keys songs store, such as `acid.cutoff` or `1.comp.threshold`, with the values the
//! program shows, `00` to `FF` or a choice's name. Its `[reference]` is for the reference side,
//! and isn't read here. Between them go what both play: `notes` for a voice, or `input` for an
//! effect, at a `rate` for a `duration` in seconds. A spec can have `cases`, each changing some of
//! either side's settings, and one is rendered at a time.
//!
//! A note is played as the keyboard plays one, held through the track for its length, so the
//! track's sound, filter and effects are all heard and nothing is sequenced. An effect's input is
//! played through the track as it is, at full level with no fades.
//!
//! A spec's `[report]` says how `tools/compare/report.py` lays out its measurements, and isn't read
//! here either.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use vt_engine::dsp::filter::track_response_db;
use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_engine::render::write_wav;
use vt_model::constants::DEFAULT_NOTE_VELOCITY;
use vt_model::insert::{INSERTS, Insert};
use vt_model::project;
use vt_model::sample::{SampleData, SampleError, load_sample};
use vt_model::samples::SamplePool;
use vt_model::settings::{Effect, Kind, Owner, SettingId, voice_key};
use vt_model::song::{FilterMode, FilterType, Machine, Song, SynthVoice, Track};
use vt_model::step::{ACCENT_VELOCITY, MAX_VELOCITY};

/// The rate a spec renders at unless it says.
pub const DEFAULT_RATE: u32 = 48_000;
/// The rates a spec may ask for.
const RATES: std::ops::RangeInclusive<u32> = 8_000..=192_000;
/// The longest render, in seconds, so a mistyped duration can't fill the disk.
const LONGEST_SECONDS: f64 = 600.0;

/// Why a probe couldn't be rendered.
#[derive(Debug, Error)]
pub enum ProbeError {
    #[error("couldn't read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("couldn't write {path}: {source}")]
    WriteText {
        path: String,
        source: std::io::Error,
    },
    #[error("{0}")]
    Parse(#[from] toml::de::Error),
    #[error("{0}")]
    Spec(String),
    #[error(transparent)]
    Sample(#[from] SampleError),
    #[error("couldn't write the probe: {0}")]
    Write(#[from] hound::Error),
}

fn wrong(message: impl Into<String>) -> ProbeError {
    ProbeError::Spec(message.into())
}

/// A spec as written.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    /// Frames a second.
    #[serde(default = "default_rate")]
    pub rate: u32,
    /// How long to render, in seconds. An effect's defaults to its input's length.
    pub duration: Option<f64>,
    /// The notes a voice plays, in any order.
    #[serde(default)]
    pub notes: Vec<NoteSpec>,
    /// What an effect is given.
    pub input: Option<Input>,
    pub ours: Ours,
    /// The reference plugin and its parameters, which only the reference side reads.
    pub reference: Option<toml::Table>,
    /// Versions of the spec, each changing some settings, rendered one at a time.
    #[serde(default)]
    pub cases: Vec<Case>,
    /// How the report lays out what's measured, which only the reference side reads.
    pub report: Option<toml::Table>,
}

fn default_rate() -> u32 {
    DEFAULT_RATE
}

/// Our side: the machine, the slots' effects and their settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ours {
    /// `sampler`, or a synth voice by its key, such as `acid`.
    pub machine: Option<String>,
    /// The file a sampler plays, from the spec's folder.
    pub sample: Option<PathBuf>,
    /// The effects in the track's slots, from the first, by their keys, such as `comp`.
    #[serde(default)]
    pub effects: Vec<String>,
    /// Settings by the keys songs store. Dotted keys can be quoted or bare, so
    /// `"acid.cutoff" = 0x40` and `acid.cutoff = 0x40` are the same.
    #[serde(default)]
    pub settings: toml::Table,
}

/// A version of the spec.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub name: String,
    /// Our settings it changes or adds.
    #[serde(default)]
    pub ours: toml::Table,
    /// The reference's parameters it changes or adds, which only the reference side reads.
    #[serde(default)]
    pub reference: toml::Table,
    /// The input's fields it changes, such as a burst's `on`, or a whole input of another
    /// `signal`.
    #[serde(default)]
    pub input: toml::Table,
}

/// A note, held for its length.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoteSpec {
    /// When it starts, in seconds.
    pub time: f64,
    /// How long it's held, in seconds.
    pub length: f64,
    /// A MIDI note number, where 60 is the program's `C-4`.
    pub note: u8,
    /// From 1 to 127, as MIDI's and the program's `00` to `7F` are.
    pub velocity: Option<u8>,
    /// Raise the velocity to an accent's, or keep it under one. Without it, the velocity decides,
    /// as it does in the program.
    pub accent: Option<bool>,
    /// Hold on into the next note, so a voice that slides glides there.
    #[serde(default)]
    pub slide: bool,
}

impl NoteSpec {
    /// The velocity it's played at, with its accent.
    pub fn velocity(&self) -> u8 {
        let velocity = self.velocity.unwrap_or(DEFAULT_NOTE_VELOCITY);
        match self.accent {
            Some(true) => velocity.max(ACCENT_VELOCITY),
            Some(false) => velocity.min(ACCENT_VELOCITY - 1),
            None => velocity,
        }
    }
}

/// What an effect is given: a file, or a signal made the same way on both sides. Levels are in dB
/// of full scale at the peak, times in seconds and frequencies in Hz.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "signal", rename_all = "lowercase", deny_unknown_fields)]
pub enum Input {
    /// A file, from the spec's folder, made stereo at the spec's rate.
    File { path: PathBuf },
    /// A steady sine.
    Sine {
        frequency: f64,
        level: f64,
        seconds: f64,
    },
    /// An exponential sweep, as Farina's, from one frequency to another.
    Sweep {
        from: f64,
        to: f64,
        level: f64,
        seconds: f64,
    },
    /// A sine at levels from `from` to `to` dB, `step` dB apart, each held for `hold` seconds, its
    /// phase running on through them, for a compressor's static curve or THD against level.
    Steps {
        frequency: f64,
        from: f64,
        to: f64,
        #[serde(default = "one_db")]
        step: f64,
        hold: f64,
    },
    /// A sine at `rest` for `before` seconds, at `level` for `on`, and at `rest` again for `after`,
    /// for attack and release.
    Burst {
        frequency: f64,
        level: f64,
        rest: f64,
        before: f64,
        on: f64,
        after: f64,
    },
    /// Nothing, for a voice or an effect that rings on its own.
    Silence { seconds: f64 },
    /// One sample at `level`, then nothing, for `seconds` in all: a kick for a filter to ring
    /// from, as one that self-oscillates has nothing to start it in silence.
    Impulse { level: f64, seconds: f64 },
}

fn one_db() -> f64 {
    1.0
}

/// A spec read from a file, and its folder, which paths in it start from.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub spec: Spec,
    pub folder: PathBuf,
}

/// Read a spec from a file.
pub fn read(path: &Path) -> Result<Loaded, ProbeError> {
    let text = std::fs::read_to_string(path).map_err(|source| ProbeError::Read {
        path: path.display().to_string(),
        source,
    })?;
    let spec = parse(&text)?;
    let folder = path.parent().map(Path::to_path_buf).unwrap_or_default();
    Ok(Loaded { spec, folder })
}

/// Read a spec from its text, and check what can be checked without a file.
pub fn parse(text: &str) -> Result<Spec, ProbeError> {
    let spec: Spec = toml::from_str(text)?;
    if !RATES.contains(&spec.rate) {
        return Err(wrong(format!(
            "a rate of {} isn't one a probe plays at, which are {} to {}",
            spec.rate,
            RATES.start(),
            RATES.end()
        )));
    }
    match (spec.notes.is_empty(), &spec.input) {
        (true, None) => {
            return Err(wrong(
                "a spec needs `notes` for a voice to play or an `input` for an effect",
            ));
        }
        (false, Some(_)) => {
            return Err(wrong(
                "a spec has `notes` for a voice or an `input` for an effect, not both",
            ));
        }
        (false, None) if spec.duration.is_none() => {
            return Err(wrong(
                "a voice's spec needs a `duration`, how many seconds to render",
            ));
        }
        _ => {}
    }
    if let Some(duration) = spec.duration
        && !(duration > 0.0 && duration <= LONGEST_SECONDS)
    {
        return Err(wrong(format!(
            "a duration of {duration} s isn't one a probe renders, which is up to {LONGEST_SECONDS} s"
        )));
    }
    for note in &spec.notes {
        if !(note.time >= 0.0 && note.length > 0.0) {
            return Err(wrong(format!(
                "the note at {} s for {} s needs a time from 0 and a length over 0",
                note.time, note.length
            )));
        }
        if note.note > 127 || !(1..=MAX_VELOCITY).contains(&note.velocity()) {
            return Err(wrong(format!(
                "the note at {} s needs a note from 0 to 127 and a velocity from 1 to {MAX_VELOCITY}",
                note.time
            )));
        }
    }
    let mut names = Vec::new();
    for case in &spec.cases {
        if names.contains(&case.name.as_str()) {
            return Err(wrong(format!("there are two cases called {}", case.name)));
        }
        names.push(case.name.as_str());
    }
    // Every case's settings are checked now, so a mistake in one shows before any renders.
    let machine = machine(&spec.ours)?;
    let effects = effects(&spec.ours)?;
    check_settings(&settings(&spec.ours.settings)?, machine, &effects)?;
    for case in &spec.cases {
        check_settings(&settings(&case.ours)?, machine, &effects)
            .and_then(|()| input_for(&spec, Some(case)).map(drop))
            .map_err(|error| wrong(format!("in the case {}: {error}", case.name)))?;
    }
    Ok(spec)
}

/// The input a spec, or one of its cases, gives: the spec's, with the case's fields over it, or
/// the case's alone if it names another `signal`.
fn input_for(spec: &Spec, case: Option<&Case>) -> Result<Option<Input>, ProbeError> {
    let changes = case
        .map(|case| &case.input)
        .filter(|input| !input.is_empty());
    let (Some(input), Some(changes)) = (&spec.input, changes) else {
        return match (&spec.input, changes) {
            (None, Some(_)) => Err(wrong(
                "a case changes the input, but the spec plays notes rather than an input",
            )),
            (input, _) => Ok(input.clone()),
        };
    };
    let mut table = match toml::Value::try_from(input) {
        Ok(toml::Value::Table(table)) => table,
        _ => toml::Table::new(),
    };
    if changes
        .get("signal")
        .is_some_and(|signal| table.get("signal") != Some(signal))
    {
        table.clear();
    }
    table.extend(changes.clone());
    toml::Value::Table(table)
        .try_into()
        .map(Some)
        .map_err(|error: toml::de::Error| wrong(format!("its input: {}", error.message())))
}

/// The key that sets the track's filter type, which a song keeps on the track rather than as a
/// setting, so it has no [`SettingId`].
const FILTER_KEY: &str = "filter";

/// The filter type a spec names, by the name the Sound page shows.
fn filter_of(given: &Given) -> Result<FilterType, ProbeError> {
    let names = || FilterType::ALL.map(FilterType::name).join(", ");
    let Given::Choice(name) = given else {
        return Err(wrong(format!(
            "{FILTER_KEY} takes a filter type's name: {}",
            names()
        )));
    };
    FilterType::ALL
        .into_iter()
        .find(|kind| kind.name().eq_ignore_ascii_case(name))
        .ok_or_else(|| {
            wrong(format!(
                "there's no filter type called {name}; there are {}",
                names()
            ))
        })
}

/// The machine a spec names, if any.
fn machine(ours: &Ours) -> Result<Option<Machine>, ProbeError> {
    let Some(name) = &ours.machine else {
        return Ok(None);
    };
    if name == "sampler" {
        return Ok(Some(Machine::Sampler));
    }
    SynthVoice::ALL
        .into_iter()
        .find(|voice| voice_key(*voice) == name)
        .map(|voice| Some(Machine::Plaits(voice)))
        .ok_or_else(|| {
            let known: Vec<_> = std::iter::once("sampler")
                .chain(SynthVoice::ALL.map(voice_key))
                .collect();
            wrong(format!(
                "there's no machine called {name}; there are {}",
                known.join(", ")
            ))
        })
}

/// The effects a spec puts in the slots, from the first.
fn effects(ours: &Ours) -> Result<Vec<Effect>, ProbeError> {
    if ours.effects.len() > INSERTS {
        return Err(wrong(format!(
            "a track has only {INSERTS} slots for effects"
        )));
    }
    ours.effects
        .iter()
        .map(|name| {
            Effect::ALL
                .into_iter()
                .find(|effect| effect.key() == name)
                .ok_or_else(|| {
                    let known = Effect::ALL.map(Effect::key);
                    wrong(format!(
                        "there's no effect called {name}; there are {}",
                        known.join(", ")
                    ))
                })
        })
        .collect()
}

/// A setting's value as a spec gives it: a number, or the name of one of its choices.
#[derive(Debug, Clone, PartialEq)]
enum Given {
    Number(i64),
    Choice(String),
}

/// Settings by key, with bare dotted keys' tables flattened back into keys.
fn settings(table: &toml::Table) -> Result<BTreeMap<String, Given>, ProbeError> {
    fn flatten(
        prefix: &str,
        table: &toml::Table,
        out: &mut BTreeMap<String, Given>,
    ) -> Result<(), ProbeError> {
        for (key, value) in table {
            let key = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{prefix}.{key}")
            };
            let given = match value {
                toml::Value::Table(inner) => {
                    flatten(&key, inner, out)?;
                    continue;
                }
                toml::Value::Integer(number) => Given::Number(*number),
                toml::Value::String(choice) => Given::Choice(choice.clone()),
                other => {
                    return Err(wrong(format!(
                        "{key} is a {}, where a setting takes a whole number such as 0x40 or \
                         the name of a choice",
                        other.type_str()
                    )));
                }
            };
            out.insert(key, given);
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    flatten("", table, &mut out)?;
    Ok(out)
}

/// The setting a key names, or why it can't be set here.
fn setting_of(
    key: &str,
    machine: Option<Machine>,
    effects: &[Effect],
) -> Result<SettingId, ProbeError> {
    let Some(id) = SettingId::from_key(key) else {
        // The settings beside the one meant: those of the longest start of the key that has any,
        // so `1.comp.thresh` lists the compressor's.
        let mut start = key;
        while let Some((head, _)) = start.rsplit_once('.') {
            let near: Vec<_> = SettingId::all()
                .map(SettingId::key)
                .filter(|known| {
                    known
                        .strip_prefix(head)
                        .is_some_and(|rest| rest.starts_with('.'))
                })
                .collect();
            if !near.is_empty() {
                return Err(wrong(format!(
                    "there's no setting called {key}; those starting {head} are {}",
                    near.join(", ")
                )));
            }
            start = head;
        }
        return Err(wrong(format!("there's no setting called {key}")));
    };
    match id.owner() {
        (Owner::Voice(voice), _) if machine != Some(Machine::Plaits(voice)) => Err(wrong(format!(
            "{key} is the {} voice's, which the spec's machine isn't",
            voice.name()
        ))),
        (Owner::Sampler, _) if machine != Some(Machine::Sampler) => Err(wrong(format!(
            "{key} is the sampler's, which the spec's machine isn't"
        ))),
        (Owner::Slot { slot, effect }, _) if effects.get(slot) != Some(&effect) => {
            Err(wrong(format!(
                "{key} is for {} in slot {}, which the spec's effects don't put there",
                effect.name(),
                slot + 1
            )))
        }
        (Owner::Bypass(slot), _) if effects.get(slot).is_none() => Err(wrong(format!(
            "{key} switches out slot {}, which the spec's effects leave empty",
            slot + 1
        ))),
        (Owner::Lfo(_), _) => Err(wrong(format!(
            "{key} is an LFO's, which a probe doesn't run yet"
        ))),
        (Owner::Track, _) if !TRACK_SETTINGS.contains(&id) => Err(wrong(format!(
            "{key} works on a song's steps, which a probe has none of; a probe sets {}",
            TRACK_SETTINGS.map(SettingId::key).join(", ")
        ))),
        _ => Ok(id),
    }
}

/// The track's own settings a probe can set: its level, pan, filter, pitch and glide.
const TRACK_SETTINGS: [SettingId; 7] = [
    SettingId::VOL,
    SettingId::PAN,
    SettingId::CUT,
    SettingId::RES,
    SettingId::FILTER_MODE,
    SettingId::PITCH,
    SettingId::GLIDE,
];

/// A setting's value from what a spec gives, within what it takes.
fn value_of(key: &str, id: SettingId, given: &Given) -> Result<u8, ProbeError> {
    let kind = id.entry().kind;
    let choices = match kind {
        Kind::Stepped { choices, .. } => choices,
        Kind::Continuous { .. } => &[],
    };
    match given {
        Given::Number(number) => u8::try_from(*number)
            .ok()
            .filter(|value| *value <= kind.top())
            .ok_or_else(|| {
                wrong(format!(
                    "{key} = {number} is out of its range, 0 to {0} ({0:#04x})",
                    kind.top()
                ))
            }),
        Given::Choice(name) => choices
            .iter()
            .position(|choice| choice.eq_ignore_ascii_case(name))
            .map(|at| at as u8)
            .ok_or_else(|| {
                if choices.is_empty() {
                    wrong(format!(
                        "{key} takes a number such as 0x40, not a choice's name"
                    ))
                } else {
                    wrong(format!(
                        "{key} has no choice called {name}; it has {}",
                        choices.join(", ")
                    ))
                }
            }),
    }
}

fn check_settings(
    settings: &BTreeMap<String, Given>,
    machine: Option<Machine>,
    effects: &[Effect],
) -> Result<(), ProbeError> {
    for (key, given) in settings {
        if key == FILTER_KEY {
            filter_of(given)?;
            continue;
        }
        let id = setting_of(key, machine, effects)?;
        value_of(key, id, given)?;
    }
    Ok(())
}

/// Set a setting where a track keeps it before any step changes it.
fn set_on(track: &mut Track, id: SettingId, value: u8) {
    match id.owner() {
        (Owner::Voice(_) | Owner::Sampler, _) => track.set_setting(id, value),
        (Owner::Slot { slot, effect }, at) => {
            track.inserts[slot] =
                track.inserts[slot].write(effect.params()[at].setting, value.into());
        }
        (Owner::Bypass(slot), _) => track.bypassed[slot] = value != 0,
        (Owner::Track, _) => match id {
            SettingId::VOL => track.level = value,
            SettingId::PAN => track.pan = value,
            SettingId::CUT => track.cutoff = value,
            SettingId::RES => track.resonance = value,
            SettingId::FILTER_MODE => {
                track.filter_mode = FilterMode::ALL
                    .get(usize::from(value))
                    .copied()
                    .unwrap_or_default();
            }
            SettingId::PITCH => track.pitch = value,
            SettingId::GLIDE => track.glide = value,
            _ => {}
        },
        (Owner::Lfo(_), _) => {}
    }
}

/// The empty slot's effect, as the program puts a new one in.
fn fresh(effect: Effect) -> Insert {
    Insert::ALL
        .into_iter()
        .find(|insert| Effect::of(*insert) == Some(effect))
        .unwrap_or_default()
}

/// What the probe renders: a song of one track set up as the spec says, and what it plays.
pub struct Probe {
    pub rate: u32,
    pub song: Song,
    pub samples: SamplePool,
    pub frames: usize,
    pub play: Play,
}

/// What goes through the track.
pub enum Play {
    Notes(Vec<NoteSpec>),
    Input(Arc<SampleData>),
}

/// Frames in so many seconds, rounded to the nearest, as the reference side rounds them.
pub fn frames_in(seconds: f64, rate: u32) -> usize {
    (seconds * f64::from(rate) + 0.5).floor().max(0.0) as usize
}

/// A level in dB of full scale as a gain.
fn gain_of(db: f64) -> f64 {
    10_f64.powf(db / 20.0)
}

/// An input signal as mono samples. The reference side makes them by the same formulas, in
/// double precision, so the two agree to a float's precision.
pub fn signal(input: &Input, rate: u32, folder: &Path) -> Result<Vec<[f32; 2]>, ProbeError> {
    let rate_f = f64::from(rate);
    let sine = |frequency: f64, frame: usize| {
        (std::f64::consts::TAU * frequency * frame as f64 / rate_f).sin()
    };
    let mono = |samples: Vec<f64>| samples.into_iter().map(|x| [x as f32; 2]).collect();
    Ok(match input {
        Input::File { path } => load_sample(&folder.join(path), rate)?.frames,
        Input::Sine {
            frequency,
            level,
            seconds,
        } => {
            let gain = gain_of(*level);
            mono(
                (0..frames_in(*seconds, rate))
                    .map(|frame| gain * sine(*frequency, frame))
                    .collect(),
            )
        }
        Input::Sweep {
            from,
            to,
            level,
            seconds,
        } => {
            if !(*from > 0.0 && *to > 0.0 && from != to) {
                return Err(wrong(
                    "a sweep goes between two different frequencies over 0",
                ));
            }
            let gain = gain_of(*level);
            let ratio = (to / from).ln();
            mono(
                (0..frames_in(*seconds, rate))
                    .map(|frame| {
                        let t = frame as f64 / rate_f;
                        let phase = std::f64::consts::TAU * from * seconds / ratio
                            * ((t / seconds * ratio).exp() - 1.0);
                        gain * phase.sin()
                    })
                    .collect(),
            )
        }
        Input::Steps {
            frequency,
            from,
            to,
            step,
            hold,
        } => {
            if !(*step > 0.0 && to >= from) {
                return Err(wrong(
                    "stepped levels go up from `from` to `to` by a `step` over 0",
                ));
            }
            let count = ((to - from) / step + 1e-9).floor() as usize + 1;
            let held = frames_in(*hold, rate);
            mono(
                (0..count * held)
                    .map(|frame| {
                        let level = from + step * (frame / held) as f64;
                        gain_of(level) * sine(*frequency, frame)
                    })
                    .collect(),
            )
        }
        Input::Burst {
            frequency,
            level,
            rest,
            before,
            on,
            after,
        } => {
            let starts = frames_in(*before, rate);
            let ends = starts + frames_in(*on, rate);
            let total = ends + frames_in(*after, rate);
            mono(
                (0..total)
                    .map(|frame| {
                        let level = if (starts..ends).contains(&frame) {
                            level
                        } else {
                            rest
                        };
                        gain_of(*level) * sine(*frequency, frame)
                    })
                    .collect(),
            )
        }
        Input::Silence { seconds } => vec![[0.0; 2]; frames_in(*seconds, rate)],
        Input::Impulse { level, seconds } => {
            let mut frames = vec![[0.0; 2]; frames_in(*seconds, rate).max(1)];
            frames[0] = [gain_of(*level) as f32; 2];
            frames
        }
    })
}

/// The case a spec renders, by name: none for a spec without cases, which must have one chosen.
fn chosen_case<'a>(spec: &'a Spec, case: Option<&str>) -> Result<Option<&'a Case>, ProbeError> {
    let names = || {
        spec.cases
            .iter()
            .map(|case| case.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    match (case, spec.cases.is_empty()) {
        (None, true) => Ok(None),
        (None, false) => Err(wrong(format!(
            "this spec has cases, so choose one with --case: {}",
            names()
        ))),
        (Some(name), _) => spec
            .cases
            .iter()
            .find(|case| case.name == name)
            .map(Some)
            .ok_or_else(|| {
                wrong(format!(
                    "there's no case called {name}; there are {}",
                    names()
                ))
            }),
    }
}

/// The spec's settings with a case's over them.
fn chosen_settings(
    spec: &Spec,
    case: Option<&Case>,
) -> Result<BTreeMap<String, Given>, ProbeError> {
    let mut chosen = settings(&spec.ours.settings)?;
    if let Some(case) = case {
        chosen.extend(settings(&case.ours)?);
    }
    Ok(chosen)
}

/// Set up what a spec, or one of its cases, renders.
pub fn prepare(loaded: &Loaded, case: Option<&str>) -> Result<Probe, ProbeError> {
    let spec = &loaded.spec;
    let machine = machine(&spec.ours)?;
    let effects = effects(&spec.ours)?;
    let mut song = Song::new(1);
    // The limiter would clip what's measured, and a probe's output goes nowhere near speakers.
    song.limiter = false;
    let mut samples = SamplePool::default();
    match machine {
        Some(Machine::Sampler) => {
            let Some(sample) = &spec.ours.sample else {
                return Err(wrong("a sampler needs a `sample` to play"));
            };
            let path = loaded.folder.join(sample);
            let path = path.to_string_lossy();
            song.sample_on(0, &path);
            if let Some(error) = project::load_samples(None, &song, spec.rate, &mut samples)
                .into_iter()
                .next()
            {
                return Err(error.into());
            }
        }
        Some(machine) => song.tracks[0].machine = machine,
        None if !spec.notes.is_empty() => {
            return Err(wrong("a spec with notes needs a `machine` to play them"));
        }
        None => {}
    }
    let track = &mut song.tracks[0];
    for (slot, effect) in effects.iter().enumerate() {
        track.inserts[slot] = fresh(*effect);
    }
    let case = chosen_case(spec, case)?;
    for (key, given) in chosen_settings(spec, case)? {
        if key == FILTER_KEY {
            track.filter = filter_of(&given)?;
            continue;
        }
        let id = setting_of(&key, machine, &effects)?;
        set_on(track, id, value_of(&key, id, &given)?);
    }
    let (play, length) = match input_for(spec, case)? {
        Some(input) => {
            let frames = signal(&input, spec.rate, &loaded.folder)?;
            let length = frames.len();
            (Play::Input(Arc::new(SampleData { frames })), length)
        }
        None => (Play::Notes(spec.notes.clone()), 0),
    };
    let frames = spec
        .duration
        .map_or(length, |seconds| frames_in(seconds, spec.rate));
    Ok(Probe {
        rate: spec.rate,
        song,
        samples,
        frames,
        play,
    })
}

/// What happens at a frame, in the order things at the same frame happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Happening {
    /// A note's own end, before a note starting at the same frame, so they don't overlap.
    Release(u8),
    Start(u8, u8),
    /// A sliding note's end, after the next note has started, so the voice glides there.
    SlideRelease(u8),
}

/// When each note starts and is let go, in order.
fn happenings(notes: &[NoteSpec], rate: u32) -> Vec<(usize, Happening)> {
    let mut notes = notes.to_vec();
    notes.sort_by(|a, b| a.time.total_cmp(&b.time));
    let mut out = Vec::new();
    for (index, note) in notes.iter().enumerate() {
        let start = frames_in(note.time, rate);
        let end = frames_in(note.time + note.length, rate).max(start + 1);
        out.push((start, Happening::Start(note.note, note.velocity())));
        let next = notes.get(index + 1).map(|next| frames_in(next.time, rate));
        match next {
            Some(next) if note.slide && next >= end => {
                out.push((next, Happening::SlideRelease(note.note)));
            }
            _ => out.push((end, Happening::Release(note.note))),
        }
    }
    out.sort();
    out
}

/// Render a probe as interleaved stereo frames.
pub fn render(probe: Probe) -> Vec<f32> {
    let Probe {
        rate,
        song,
        samples,
        frames,
        play,
    } = probe;
    let version = Published::new(Arc::new(song), Arc::new(samples));
    let mut engine = Engine::new(rate, Arc::new(version));
    let mut out = vec![0.0; frames * 2];
    let happenings = match play {
        Play::Input(input) => {
            engine.play_input(0, input);
            Vec::new()
        }
        Play::Notes(notes) => happenings(&notes, rate),
    };
    // A key held twice, as a note slid into itself, is let go when the last of them is.
    let mut held = [0_u8; 128];
    let mut done = 0;
    for (frame, happening) in happenings {
        let frame = frame.min(frames);
        engine.process(&mut out[done * 2..frame * 2]);
        done = frame;
        match happening {
            Happening::Start(note, velocity) => {
                held[usize::from(note)] += 1;
                engine.hold_preview(0, note, velocity);
            }
            Happening::Release(note) | Happening::SlideRelease(note) => {
                let count = &mut held[usize::from(note)];
                *count = count.saturating_sub(1);
                if *count == 0 {
                    engine.release_preview(0, note);
                }
            }
        }
    }
    engine.process(&mut out[done * 2..]);
    out
}

/// What `vimtracker probe` was asked for.
#[derive(Debug, Clone)]
pub struct ProbeRequest {
    pub spec: PathBuf,
    pub out: PathBuf,
    pub case: Option<String>,
    /// Where to write the generated input too, so the reference side's can be checked against it.
    pub write_input: Option<PathBuf>,
    /// Where to write the track filter's response as the Sound page draws it, as CSV.
    pub response: Option<PathBuf>,
}

/// How many points a drawn response has to an octave, from 20 Hz to 20 kHz.
const RESPONSE_POINTS_PER_OCTAVE: u32 = 48;

/// The track filter's response as the Sound page draws it, from `track_response_db`, as pairs of
/// Hz and dB, so what's measured can be checked against what's drawn.
pub fn drawn_response(probe: &Probe) -> Vec<(f32, f32)> {
    let octaves = 1000_f32.log2();
    let points = (octaves * RESPONSE_POINTS_PER_OCTAVE as f32).round() as u32;
    (0..=points)
        .map(|point| {
            let hertz = 20.0 * (octaves * point as f32 / points as f32).exp2();
            (
                hertz,
                track_response_db(&probe.song.tracks[0], probe.rate, hertz),
            )
        })
        .collect()
}

/// `vimtracker probe`: render a spec, or one of its cases, to a WAV file.
pub fn run(request: &ProbeRequest) -> Result<(), ProbeError> {
    let loaded = read(&request.spec)?;
    let probe = prepare(&loaded, request.case.as_deref())?;
    if let (Some(path), Play::Input(input)) = (&request.write_input, &probe.play) {
        let interleaved: Vec<f32> = input.frames.iter().flatten().copied().collect();
        write_wav(path, probe.rate, &interleaved)?;
    }
    if let Some(path) = &request.response {
        let mut text = String::from("hertz,db\n");
        for (hertz, db) in drawn_response(&probe) {
            text.push_str(&format!("{hertz},{db}\n"));
        }
        std::fs::write(path, text).map_err(|source| ProbeError::WriteText {
            path: path.display().to_string(),
            source,
        })?;
    }
    let rate = probe.rate;
    let samples = render(probe);
    write_wav(&request.out, rate, &samples)?;
    println!(
        "Probed {} ({:.2} s)",
        request.out.display(),
        samples.len() as f64 / 2.0 / f64::from(rate)
    );
    Ok(())
}
