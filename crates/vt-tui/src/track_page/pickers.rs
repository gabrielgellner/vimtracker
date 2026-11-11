//! `K` and `i` on a setting that's a choice: a list of what it can be.

use vt_model::accumulator::{Accumulation, Adds, AtLimit, MOST_DEGREES, Moves, Reset};
use vt_model::arpeggio::{ArpMode, ArpRate, Arpeggio, MOST_OCTAVES, retrig_meaning};
use vt_model::envelope::Envelope;
use vt_model::insert::Insert;
use vt_model::lfo::{Lfo, Shape, note_rates};
use vt_model::settings::{Kind, SettingId};
use vt_model::song::{FilterType, Machine};
use vt_model::speed::Speed;

use crate::app::App;
use crate::glyphs;
use crate::input::picker::{Item, Pick, Picker};

use super::controls::no_modes;
use super::{AccumulatorField, ArpField, Control, EnvelopeField, LfoField};

impl App {
    pub(super) fn open_control_picker(&mut self) {
        let control = self.track_control();
        if matches!(control, Control::Mute | Control::Solo) {
            self.toggle_control(control);
            return;
        }
        if self.track_control() == Control::Sample {
            self.open_bank_picker();
            return;
        }
        if self.track_control().setting_on(&self.page_track()) == Some(SettingId::SLICE) {
            self.open_slice_picker();
            return;
        }
        if let Control::Insert(slot) = control {
            self.open_insert_picker(slot);
            return;
        }
        if let Control::InsertParam(slot, param) = control {
            self.open_insert_param_picker(slot, param);
            return;
        }
        if self.track_control() == Control::FilterType {
            self.open_filter_picker();
            return;
        }
        if self.track_control() == Control::FilterMode {
            self.open_filter_mode_picker();
            return;
        }
        if self.track_control() == Control::Speed {
            self.open_speed_picker();
            return;
        }
        if self.track_control() == Control::Machine {
            self.open_machine_picker();
            return;
        }
        if self.track_control() == Control::Groove {
            self.open_groove_picker(true);
            return;
        }
        if let Some(id) = self.track_control().setting_on(&self.page_track())
            && let Kind::Stepped { choices, .. } = id.entry().kind
        {
            self.open_setting_picker(id, choices);
            return;
        }
        if let Control::Envelope(slot, field) = self.track_control() {
            self.open_envelope_picker(slot, field);
            return;
        }
        if let Control::Accumulator(field) = self.track_control() {
            self.open_accumulator_picker(field);
            return;
        }
        if let Control::Arpeggio(field) = self.track_control() {
            self.open_arpeggio_picker(field);
            return;
        }
        let Control::Lfo(slot, field) = self.track_control() else {
            self.status =
                "K chooses from a list on Machine, Voice, Patch, File, Type, Groove, Speed, Dest, Shape, Rate, Retrig and Reset"
                    .to_owned();
            return;
        };
        let track = self.cursor.track;
        let page = self.page_track();
        let lfo = page.lfos[slot];
        let pick = |lfo| Pick::Lfo { track, slot, lfo };
        let (title, items): (&'static str, Vec<Item>) = match field {
            LfoField::Destination => {
                let off = Item::new(
                    "---",
                    pick(Lfo {
                        destination: None,
                        ..lfo
                    }),
                )
                .meaning("Off");
                let on = SettingId::for_lfo(&page, slot)
                    .into_iter()
                    .map(|destination| {
                        Item::new(
                            destination.name_on(&page),
                            pick(Lfo {
                                destination: Some(destination),
                                ..lfo
                            }),
                        )
                        .meaning(destination.meaning())
                        .category(destination.group())
                    });
                ("LFO destination", std::iter::once(off).chain(on).collect())
            }
            LfoField::Shape => {
                let shapes =
                    Shape::ALL.map(|shape| Item::new(shape.name(), pick(Lfo { shape, ..lfo })));
                ("LFO shape", shapes.to_vec())
            }
            LfoField::Rate => {
                let rates = note_rates()
                    .into_iter()
                    .map(|rate| Item::new(rate.to_string(), pick(Lfo { rate, ..lfo })))
                    .collect();
                ("LFO rate. i types any other", rates)
            }
            LfoField::Retrig => {
                let choices = [false, true].map(|retrig| {
                    let name = if retrig { "on" } else { "off" };
                    Item::new(name, pick(Lfo { retrig, ..lfo }))
                });
                ("LFO retrig", choices.to_vec())
            }
            LfoField::Depth | LfoField::Phase => {
                self.status =
                    "K chooses from a list on Type, Dest, Shape, Rate, Retrig and Reset".to_owned();
                return;
            }
        };
        let position = items.iter().position(|item| item.pick == pick(lfo));
        self.show_picker(Picker::new(title, items).opened_on(position));
    }

    /// List the filter types, with what each does and the current one highlighted.
    /// List the effects a slot can hold, with the current one highlighted.
    fn open_insert_picker(&mut self, slot: usize) {
        let track = self.cursor.track;
        let current = self.page_track().inserts[slot];
        let items = Insert::ALL
            .map(|insert| {
                Item::new(
                    insert.name(),
                    Pick::Insert {
                        track,
                        slot,
                        // Choosing what's already there keeps its settings.
                        insert: if insert.name() == current.name() {
                            current
                        } else {
                            insert
                        },
                    },
                )
                .meaning(insert.meaning())
            })
            .to_vec();
        let position = Insert::ALL
            .iter()
            .position(|insert| insert.name() == current.name());
        self.show_picker(Picker::new("Effect", items).opened_on(position));
    }

    /// List the choices of a slot's setting, with the current one highlighted.
    fn open_insert_param_picker(&mut self, slot: usize, param: usize) {
        let track = self.cursor.track;
        let current = self.page_track().inserts[slot];
        let Some(found) = current
            .params()
            .get(param)
            .filter(|p| !p.choices.is_empty())
        else {
            self.status = "This is a value: = and - change it, and i types it".to_owned();
            return;
        };
        let items = found
            .choices
            .iter()
            .enumerate()
            .map(|(index, choice)| {
                Item::new(
                    *choice,
                    Pick::Insert {
                        track,
                        slot,
                        insert: current.with_value(param, index as u8),
                    },
                )
            })
            .collect();
        let position = usize::from(current.value(param));
        self.show_picker(Picker::new(found.name, items).opened_on(Some(position)));
    }

    fn open_filter_picker(&mut self) {
        let track = self.cursor.track;
        let current = self.page_track().filter;
        let items = FilterType::ALL
            .map(|filter| {
                Item::new(filter.name(), Pick::FilterType { track, filter })
                    .meaning(filter.meaning())
            })
            .to_vec();
        let position = FilterType::ALL.iter().position(|filter| *filter == current);
        self.show_picker(Picker::new("Filter type", items).opened_on(position));
    }

    /// List what one of the accumulator's settings can be, with what each does and the one it has
    /// highlighted. Its limits are numbers, typed or moved instead.
    fn open_accumulator_picker(&mut self, field: AccumulatorField) {
        let track = self.cursor.track;
        let now = self.page_track().accumulation;
        let pick = |accumulation| Pick::Accumulation {
            track,
            accumulation,
        };
        let (title, items): (&str, Vec<Item>) = match field {
            AccumulatorField::Up | AccumulatorField::Down => {
                self.status = format!(
                    "A limit is a number of degrees from 1 to {MOST_DEGREES}: i types it, = and - move it"
                );
                return;
            }
            AccumulatorField::Moves => (
                "Accumulator moves",
                Moves::ALL
                    .map(|moves| {
                        Item::new(moves.name(), pick(Accumulation { moves, ..now }))
                            .meaning(moves.meaning())
                    })
                    .to_vec(),
            ),
            AccumulatorField::AtLimit => (
                "Accumulator at its limit",
                AtLimit::ALL
                    .map(|at_limit| {
                        Item::new(at_limit.name(), pick(Accumulation { at_limit, ..now }))
                            .meaning(at_limit.meaning())
                    })
                    .to_vec(),
            ),
            AccumulatorField::Adds => (
                "Accumulator adds",
                Adds::ALL
                    .map(|adds| {
                        Item::new(adds.name(), pick(Accumulation { adds, ..now }))
                            .meaning(adds.meaning())
                    })
                    .to_vec(),
            ),
            AccumulatorField::Reset => (
                "Accumulator starts again",
                Reset::all()
                    .map(|reset| {
                        Item::new(reset.name(), pick(Accumulation { reset, ..now }))
                            .meaning(reset.meaning())
                    })
                    .collect(),
            ),
        };
        let position = items.iter().position(|item| item.pick == pick(now));
        self.show_picker(Picker::new(title, items).opened_on(position));
    }

    /// List what one of the arpeggio's settings can be, with what each does and the one it has
    /// highlighted. Its octaves are a number, typed or moved instead.
    fn open_arpeggio_picker(&mut self, field: ArpField) {
        let track = self.cursor.track;
        let now = self.page_track().arpeggio;
        let pick = |arpeggio| Pick::Arpeggio { track, arpeggio };
        let (title, items): (&str, Vec<Item>) = match field {
            ArpField::Octaves => {
                self.status = format!(
                    "Octaves is a number from 1 to {MOST_OCTAVES}: i types it, = and - move it"
                );
                return;
            }
            ArpField::Rate => (
                "Arp rate",
                ArpRate::ALL
                    .map(|rate| {
                        Item::new(rate.name(), pick(Arpeggio { rate, ..now }))
                            .meaning(rate.meaning())
                    })
                    .to_vec(),
            ),
            ArpField::Mode => (
                "Arp mode",
                ArpMode::ALL
                    .map(|mode| {
                        Item::new(mode.name(), pick(Arpeggio { mode, ..now }))
                            .meaning(mode.meaning())
                    })
                    .to_vec(),
            ),
            ArpField::Retrig => (
                "Arp retrig",
                [false, true]
                    .map(|retrig| {
                        let name = if retrig { "on" } else { "off" };
                        Item::new(name, pick(Arpeggio { retrig, ..now }))
                            .meaning(retrig_meaning(retrig))
                    })
                    .to_vec(),
            ),
        };
        let position = items.iter().position(|item| item.pick == pick(now));
        self.show_picker(Picker::new(title, items).opened_on(position));
    }

    /// List the speeds a track's steps can go at, with the one it plays highlighted.
    fn open_speed_picker(&mut self) {
        let track = self.cursor.track;
        let current = self.page_track().speed;
        let items = Speed::ALL
            .map(|speed| {
                Item::new(speed.name(), Pick::Speed { track, speed }).meaning(speed.meaning())
            })
            .to_vec();
        let position = Speed::ALL.iter().position(|speed| *speed == current);
        self.show_picker(Picker::new("Speed", items).opened_on(position));
    }

    /// List the modes the track's filter type offers, with what each does and the one it plays
    /// highlighted.
    fn open_filter_mode_picker(&mut self) {
        let track = self.cursor.track;
        let page = self.page_track();
        let modes = page.filter.modes();
        if modes.is_empty() {
            self.status = no_modes(page.filter);
            return;
        }
        let items = modes
            .iter()
            .map(|mode| {
                Item::new(mode.name(), Pick::FilterMode { track, mode: *mode })
                    .meaning(mode.meaning())
            })
            .collect();
        let playing = page.filter.plays(page.filter_mode);
        let position = modes.iter().position(|mode| *mode == playing);
        self.show_picker(Picker::new("Filter mode", items).opened_on(position));
    }

    /// List the machines, each with its glyph, a sampler, a synth or a drum, and what it does, and
    /// the current one highlighted.
    fn open_machine_picker(&mut self) {
        let track = self.cursor.track;
        let current = self.page_track().machine;
        let items = Machine::ALL
            .map(|machine| {
                let label = format!("{} {}", glyphs::machine(machine), machine.name());
                Item::new(label, Pick::Machine { track, machine }).meaning(machine.meaning())
            })
            .to_vec();
        let position = Machine::ALL.iter().position(|machine| *machine == current);
        self.show_picker(Picker::new("Machine", items).opened_on(position));
    }

    /// List what a machine's setting can be, such as 6-op's patches, with the current one
    /// highlighted.
    fn open_setting_picker(&mut self, setting: SettingId, choices: &'static [&'static str]) {
        let track = self.cursor.track;
        let current = self.page_track().setting(setting);
        let items = (0..choices.len().min(0x100))
            .map(|at| {
                Item::new(
                    format!("{at:02X}  {}", choices[at]),
                    Pick::Setting {
                        track,
                        setting,
                        value: at as u8,
                    },
                )
            })
            .collect();
        self.show_picker(Picker::new(setting.name(), items).opened_on(Some(usize::from(current))));
    }

    /// List what an envelope's setting can be, with the current one highlighted.
    fn open_envelope_picker(&mut self, slot: usize, field: EnvelopeField) {
        let track = self.cursor.track;
        let page = self.page_track();
        let envelope = self.page_track().envelopes[slot];
        let pick = |envelope| Pick::Envelope {
            track,
            slot,
            envelope,
        };
        let (title, items): (&'static str, Vec<Item>) = match field {
            EnvelopeField::Destination => {
                let off = Item::new(
                    "---",
                    pick(Envelope {
                        destination: None,
                        ..envelope
                    }),
                )
                .meaning("Off");
                let on = SettingId::for_envelope(&page)
                    .into_iter()
                    .map(|destination| {
                        Item::new(
                            destination.name_on(&page),
                            pick(Envelope {
                                destination: Some(destination),
                                ..envelope
                            }),
                        )
                        .meaning(destination.meaning())
                        .category(destination.group())
                    });
                (
                    "Envelope destination",
                    std::iter::once(off).chain(on).collect(),
                )
            }
            EnvelopeField::Reset => {
                let choices = [true, false].map(|reset| {
                    let (name, meaning) = if reset {
                        ("on", "Each note starts it from zero")
                    } else {
                        ("off", "Each note goes on from where it is")
                    };
                    Item::new(name, pick(Envelope { reset, ..envelope })).meaning(meaning)
                });
                ("Envelope reset", choices.to_vec())
            }
            _ => {
                self.status = "K chooses from a list on Dest and Reset".to_owned();
                return;
            }
        };
        let position = items.iter().position(|item| item.pick == pick(envelope));
        self.show_picker(Picker::new(title, items).opened_on(position));
    }
}
