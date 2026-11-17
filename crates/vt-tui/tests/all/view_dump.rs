//! Every setting's [`ParamView`] written out, on tracks that between them have every machine,
//! filter, effect, mod type, fill and column: what each cell shows, how it steps, what `K` lists,
//! how `i` reads it and what `set` makes of a value. It's kept in `tests/golden/views.txt` and
//! any difference fails, so a change to how settings are described (#873) can be held against it.
//! `VT_BLESS=1` writes it again after a change that's meant, which then shows in the diff.
//!
//! A view is written once and named by its number wherever it comes again, and so is a list of
//! choices, as most are the same on many tracks.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

use vt_model::action::{Action, edit};
use vt_model::constants::FX_SLOTS;
use vt_model::generator::Span;
use vt_model::insert::{INSERTS, Insert};
use vt_model::lane::LaneMove;
use vt_model::macros::{MacroMap, MacroTarget};
use vt_model::modulation::{MOD_SLOTS, ModField, ModKind};
use vt_model::param::Typing;
use vt_model::scale::{Scale, ScaleKind};
use vt_model::selection::StepRange;
use vt_model::settings::SettingId;
use vt_model::shape::{Context, FillKind, GenParam, Shape, ShapeColumn, Source, is_empty};
use vt_model::song::{FilterType, Machine, Phrase, Song};
use vt_model::step::{FxCommand, Step};
use vt_tui::controls::{Control, LaneField, Page, ParamView, set, view};

const C_MINOR: Scale = Scale {
    on: true,
    root: 0,
    kind: ScaleKind::Minor,
};

/// The dump as it's built: a line for each control seen, and each different view and list of
/// choices once, by the number it was first given.
#[derive(Default)]
struct Dump {
    lines: Vec<String>,
    views: Vec<String>,
    view_numbers: HashMap<String, usize>,
    choices: Vec<String>,
    choice_numbers: HashMap<String, usize>,
    /// The settings whose view says nothing of what they do, by control and name, once each.
    silent: Vec<String>,
}

/// The number of a text among those seen, a new one for one not seen before.
fn number_of(text: String, seen: &mut Vec<String>, numbers: &mut HashMap<String, usize>) -> usize {
    *numbers.entry(text.clone()).or_insert_with(|| {
        seen.push(text);
        seen.len() - 1
    })
}

impl Dump {
    /// A view's number, or `none` for a control with no view.
    fn seen(&mut self, found: Option<ParamView>) -> String {
        let Some(mut found) = found else {
            return "none".to_owned();
        };
        let choices = std::mem::take(&mut found.choices);
        let listed = match choices.is_empty() {
            true => String::new(),
            false => {
                let text = format!("{choices:?}");
                let number = number_of(text, &mut self.choices, &mut self.choice_numbers);
                format!(" choices c{number}")
            }
        };
        let text = format!("{found:?}{listed}");
        let number = number_of(text, &mut self.views, &mut self.view_numbers);
        format!("v{number}")
    }

    /// A control on the first track of a song: its view as it is, with the scale lock where that
    /// differs, and then set to each end of its bounds and to their middle, with a fine part
    /// where it has one, and seen again, which says what `set` made of each.
    fn control(&mut self, tag: &str, song: &Arc<Song>, control: Control) {
        let track = &song.tracks[0];
        let plain = view(control, track, song, false);
        let scaled = view(control, track, song, true);
        if let Some(silent) = plain.as_ref().filter(|plain| plain.meaning.is_empty()) {
            let named = format!("{control:?} {}", silent.name);
            if !self.silent.contains(&named) {
                self.silent.push(named);
            }
        }
        let mut line = format!("{tag} {control:?} = {}", self.seen(plain.clone()));
        if scaled != plain {
            let _ = write!(line, " scale {}", self.seen(scaled));
        }
        if let Some(plain) = plain {
            let (low, high) = plain.bounds;
            let fine = plain.fine.map(|_| 0x80);
            for value in [low, low + (high - low) / 2, high] {
                let made = set(control, 0, track, song, value, fine);
                let after = match made.as_ref().map(|action| edit(song, action)) {
                    None => "unset".to_owned(),
                    Some(Err(error)) => format!("refused ({error})"),
                    Some(Ok(next)) => self.seen(view(control, &next.tracks[0], &next, false)),
                };
                let _ = write!(line, " @{value}:{after}");
            }
        }
        self.lines.push(line);
    }

    /// Every control of a page's devices on the first track.
    fn page(&mut self, tag: &str, song: &Arc<Song>, page: Page) {
        for device in 0..page.devices(&song.tracks[0]).len() {
            self.device(tag, song, page, device);
        }
    }

    /// Every control of one of a page's devices on the first track, by its place in the rack.
    fn device(&mut self, tag: &str, song: &Arc<Song>, page: Page, device: usize) {
        let devices = page.devices(&song.tracks[0]);
        for control in devices[device].rows().flatten().flatten() {
            self.control(tag, song, control);
        }
    }

    fn text(&self) -> String {
        let mut text = self.lines.join("\n");
        for (name, each) in [("v", &self.views), ("c", &self.choices)] {
            for (number, one) in each.iter().enumerate() {
                let _ = write!(text, "\n{name}{number} {one}");
            }
        }
        text + "\n"
    }
}

/// A song of two sound tracks to change the first of.
fn song(change: impl FnOnce(&mut Song)) -> Arc<Song> {
    let mut song = Song::new(2);
    change(&mut song);
    Arc::new(song)
}

/// The Sound page on every machine and every filter, on a sampler with a sample, and on each
/// kind of fixed track.
fn sound(dump: &mut Dump) {
    for machine in Machine::ALL {
        let on = song(|song| song.tracks[0].machine = machine);
        dump.page(&format!("sound {machine:?}"), &on, Page::Sound);
    }
    for filter in FilterType::ALL {
        let on = song(|song| song.tracks[0].filter = filter);
        dump.page(&format!("sound {filter:?}"), &on, Page::Sound);
    }
    let sampled = song(|song| {
        song.sample_on(0, "tone.wav");
    });
    dump.page("sound sampled", &sampled, Page::Sound);
    let plain = Song::new(1);
    for fixed in 1..plain.tracks.len() {
        let mut moved = plain.clone();
        moved.tracks.swap(0, fixed);
        let tag = format!("sound {:?}", moved.tracks[0].kind);
        for page in [Page::Sound, Page::Fx] {
            dump.page(&tag, &Arc::new(moved.clone()), page);
        }
    }
}

/// A slot of the FX page with every effect in it, in each of its types or modes: every setting
/// that's a list is put at each of its choices, and the whole slot seen there. The slots go
/// round, so each is seen with some effects.
fn effects(dump: &mut Dump) {
    for (at, insert) in Insert::ALL.into_iter().enumerate() {
        let slot = at % INSERTS;
        let on = |insert: Insert| song(|song| song.tracks[0].inserts[slot] = insert);
        dump.device(
            &format!("fx {}", insert.name()),
            &on(insert),
            Page::Fx,
            slot,
        );
        let params = insert.params();
        for (param, found) in params.iter().enumerate() {
            for choice in 0..found.choices().len() {
                let chosen = insert.with_value(param, choice as u8);
                if chosen == insert {
                    continue;
                }
                let tag = format!("fx {} {}={choice}", insert.name(), found.key);
                dump.device(&tag, &on(chosen), Page::Fx, slot);
            }
        }
    }
}

/// The Mod page as a new track has it, then each mod type in a slot, going to `Pitch`, in each of
/// its modes and, an LFO, each of its shapes, the slots going round.
fn mods(dump: &mut Dump) {
    dump.page("mod", &song(|_| {}), Page::Mod);
    for (at, kind) in ModKind::ALL.into_iter().enumerate() {
        let slot = (at + 1) % MOD_SLOTS;
        let of = |field: ModField, value: i32| {
            song(|song| {
                let modulator = &mut song.tracks[0].mods[slot];
                modulator.kind = kind;
                modulator.destination = Some(SettingId::PITCH);
                modulator.set_field(field, value);
            })
        };
        let plain = of(ModField::Mode, 0);
        for field in [ModField::Mode, ModField::Shape, ModField::Loop] {
            let Some((low, high)) = plain.tracks[0].mods[slot].bounds(field) else {
                continue;
            };
            for value in low..=high {
                let tag = format!("mod {kind:?} {field:?}={value}");
                dump.device(&tag, &of(field, value), Page::Mod, slot);
            }
        }
    }
}

/// The macros, one of them mapped and named, and each FX column's lane moved and at rest.
fn macros_and_lanes(dump: &mut Dump) {
    let mapped = song(|song| {
        let held = &mut song.tracks[0].macros[1];
        held.value = 0x4080;
        held.name = Some("Bright".to_owned());
        held.maps.push(MacroMap {
            target: MacroTarget::Setting(SettingId::CUT),
            min: 0x1000,
            max: 0xF000,
            curve: 0,
            s: 0,
            zone: Default::default(),
            member: None,
        });
    });
    dump.page("macros", &mapped, Page::Gen);
    for (tag, moved) in [
        ("lane at rest", LaneMove::default()),
        (
            "lane moved",
            LaneMove {
                length: 3,
                rate: 9,
                start: 2,
            },
        ),
    ] {
        let on = song(|song| song.tracks[0].lanes = [moved; FX_SLOTS]);
        for slot in 0..FX_SLOTS {
            for field in [LaneField::Length, LaneField::Rate, LaneField::Start] {
                dump.control(tag, &on, Control::Lane(slot, field));
            }
        }
    }
}

/// The columns a generator goes on: the notes with the scale off and on, and where there are
/// none to rework, each of the other cells, and an FX column locking a setting and one of an
/// event.
fn columns() -> Vec<(&'static str, ShapeColumn, bool)> {
    let note = |scale, writes| ShapeColumn::Note { scale, writes };
    let fx = |slot, command| ShapeColumn::Fx { slot, command };
    vec![
        ("note", note(Scale::default(), None), true),
        ("note in scale", note(C_MINOR, None), true),
        ("note written", note(Scale::default(), Some(0x60)), false),
        ("velocity", ShapeColumn::Velocity, true),
        ("nudge", ShapeColumn::Nudge { most: 11 }, true),
        ("length", ShapeColumn::Length, true),
        ("fx lock", fx(0, FxCommand::Set(SettingId::CUT)), false),
        ("fx event", fx(1, FxCommand::Del), false),
    ]
}

/// Every fill a column takes, live on a phrase's first sixteen rows, with every setting its
/// shape has there.
fn generators(dump: &mut Dump) {
    for (name, column, notes) in columns() {
        let base = song(|song| {
            let steps: Vec<Step> = (0..16)
                .map(|row| match notes && row % 2 == 0 {
                    true => "C-3 40".parse().unwrap(),
                    false => Step::default(),
                })
                .collect();
            for track in 0..2 {
                let phrase = Arc::new(Phrase::from_steps(steps.clone()));
                song.tracks[track].phrases.insert(0, phrase);
            }
            if let ShapeColumn::Note { scale, .. } = column {
                song.saved_scale = scale;
            }
        });
        for fill in column.fills() {
            let mut shape = Shape {
                kind: fill,
                ..Shape::default()
            };
            shape.follow.source = Source {
                track: 1,
                phrase: 0,
            };
            let tag = format!("gen {name} {fill:?}");
            let Some((shape, generates)) = generated(dump, &tag, &base, column, shape, &[]) else {
                continue;
            };
            // A setting that only some choice of a list brings, as a Euclidean `Where` brings
            // its hits and rotation, is seen under each choice that brings it.
            let shown = GenParam::of(&shape, column, generates);
            let context = Context {
                column,
                open: 16,
                generates,
                sources: &[],
                source: None,
            };
            for param in &shown {
                if *param == GenParam::Fill || param.spec().typing != Typing::Listed {
                    continue;
                }
                let count = param.choices(&context).map_or(0, |choices| choices.len());
                for at in 0..count {
                    let mut chosen = shape;
                    param.set_number(&mut chosen, &context, at as i32);
                    let tag = format!("{tag} {param:?}={at}");
                    generated(dump, &tag, &base, column, chosen, &shown);
                }
            }
        }
    }
    // Every fill is on some column above.
    let seen = |fill: FillKind| {
        columns()
            .iter()
            .any(|(_, column, _)| column.fills().contains(&fill))
    };
    assert!(FillKind::ALL.into_iter().all(seen));
}

/// A generator of `shape` made live on `base`'s first phrase, on its first sixteen rows, with
/// each of its settings seen that isn't among those `seen` already: its shape as it's live and
/// whether it generates on an empty column, or none, said why, where it isn't made.
fn generated(
    dump: &mut Dump,
    tag: &str,
    base: &Arc<Song>,
    column: ShapeColumn,
    shape: Shape,
    seen: &[GenParam],
) -> Option<(Shape, bool)> {
    let made = edit(
        base,
        &Action::Generate {
            track: 0,
            phrase: 0,
            column,
            span: Span::rows(StepRange { first: 0, last: 15 }),
            shape,
        },
    );
    let live = match made {
        Ok(live) => live,
        Err(error) => {
            dump.lines.push(format!("{tag} refused ({error})"));
            return None;
        }
    };
    let phrase = &live.tracks[0].phrases[&0];
    let Some(generator) = phrase.generators().first() else {
        dump.lines.push(format!("{tag} left none live"));
        return None;
    };
    let generates = generator
        .rows()
        .is_some_and(|rows| is_empty(&generator.under(phrase.steps()), rows, column));
    let shape = generator.shape;
    for param in GenParam::of(&shape, column, generates) {
        if seen.contains(&param) {
            continue;
        }
        let control = Control::Gen {
            phrase: 0,
            generator: 0,
            param,
        };
        dump.control(tag, &live, control);
    }
    Some((shape, generates))
}

/// Every setting on every page, as the dump goes through them.
fn dumped() -> Dump {
    let mut dump = Dump::default();
    sound(&mut dump);
    effects(&mut dump);
    mods(&mut dump);
    macros_and_lanes(&mut dump);
    generators(&mut dump);
    dump
}

/// Every setting the Track page's pages reach says what it does, for the line that describes the
/// one under the cursor (#969), so a new setting comes with its words.
#[test]
fn every_setting_on_the_pages_says_what_it_does() {
    let silent = dumped().silent;
    assert!(
        silent.is_empty(),
        "{} settings say nothing of what they do: {}",
        silent.len(),
        silent.join(", ")
    );
}

#[test]
fn every_settings_view_is_as_it_was() {
    let text = dumped().text();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/views.txt");
    if std::env::var_os("VT_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        return;
    }
    let saved = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("{} is missing: run with VT_BLESS=1", path.display()));
    if saved == text {
        return;
    }
    // The first lines that differ say where to look; the rest is in the diff after a bless.
    let differing: Vec<String> = saved
        .lines()
        .zip(text.lines())
        .enumerate()
        .filter(|(_, (was, is))| was != is)
        .take(5)
        .map(|(at, (was, is))| format!("line {}:\n  was {was}\n  is  {is}", at + 1))
        .collect();
    panic!(
        "the views moved, {} lines to {}: {}\nrun with VT_BLESS=1 if that was intended",
        saved.lines().count(),
        text.lines().count(),
        differing.join("\n")
    );
}
