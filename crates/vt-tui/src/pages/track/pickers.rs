//! `K` and `i` on a setting that's a choice: a list of what it can be. Every setting the page
//! describes lists its choices one way, [`choice_picker`] from its view's; only the lists of
//! things that aren't a value's choices keep their own: a sampler's sample, slice and tune, the
//! groove a track can follow, a macro's patches, and a mod slot's type, rate, fade and reset.

use ratatui::style::Style;
use vt_model::accumulator::MOST_DEGREES;
use vt_model::arpeggio::MOST_OCTAVES;
use vt_model::lfo::{Fade, Lfo, note_rates};
use vt_model::modulation::{MOD_SLOTS, ModField, ModKind, ModSlot};
use vt_model::preset::PresetKind;
use vt_model::settings::SettingId;
use vt_model::song::{Machine, Role, Track};
use vt_model::steps::{StepRate, Steps};

use crate::app::App;
use crate::overlays::command::parse::{Line, PatchLine, PresetLine};
use crate::overlays::picker::{Item, Pick, Picker};
use crate::widgets::glyphs;
use crate::widgets::param::edit::{self, ChoiceList};
use crate::widgets::theme;

use super::controls::no_modes;
use super::editing::toggle_control;
use super::view::view;
use super::{AccumulatorField, ArpField, Control};

/// The titles of each slot's lists of types, as a list's title is fixed text.
const TYPE_TITLES: [&str; MOD_SLOTS] = [
    "Mod 1 type",
    "Mod 2 type",
    "Mod 3 type",
    "Mod 4 type",
    "Mod 5 type",
    "Mod 6 type",
];

/// A list of what a machine or a slot can be, with loading and saving a preset of it after them,
/// as Ableton's hot-swap is on a device's title (decision 310).
/// The machines' list draws each with its icon, so these have the presets folder's, which also
/// keeps a search for a machine, as `sa` for `Sampler`, from finding them first.
fn with_presets(mut items: Vec<Item>, kind: PresetKind) -> Vec<Item> {
    if kind == PresetKind::Patch {
        return with_patches(items);
    }
    let (what, icon) = match kind {
        PresetKind::Sound => ("sound preset", format!("{} ", glyphs::FOLDER)),
        _ => ("effect preset", String::new()),
    };
    let (load, into) = match kind {
        PresetKind::Sound => ("a", "keeping the track's phrases, level and sends"),
        _ => ("an", "into this slot"),
    };
    items.push(
        Item::new(
            format!("{icon}Load {load} {what}"),
            Pick::Line(Line::Preset(PresetLine::Browse)),
        )
        .meaning(format!(":preset, {into}"))
        .category(PRESETS),
    );
    items.push(
        Item::new(
            format!("{icon}Save as {load} {what}"),
            Pick::Prompt("preset save ".to_owned()),
        )
        .meaning(":preset save, then its name")
        .category(PRESETS),
    );
    items
}

/// A list on the Mod page or the macros, with adding a patch and saving the track's as one after
/// it (decision 311).
pub(crate) fn with_patches(mut items: Vec<Item>) -> Vec<Item> {
    items.push(
        Item::new(
            "Add a patch",
            Pick::Line(Line::Patch(PatchLine::Browse { over: false })),
        )
        .meaning(":patch, into free mod slots and macros, keeping the sound")
        .category(PATCHES),
    );
    items.push(
        Item::new("Save as a patch", Pick::Prompt("patch save ".to_owned()))
            .meaning(":patch save, the macros and what they move, then its name")
            .category(PATCHES),
    );
    items
}

/// The heading the patch items are listed under.
const PATCHES: &str = "Patches";

/// The heading the preset items are listed under.
const PRESETS: &str = "Presets";

/// The picker for a setting's choices, as its view lists them, opened on the one it has, each
/// item setting the `track`th track's setting to its place in the list. The machines draw with
/// their glyphs and the roles in their colours, and the machines and an effect slot's are
/// followed by loading and saving a preset.
pub(crate) fn choice_picker(track: usize, control: Control, list: ChoiceList) -> Picker {
    let items = list.items.into_iter().enumerate().map(|(at, choice)| {
        let pick = Pick::Param {
            track,
            control,
            value: at as i32,
        };
        let item = Item::new(choice.name, pick)
            .meaning(choice.meaning)
            .category(choice.group);
        match control {
            Control::Machine => Item {
                label: format!("{} {}", glyphs::machine(Machine::ALL[at]), item.label),
                ..item
            },
            Control::Role => {
                let colour = theme::role(Role::ALL[at]);
                item.styled(colour.map_or_else(Style::new, |colour| Style::new().fg(colour)))
            }
            _ => item,
        }
    });
    let items = items.collect();
    let items = match control {
        Control::Machine => with_presets(items, PresetKind::Sound),
        Control::Insert(_) => with_presets(items, PresetKind::Effect),
        _ => items,
    };
    Picker::new(list.title, items).opened_on(list.at)
}

/// What the status line says for `K` on a setting that isn't a list.
fn no_list(control: Control, track: &Track) -> String {
    match control {
        Control::InsertParam(..) => "This is a value: = and - change it, and i types it".to_owned(),
        Control::Accumulator(AccumulatorField::Up | AccumulatorField::Down) => format!(
            "A limit is a number of degrees from 1 to {MOST_DEGREES}: i types it, = and - move it"
        ),
        Control::Arpeggio(ArpField::Octaves) => {
            format!("Octaves is a number from 1 to {MOST_OCTAVES}: i types it, = and - move it")
        }
        Control::FilterMode => no_modes(track.filter),
        Control::Mod(..) => {
            "K chooses from a list on Type, Mode, Loop, Polarity, Dest, Shape, Rate, Fade and Reset"
                .to_owned()
        }
        _ => {
            "K chooses from a list on Machine, Voice, Patch, File, Type, Mode, Groove, Speed, Dest, Shape, Rate and Reset"
                .to_owned()
        }
    }
}

impl App {
    pub(super) fn open_control_picker(&mut self) {
        let control = self.track_control();
        let page = self.services.page_track();
        match control {
            // Something that's only on or off switches, as there's nothing to choose between.
            Control::Mute | Control::Solo | Control::Limiter | Control::Safe => {
                toggle_control(&mut self.services, control);
            }
            Control::Sample => self.open_bank_picker(),
            Control::Tune | Control::Cents => self.open_tuning_picker(control),
            _ if control.setting_on(&page) == Some(SettingId::SLICE) => self.open_slice_picker(),
            // A track can follow the project's, which isn't one of the grooves.
            Control::Groove => self.open_groove_picker(true),
            // A macro is the patch's knob, so its list is adding a patch and saving one.
            Control::Macro(_) => {
                self.show_picker(Picker::new("Patches", with_patches(Vec::new())));
            }
            Control::Mod(
                slot,
                field @ (ModField::Type | ModField::Rate | ModField::Fade | ModField::Reset),
            ) => self.open_mod_picker(slot, field),
            // The Gen page's lists are its own (`generators`).
            Control::Gen { .. } | Control::NewGenerator(_) => {
                self.services.status = no_list(control, &page);
            }
            _ => {
                let shown = view(control, &page, self.services.song(), false);
                match shown.as_ref().and_then(edit::choices) {
                    Some(list) => {
                        let picker = choice_picker(self.services.cursor.track, control, list);
                        self.show_picker(picker);
                    }
                    None => self.services.status = no_list(control, &page),
                }
            }
        }
    }

    /// List what a mod slot's setting can be that isn't one of its view's choices, with the
    /// current one highlighted: its type, which changes what the device is, a rate or a fade,
    /// which aren't one number, and an envelope's reset, on first.
    fn open_mod_picker(&mut self, slot: usize, field: ModField) {
        let track = self.services.cursor.track;
        let modulator = self.services.page_track().mods[slot];
        let pick = |modulator| Pick::Mod {
            track,
            slot,
            modulator,
        };
        let (lfo, steps) = (modulator.lfo, modulator.steps);
        let (title, items): (&'static str, Vec<Item>) = match field {
            ModField::Type => {
                let kinds = ModKind::ALL.map(|kind| {
                    Item::new(kind.name(), pick(ModSlot { kind, ..modulator }))
                        .meaning(kind.meaning())
                });
                (TYPE_TITLES[slot], with_patches(kinds.to_vec()))
            }
            ModField::Rate if modulator.kind == ModKind::Steps => {
                let rates = StepRate::ALL.map(|rate| {
                    let steps = Steps { rate, ..steps };
                    Item::new(rate.to_string(), pick(ModSlot { steps, ..modulator }))
                });
                ("Steps rate", rates.to_vec())
            }
            ModField::Reset => {
                let choices = [true, false].map(|reset| {
                    let (name, meaning) = if reset {
                        ("on", "Each note starts it from zero")
                    } else {
                        ("off", "Each note goes on from where it is")
                    };
                    let mut chosen = modulator;
                    chosen.set_field(field, i32::from(reset));
                    Item::new(name, pick(chosen)).meaning(meaning)
                });
                ("Envelope reset", choices.to_vec())
            }
            ModField::Rate => {
                let rates = note_rates()
                    .into_iter()
                    .map(|rate| {
                        let lfo = Lfo { rate, ..lfo };
                        Item::new(rate.to_string(), pick(ModSlot { lfo, ..modulator }))
                    })
                    .collect();
                ("LFO rate. i types any other", rates)
            }
            _ => {
                let fades = Fade::ALL.map(|fade| {
                    let lfo = Lfo { fade, ..lfo };
                    let meaning = match fade {
                        Fade::Off => "the whole wave from each note",
                        Fade::In(_) => "in from nothing after each note",
                        Fade::Out(_) => "out to nothing after each note",
                    };
                    Item::new(fade.to_string(), pick(ModSlot { lfo, ..modulator })).meaning(meaning)
                });
                ("LFO fade", fades.to_vec())
            }
        };
        let position = items.iter().position(|item| item.pick == pick(modulator));
        self.show_picker(Picker::new(title, items).opened_on(position));
    }
}
