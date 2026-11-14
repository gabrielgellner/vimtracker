//! Presets: a track's sound, or one slot's effect, saved on its own to load on any track of any
//! song, as Elektron's sounds, the M8's instrument files and Ableton's device presets are
//! (decision 310). Each is a small RON file in the presets folder, `sounds/` or `effects/`, with
//! folders below those for sorting.
//!
//! A sound preset is the sound and its patch: [`Sound`], what makes the sound, from the machine to
//! the slots' effects, and [`Patch`], what plays it, the mod slots, the accumulator, the arpeggio
//! and the eight macros. Loading one replaces both, which makes it a whole device with its knobs.
//! A patch is also a preset of its own (decision 311), in `patches/`, which adds its parts to a
//! track and keeps the track's sound; it's the same [`Patch`], holding only the parts its macros
//! move, with the live generators they reach ([`patch`]).

mod patch;

pub use patch::{PatchGen, PatchPreset, Placement, Short, load_patch};

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::accumulator::Accumulation;
use crate::arpeggio::Arpeggio;
use crate::bank::BankEntry;
use crate::insert::{INSERTS, Insert};
use crate::macros::{MACROS, Macro};
use crate::modulation::{MOD_SLOTS, ModSlot, default_mod};
use crate::settings::{Effect, Owner, SettingId};
use crate::song::{FilterMode, FilterType, Machine, Track};

/// The format presets are written in. A preset from a later format isn't read; one from this
/// format missing a setting takes the setting's default, so presets outlive the builds that made
/// them, and one naming a setting this build doesn't know isn't read, and says which.
pub const PRESET_FORMAT: u32 = 1;

/// The most characters a preset's name has, with the folders before it.
pub const MAX_PRESET_NAME: usize = 64;

/// What a preset is, which decides its folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PresetKind {
    /// A track's sound and its patch.
    Sound,
    /// One slot's effect and its settings.
    Effect,
    /// What plays a sound without the sound: macros and the parts they move.
    Patch,
}

impl PresetKind {
    /// Its folder in the presets folder.
    pub fn folder(self) -> &'static str {
        match self {
            Self::Sound => "sounds",
            Self::Effect => "effects",
            Self::Patch => "patches",
        }
    }

    /// What it's called in a sentence.
    pub fn noun(self) -> &'static str {
        match self {
            Self::Sound => "sound",
            Self::Effect => "effect",
            Self::Patch => "patch",
        }
    }
}

/// Why a preset couldn't be read, written or named.
#[derive(Debug, Error)]
pub enum PresetError {
    #[error("{path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("{path}: {detail}")]
    Parse { path: String, detail: String },
    #[error(
        "{path} is a preset of format {found}, from a later vimtracker: this one reads {PRESET_FORMAT}"
    )]
    Format { path: String, found: u32 },
    #[error("{0} is taken: :preset! save writes over it")]
    Taken(String),
    #[error(
        "A preset's name is letters, digits, spaces, - _ . ( ) and / for folders, up to {MAX_PRESET_NAME}"
    )]
    Name,
    #[error("{path}: {detail}")]
    Holds { path: String, detail: String },
}

fn io_at(path: &Path) -> impl Fn(std::io::Error) -> PresetError + '_ {
    move |source| PresetError::Io {
        path: path.display().to_string(),
        source,
    }
}

/// Whether a preset can be called this: letters, digits, spaces and `-` `_` `.` `(` `)`, with `/`
/// between folders, none of them empty or starting with a dot, up to [`MAX_PRESET_NAME`].
pub fn is_preset_name(name: &str) -> bool {
    let allowed = |c: char| c.is_alphanumeric() || " -_.()".contains(c);
    name.chars().count() <= MAX_PRESET_NAME
        && name.split('/').all(|part| {
            !part.trim().is_empty() && !part.starts_with('.') && part.chars().all(allowed)
        })
}

/// Where a preset of this name is kept, in its kind's folder of the presets folder.
pub fn preset_path(presets: &Path, kind: PresetKind, name: &str) -> Result<PathBuf, PresetError> {
    if !is_preset_name(name) {
        return Err(PresetError::Name);
    }
    Ok(presets.join(kind.folder()).join(format!("{name}.ron")))
}

/// A preset's name, from its path: its file's name without `.ron`.
pub fn preset_name(path: &Path) -> String {
    path.file_stem()
        .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned())
}

fn is_zero(value: &u8) -> bool {
    *value == 0
}

fn is_off(value: &bool) -> bool {
    !*value
}

fn open() -> u8 {
    0xFF
}

fn is_open(value: &u8) -> bool {
    *value == 0xFF
}

fn centre() -> u8 {
    0x80
}

fn is_centre(value: &u8) -> bool {
    *value == 0x80
}

fn no_inserts(inserts: &[Insert; INSERTS]) -> bool {
    inserts.iter().all(|insert| *insert == Insert::Empty)
}

/// The settings that are the track's place in the song rather than its sound: its fader, pan and
/// sends, its groove, how long its steps are and how near the grid they're brought.
const PLACE: [SettingId; 8] = [
    SettingId::VOL,
    SettingId::LEVEL,
    SettingId::PAN,
    SettingId::SEND_A,
    SettingId::SEND_B,
    SettingId::GROOVE,
    SettingId::SPEED,
    SettingId::QUANTISE,
];

/// Whether a sound on this machine holds a setting, or its fine part: the machine's own, and not
/// another voice's the track remembers, and none of the track's place, or of a slot's bypass.
fn holds(machine: Machine, id: SettingId) -> bool {
    match id.owner().0 {
        Owner::Voice(voice) => machine == Machine::Plaits(voice),
        Owner::Sampler => machine == Machine::Sampler,
        Owner::Track => !PLACE.contains(&id),
        Owner::Bypass(_) => false,
        Owner::Slot { .. }
        | Owner::Mod(_)
        | Owner::Macro(_)
        | Owner::ModOwn(_)
        | Owner::Accumulation
        | Owner::Arpeggio => true,
    }
}

/// What makes a track's sound: its machine with its settings, its filters, how it plays its
/// notes, and its slots' effects. Not its level, trim, pan, sends, group, choke group, groove,
/// speed, reset, mute, solo, name or phrases, which are its place in the song.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sound {
    #[serde(default)]
    pub machine: Machine,
    /// The machine's settings that aren't at their defaults, by key, as a song stores them; only
    /// the voice it's on, and for the sampler not its `Sample`, which is [`Sound::sample`].
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub settings: BTreeMap<SettingId, u8>,
    /// The fine parts of its settings, its machine's and its effects'.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fine: BTreeMap<SettingId, u8>,
    #[serde(default = "open", skip_serializing_if = "is_open")]
    pub cutoff: u8,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub resonance: u8,
    #[serde(default)]
    pub filter: FilterType,
    #[serde(default)]
    pub filter_mode: FilterMode,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub drive: u8,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub punch: u8,
    #[serde(default, skip_serializing_if = "is_off")]
    pub safe: bool,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub low_cut: u8,
    #[serde(default = "open", skip_serializing_if = "is_open")]
    pub high_cut: u8,
    #[serde(default = "centre", skip_serializing_if = "is_centre")]
    pub pitch: u8,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub glide: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_length: Option<u8>,
    #[serde(default = "centre", skip_serializing_if = "is_centre")]
    pub velocity_delta: u8,
    /// The effects in its slots, in order.
    #[serde(default, skip_serializing_if = "no_inserts")]
    pub inserts: [Insert; INSERTS],
    /// The sample a sampler plays, as the bank had it, with its slices and how it plays, and its
    /// file by its full path, so it's found from any song. Loading puts it in that song's bank.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample: Option<BankEntry>,
}

/// What plays a sound rather than what it is: the mod slots, the accumulator, the arpeggio and
/// the macros, as the study's §5.2 has a patch. In a sound preset it holds every slot and macro in
/// order, numbered as the track's; on its own (decision 311) it holds only the parts its macros
/// move, numbered from the first in the patch, which go into free ones, and the live generators
/// they reach.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Patch {
    /// The mod slots from the first. Any left out start as a new track's do.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mods: Vec<ModSlot>,
    #[serde(default, skip_serializing_if = "is_default_accumulation")]
    pub accumulation: Accumulation,
    #[serde(default, skip_serializing_if = "is_default_arpeggio")]
    pub arpeggio: Arpeggio,
    /// The macros from the first, with their names, values, mappings and ranges, left out when
    /// none holds anything.
    #[serde(default, skip_serializing_if = "no_macros")]
    pub macros: Vec<Macro>,
    /// The live generators its macros reach, by column, from the phrase it was saved on. A sound
    /// preset holds none, as it holds no phrases.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub generators: Vec<PatchGen>,
}

fn is_default_accumulation(accumulation: &Accumulation) -> bool {
    *accumulation == Accumulation::default()
}

fn is_default_arpeggio(arpeggio: &Arpeggio) -> bool {
    *arpeggio == Arpeggio::default()
}

fn no_macros(macros: &[Macro]) -> bool {
    macros.iter().all(Macro::is_empty)
}

/// A sound preset's file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoundPreset {
    pub format: u32,
    pub sound: Sound,
    #[serde(default)]
    pub patch: Patch,
}

/// An effect preset's file: one slot's effect, and the fine parts of its settings by their own
/// keys, so it loads into any slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectPreset {
    pub format: u32,
    pub effect: Insert,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fine: BTreeMap<String, u8>,
}

impl SoundPreset {
    /// A track's sound and patch, with the bank entry its sampler plays, if it plays one, by the
    /// file's full path.
    pub fn of(track: &Track, sample: Option<BankEntry>) -> Self {
        let machine = track.machine;
        let kept = |map: &BTreeMap<SettingId, u8>| -> BTreeMap<SettingId, u8> {
            map.iter()
                .filter(|(id, _)| holds(machine, **id) && **id != SettingId::SAMPLE)
                .map(|(id, value)| (*id, *value))
                .collect()
        };
        let sample = sample.filter(|_| machine == Machine::Sampler);
        let sound = Sound {
            machine,
            settings: kept(&track.settings),
            fine: kept(&track.fine),
            cutoff: track.cutoff,
            resonance: track.resonance,
            filter: track.filter,
            filter_mode: track.filter_mode,
            drive: track.drive,
            punch: track.punch,
            safe: track.safe,
            low_cut: track.low_cut,
            high_cut: track.high_cut,
            pitch: track.pitch,
            glide: track.glide,
            gate_length: track.gate_length,
            velocity_delta: track.velocity_delta,
            inserts: track.inserts,
            sample,
        };
        let patch = Patch {
            mods: track.mods.to_vec(),
            accumulation: track.accumulation,
            arpeggio: track.arpeggio,
            macros: track.macros.to_vec(),
            generators: Vec::new(),
        };
        Self {
            format: PRESET_FORMAT,
            sound,
            patch,
        }
    }

    /// Make a track's sound and patch this one's, its sampler playing the bank entry `sample`,
    /// leaving its place in the song and its phrases alone. The settings of the other voices it
    /// remembers are kept, as switching voice keeps them. The slots all start in.
    pub fn apply(&self, track: &mut Track, sample: u8) {
        let sound = &self.sound;
        let machine = sound.machine;
        track.machine = machine;
        track.settings.retain(|id, _| !holds(machine, *id));
        track.fine.retain(|id, _| !holds(machine, *id));
        track.cutoff = sound.cutoff;
        track.resonance = sound.resonance;
        track.filter = sound.filter;
        track.filter_mode = sound.filter_mode;
        track.drive = sound.drive;
        track.punch = sound.punch;
        track.safe = sound.safe;
        track.low_cut = sound.low_cut;
        track.high_cut = sound.high_cut;
        track.pitch = sound.pitch;
        track.glide = sound.glide;
        track.gate_length = sound.gate_length;
        track.velocity_delta = sound.velocity_delta;
        track.inserts = sound.inserts;
        track.bypassed = [false; INSERTS];
        for (id, value) in &sound.settings {
            if holds(machine, *id) && *id != SettingId::SAMPLE {
                track.set_setting(*id, *value);
            }
        }
        if machine == Machine::Sampler {
            track.set_setting(SettingId::SAMPLE, sample);
        }
        // After the coarse values, as a fine part at a setting's top isn't kept.
        for (id, fine) in &sound.fine {
            if holds(machine, *id) {
                track.set_fine(*id, *fine);
            }
        }
        let patch = &self.patch;
        track.mods = std::array::from_fn(|slot| {
            patch
                .mods
                .get(slot)
                .cloned()
                .unwrap_or_else(|| default_mod(slot))
        });
        track.accumulation = patch.accumulation;
        track.arpeggio = patch.arpeggio;
        track.macros = std::array::from_fn(|at| patch.macros.get(at).cloned().unwrap_or_default());
    }

    /// What it is, for a list: its machine, then what's in its slots and how many macros hold
    /// something, as `Acid · Distort Delay · 3 macros`.
    pub fn summary(&self) -> String {
        let mut parts = vec![self.sound.machine.name().to_owned()];
        let effects: Vec<&str> = self
            .sound
            .inserts
            .iter()
            .filter(|insert| **insert != Insert::Empty)
            .map(|insert| insert.name())
            .collect();
        if !effects.is_empty() {
            parts.push(effects.join(" "));
        }
        let macros = self.patch.macros.iter().filter(|m| !m.is_empty()).count();
        match macros {
            0 => {}
            1 => parts.push("1 macro".to_owned()),
            n => parts.push(format!("{n} macros")),
        }
        parts.join(" · ")
    }

    /// Check what it holds fits a track, beyond what reading it checks.
    fn check(&self) -> Result<(), String> {
        if self.patch.mods.len() > MOD_SLOTS {
            return Err(format!(
                "it has {} mod slots, and a track has {MOD_SLOTS}",
                self.patch.mods.len()
            ));
        }
        if self.patch.macros.len() > MACROS {
            return Err(format!(
                "it has {} macros, and a track has {MACROS}",
                self.patch.macros.len()
            ));
        }
        Ok(())
    }
}

impl EffectPreset {
    /// One of a track's slots, with the fine parts of its effect's settings, or none if it's
    /// empty.
    pub fn of(track: &Track, slot: usize) -> Option<Self> {
        let effect = *track.inserts.get(slot)?;
        let kind = Effect::of(effect)?;
        let fine = track
            .fine
            .iter()
            .filter_map(|(id, fine)| match id.owner() {
                (Owner::Slot { slot: at, effect }, _) if at == slot && effect == kind => {
                    Some((id.entry().key.to_owned(), *fine))
                }
                _ => None,
            })
            .collect();
        Some(Self {
            format: PRESET_FORMAT,
            effect,
            fine,
        })
    }

    /// The fine parts of its settings as they'd be in a slot, or the first key its effect doesn't
    /// have.
    pub fn fine_in(&self, slot: usize) -> Result<BTreeMap<SettingId, u8>, String> {
        let Some(effect) = Effect::of(self.effect) else {
            return Ok(BTreeMap::new());
        };
        self.fine
            .iter()
            .map(|(key, fine)| {
                let at = effect
                    .params()
                    .iter()
                    .position(|param| param.key == key.as_str());
                at.and_then(|at| SettingId::slot(slot, effect, at))
                    .map(|id| (id, *fine))
                    .ok_or_else(|| format!("{} has no setting called {key}", self.effect.name()))
            })
            .collect()
    }

    /// What it is, for a list: the effect's name.
    pub fn summary(&self) -> String {
        self.effect.name().to_owned()
    }
}

/// Only the format, so a later one gives a clear message rather than a field error.
#[derive(Deserialize)]
struct FormatProbe {
    format: u32,
}

/// Read a preset's text, checking its format first.
fn parse<T: for<'de> Deserialize<'de>>(text: &str, path: &Path) -> Result<T, PresetError> {
    let parse_error = |error: &dyn std::fmt::Display| PresetError::Parse {
        path: path.display().to_string(),
        detail: error.to_string(),
    };
    let probe: FormatProbe = ron::from_str(text).map_err(|e| parse_error(&e))?;
    if probe.format > PRESET_FORMAT {
        return Err(PresetError::Format {
            path: path.display().to_string(),
            found: probe.format,
        });
    }
    ron::from_str(text).map_err(|e| parse_error(&e))
}

/// Read a sound preset.
pub fn load_sound(path: &Path) -> Result<SoundPreset, PresetError> {
    let text = fs::read_to_string(path).map_err(io_at(path))?;
    let preset: SoundPreset = parse(&text, path)?;
    preset.check().map_err(|detail| PresetError::Holds {
        path: path.display().to_string(),
        detail,
    })?;
    Ok(preset)
}

/// Read an effect preset.
pub fn load_effect(path: &Path) -> Result<EffectPreset, PresetError> {
    let text = fs::read_to_string(path).map_err(io_at(path))?;
    let preset: EffectPreset = parse(&text, path)?;
    preset.fine_in(0).map_err(|detail| PresetError::Holds {
        path: path.display().to_string(),
        detail,
    })?;
    Ok(preset)
}

/// Write a preset, making its folders. One that's there already is written over only if asked,
/// through a temporary file and a rename, as a song is.
pub fn save<T: Serialize>(path: &Path, preset: &T, overwrite: bool) -> Result<(), PresetError> {
    if path.exists() && !overwrite {
        return Err(PresetError::Taken(path.display().to_string()));
    }
    let folder = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(folder).map_err(io_at(folder))?;
    let text =
        ron::ser::to_string_pretty(preset, ron::ser::PrettyConfig::default()).map_err(|error| {
            PresetError::Parse {
                path: path.display().to_string(),
                detail: error.to_string(),
            }
        })?;
    let name = path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let temporary = folder.join(format!(".{name}.tmp"));
    fs::write(&temporary, text).map_err(io_at(&temporary))?;
    fs::rename(&temporary, path).map_err(io_at(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::macros::{MacroMap, MacroTarget};
    use crate::song::{Song, SynthVoice};

    fn folder(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("vt-preset-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        path
    }

    /// The acid voice's first setting that goes from `00` to `FF`.
    fn acid_setting() -> SettingId {
        (0..)
            .map_while(|at| SettingId::voice(SynthVoice::Acid, at))
            .find(|id| id.entry().kind.top() == 0xFF)
            .unwrap()
    }

    /// An acid track with a bit of everything: its own settings and another voice's, a fine
    /// part, its place in the song, an effect with a fine part, and a macro.
    fn acid() -> Track {
        let mut track = Song::new(1).tracks[0].clone();
        track.machine = Machine::Plaits(SynthVoice::VirtualAnalog);
        let other = SettingId::voice(SynthVoice::VirtualAnalog, 0).unwrap();
        track.set_setting(other, 0x11);
        track.machine = Machine::Plaits(SynthVoice::Acid);
        let own = acid_setting();
        track.set_setting(own, 0x42);
        track.cutoff = 0x40;
        track.set_fine(SettingId::CUT, 0x80);
        track.resonance = 0x90;
        track.level = 0x30;
        track.pan = 0x20;
        track.send_a = 0x50;
        track.name = "AB".to_owned();
        track.inserts[1] = Insert::RETURN_DELAY;
        let feedback = SettingId::slot(1, Effect::Delay, 1).unwrap();
        track.set_fine(feedback, 0x40);
        track.macros[0] = Macro {
            name: Some("Bite".to_owned()),
            value: 0x8000,
            maps: vec![MacroMap {
                target: MacroTarget::Setting(SettingId::CUT),
                min: 0x1000,
                max: 0xF000,
                // Bent, so a sound's mapping is seen to keep its curve (decision 314).
                curve: 40,
                s: -20,
            }],
        };
        track
    }

    #[test]
    fn a_sound_keeps_its_own_voice_and_leaves_its_place_in_the_song() {
        let preset = SoundPreset::of(&acid(), None);
        let own = acid_setting();
        assert_eq!(
            preset.sound.settings.keys().copied().collect::<Vec<_>>(),
            [own]
        );
        assert_eq!(preset.sound.fine.get(&SettingId::CUT), Some(&0x80));
        assert_eq!(preset.patch.macros[0].name.as_deref(), Some("Bite"));

        let mut other = Song::new(2).tracks[1].clone();
        other.level = 0x99;
        other.pan = 0x10;
        other.name = "ZZ".to_owned();
        other.bypassed[1] = true;
        let phrases = other.phrases.clone();
        preset.apply(&mut other, 0);
        assert_eq!(other.machine, Machine::Plaits(SynthVoice::Acid));
        assert_eq!(other.setting(own), 0x42);
        assert_eq!((other.cutoff, other.fine(SettingId::CUT)), (0x40, 0x80));
        assert_eq!(other.inserts[1], Insert::RETURN_DELAY);
        assert_eq!(
            other.fine(SettingId::slot(1, Effect::Delay, 1).unwrap()),
            0x40
        );
        assert_eq!(other.macros, acid().macros);
        assert_eq!(other.mods, acid().mods);
        assert!(!other.bypassed[1], "a loaded slot starts in");
        assert_eq!(
            (other.level, other.pan, other.name.as_str()),
            (0x99, 0x10, "ZZ")
        );
        assert_eq!(other.phrases, phrases);
    }

    #[test]
    fn a_sound_reads_back_as_it_was_written() {
        let root = folder("round");
        let path = preset_path(&root, PresetKind::Sound, "bass/Acid Bite").unwrap();
        let preset = SoundPreset::of(&acid(), None);
        save(&path, &preset, false).unwrap();
        assert_eq!(load_sound(&path).unwrap(), preset);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("format: 1"), "{text}");
        assert!(!text.contains("level"), "{text}");
        assert!(text.contains("curve: 40"), "the mapping's curve: {text}");
        let map = load_sound(&path).unwrap().patch.macros[0].maps[0];
        assert_eq!((map.curve, map.s), (40, -20));
        assert!(matches!(
            save(&path, &preset, false),
            Err(PresetError::Taken(_))
        ));
        save(&path, &preset, true).unwrap();
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_setting_left_out_takes_its_default() {
        let preset: SoundPreset =
            ron::from_str("(format: 1, sound: (machine: Plaits(Acid)))").unwrap();
        assert_eq!(preset.sound.cutoff, 0xFF);
        assert_eq!(preset.sound.pitch, 0x80);
        assert!(preset.patch.mods.is_empty());
        let mut track = acid();
        preset.apply(&mut track, 0);
        assert_eq!(track.mods, crate::modulation::default_mods());
        assert!(track.macros.iter().all(Macro::is_empty));
    }

    #[test]
    fn a_setting_this_build_doesnt_know_is_named() {
        let root = folder("unknown");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("x.ron");
        fs::write(&path, "(format: 1, sound: (machine: Sampler, wobble: 3))").unwrap();
        let error = load_sound(&path).unwrap_err().to_string();
        assert!(error.contains("wobble"), "{error}");
        fs::write(
            &path,
            r#"(format: 1, sound: (settings: {"acid.wobble": 3}))"#,
        )
        .unwrap();
        let error = load_sound(&path).unwrap_err().to_string();
        assert!(error.contains("acid.wobble"), "{error}");
        fs::write(&path, "(format: 2, sound: (), later: true)").unwrap();
        let error = load_sound(&path).unwrap_err().to_string();
        assert!(error.contains("format 2"), "{error}");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn an_effect_loads_into_any_slot() {
        let preset = EffectPreset::of(&acid(), 1).unwrap();
        assert_eq!(preset.effect, Insert::RETURN_DELAY);
        let key = Effect::Delay.params()[1].key;
        assert_eq!(preset.fine.get(key), Some(&0x40));
        let fine = preset.fine_in(3).unwrap();
        assert_eq!(
            fine.into_iter().collect::<Vec<_>>(),
            [(SettingId::slot(3, Effect::Delay, 1).unwrap(), 0x40)]
        );
        assert!(EffectPreset::of(&acid(), 0).is_none(), "an empty slot");
        let wrong = EffectPreset {
            fine: BTreeMap::from([("nothing".to_owned(), 1)]),
            ..preset
        };
        assert!(wrong.fine_in(0).unwrap_err().contains("nothing"));
    }

    #[test]
    fn a_name_is_plain_with_folders() {
        for good in ["Acid Bite", "bass/Acid Bite", "Pad (wide)", "v1.2_x-y"] {
            assert!(is_preset_name(good), "{good}");
        }
        for bad in [
            "", "/abs", "a//b", "../up", ".hidden", "a/", "c:d", "a\\b", "x*",
        ] {
            assert!(!is_preset_name(bad), "{bad}");
        }
        assert!(!is_preset_name(&"x".repeat(MAX_PRESET_NAME + 1)));
    }
}
