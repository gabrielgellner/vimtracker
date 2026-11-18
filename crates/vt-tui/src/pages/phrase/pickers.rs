//! `K` on a phrase cell: the lists of what its gate, its length and its FX columns can hold, and
//! the orders the phrase's rows can play in, which `:order` lists. Each is built from the
//! [`Services`] and given back to be shown; what a choice does is the picker's, in `picking.rs`.

use vt_model::accumulator::{MOST_DEGREES, amount, amount_text, amount_value};
use vt_model::action::Action;
use vt_model::condition::Condition;
use vt_model::order::Order;
use vt_model::selection::Field;
use vt_model::settings::{SettingId, gate_percent};
use vt_model::song::FULL_GATE;
use vt_model::step::{FxCommand, GateKind};

use crate::overlays::command::parse::Line;
use crate::overlays::picker::{Item, Pick, Picker};
use crate::pages::Outcome;
use crate::pages::phrase::PhraseView;
use crate::services::{Services, walked_order};

/// The lengths `K` lists on a gate cell's length, as shares of its last pulse: from a click, through
/// a 303's half, to all of it.
const LENGTHS: [(u8, &str); 7] = [
    (0x08, "a click, for ticks and plucks"),
    (0x20, "very short, a stab"),
    (0x40, "short, staccato"),
    (0x80, "half, as a 303's gate closes"),
    (0xC0, "long, with a gap before the next"),
    (0xE6, "nearly legato, a breath before the next"),
    (0xFF, "the whole pulse, held until something ends it"),
];

impl PhraseView {
    /// The gate types, with what each does and the cell's current one highlighted.
    pub(crate) fn gate_picker(services: &Services) -> Picker {
        let current = services.current_gate().map(|gate| gate.kind);
        let empty = Item::new("---", Pick::Gate(None)).meaning("Empty, one pulse of SGL");
        let items = std::iter::once(empty)
            .chain(GateKind::ALL.iter().map(|kind| {
                Item::new(kind.name(), Pick::Gate(Some(*kind))).meaning(kind.meaning())
            }))
            .collect();
        let position = GateKind::ALL
            .iter()
            .position(|kind| Some(*kind) == current)
            .map_or(0, |position| position + 1);
        Picker::new("Gates", items).opened_on(Some(position))
    }

    /// Some useful lengths for the gate cell, from a click to all of the pulse, each with its
    /// share of the last pulse and what it's for, with the cell's own marked, or the nearest shown.
    pub(crate) fn length_picker(services: &Services) -> Picker {
        let track = services
            .song()
            .tracks
            .get(services.cursor.track)
            .map_or(FULL_GATE, |track| track.gate_length());
        let empty = Item::new("--", Pick::Length(None))
            .meaning(format!("The track's Gate, now {}%", gate_percent(track)));
        let items = std::iter::once(empty)
            .chain(LENGTHS.iter().map(|(length, meaning)| {
                Item::new(format!("{length:02X}"), Pick::Length(Some(*length)))
                    .meaning(format!("{}%, {meaning}", gate_percent(*length)))
            }))
            .collect();
        let current = services.current_gate().and_then(|gate| gate.length);
        let exact = match current {
            None => Some(0),
            Some(current) => LENGTHS
                .iter()
                .position(|(length, _)| *length == current)
                .map(|position| position + 1),
        };
        let nearest = current.and_then(|current| {
            LENGTHS
                .iter()
                .enumerate()
                .min_by_key(|(_, (length, _))| length.abs_diff(current))
                .map(|(position, _)| position + 1)
        });
        let picker = Picker::new("Lengths", items);
        match exact {
            Some(_) => picker.opened_on(exact),
            None => picker.revealing(nearest),
        }
    }

    /// The FX commands this track has, in categories as `Dest` is: the filter, output, the voice,
    /// each slot's effect, then the events. It opens on its search, so typing finds a label or a
    /// name at once, and `Esc` goes to the list. A query starts it searching for that.
    pub(crate) fn fx_picker(services: &Services, slot: usize, query: &str) -> Picker {
        let current = services.current_slot(slot).and_then(|s| s.command);
        let empty = Item::new(
            "---",
            Pick::FxCommand {
                slot,
                command: None,
            },
        )
        .meaning("Empty");
        let (commands, listed) =
            services.fx_command_items(services.cursor.track, |command| Pick::FxCommand {
                slot,
                command: Some(command),
            });
        let items = std::iter::once(empty).chain(listed).collect();
        let position = commands
            .iter()
            .position(|command| Some(*command) == current)
            .map_or(0, |position| position + 1);
        Picker::new("FX commands", items)
            .opened_on(Some(position))
            .searching(query)
    }

    /// The conditions a `CND` can hold, in Elektron's order and by Elektron's names, in groups,
    /// opening on the slot's own and on its search, as the FX commands' list does.
    pub(crate) fn condition_picker(services: &Services, slot: usize) -> Picker {
        let current = services
            .current_slot(slot)
            .filter(|s| s.command == Some(FxCommand::Cnd))
            .and_then(|s| Condition::from_value(s.coarse()));
        let items = Condition::all()
            .map(|condition| {
                Item::new(
                    condition.label(),
                    Pick::Condition {
                        slot,
                        value: condition.value(),
                    },
                )
                .meaning(format!(
                    "{:02X}  {}",
                    condition.value(),
                    condition.meaning()
                ))
                .category(condition.group())
            })
            .collect();
        let position = Condition::all().position(|condition| Some(condition) == current);
        Picker::new("Conditions", items)
            .opened_on(position)
            .searching("")
    }

    /// How far an `ACU` can move its note each time, up and then down, in degrees or semitones as
    /// the scale is on or off, opening on the slot's own, or on one up.
    pub(crate) fn amount_picker(services: &Services, slot: usize) -> Picker {
        let current = services
            .current_slot(slot)
            .filter(|s| s.command == Some(FxCommand::Acu))
            .and_then(|s| amount(s.coarse()))
            .filter(|amount| *amount != 0)
            .unwrap_or(1);
        let most = MOST_DEGREES as i8;
        let amounts: Vec<i8> = (1..=most).chain((1..=most).map(|down| -down)).collect();
        let items = amounts
            .iter()
            .map(|&amount| {
                let value = amount_value(amount);
                Item::new(format!("{amount:+}"), Pick::Amount { slot, value })
                    .meaning(format!(
                        "{value:02X}  {}",
                        capitalised(&amount_text(amount, services.scale_here().on))
                    ))
                    .category(if amount > 0 { "Up" } else { "Down" })
            })
            .collect();
        let position = amounts.iter().position(|amount| *amount == current);
        Picker::new("Accumulate", items)
            .opened_on(position)
            .searching("")
    }

    /// `:order`: the orders the cursor's phrase's rows can play in, on the one they play in.
    pub(crate) fn order_picker(services: &Services) -> Outcome {
        let Some(phrase) = services.phrase_here() else {
            return Outcome::Refused("This track has no phrase here");
        };
        // An order that takes numbers is listed with the phrase's own, if it plays in one.
        let playing = phrase.order();
        let orders = Order::ALL.map(|order| {
            if order.takes_numbers() && order.is_like(playing) {
                playing
            } else {
                order
            }
        });
        let at = orders.iter().position(|order| *order == playing);
        let items = orders
            .map(|order| {
                Item::new(order.to_string(), Pick::Line(Line::Order(Some(order))))
                    .meaning(order.meaning())
            })
            .to_vec();
        Outcome::Open(Box::new(
            Picker::new("This phrase's order", items).opened_on(at),
        ))
    }

    /// `K`: choose the field under the cursor from a list, in the columns that are one.
    pub(crate) fn field_picker(services: &Services) -> Outcome {
        let command = |slot| services.current_slot(slot).and_then(|s| s.command);
        let picker = match services.cursor.field {
            // A condition's value is one of a list, so `K` on it lists them.
            Field::FxValue(slot) if command(slot) == Some(FxCommand::Cnd) => {
                Self::condition_picker(services, slot)
            }
            // So is an amount, as a signed byte is hard to type.
            Field::FxValue(slot) if command(slot) == Some(FxCommand::Acu) => {
                Self::amount_picker(services, slot)
            }
            // And a sample lock's is one of the bank's.
            Field::FxValue(slot) if command(slot) == Some(FxCommand::Set(SettingId::SAMPLE)) => {
                Self::sample_lock_picker(services, slot)
            }
            // And a slice lock's is one of the slices of the sample the step plays.
            Field::FxValue(slot) if command(slot) == Some(FxCommand::Set(SettingId::SLICE)) => {
                Self::slice_lock_picker(services, slot)
            }
            Field::FxCommand(slot) | Field::FxValue(slot) | Field::FxFine(slot) => {
                Self::fx_picker(services, slot, "")
            }
            Field::Gate => Self::gate_picker(services),
            Field::GateLength => Self::length_picker(services),
            Field::Note | Field::Velocity | Field::Nudge => {
                return Outcome::Refused("K chooses on the gate and FX columns");
            }
        };
        Outcome::Open(Box::new(picker))
    }

    /// The sample a step's `SMP` locks, from the bank, on the slot's own.
    pub(crate) fn sample_lock_picker(services: &Services, slot: usize) -> Picker {
        let current = services
            .current_slot(slot)
            .filter(|cell| cell.command == Some(FxCommand::Set(SettingId::SAMPLE)))
            .map_or(0, |cell| cell.coarse());
        let items = services.bank_items(|number| Pick::SampleLock { slot, number });
        let at = services.bank_place(current);
        Picker::new("Sample lock", items).opened_on(Some(at))
    }

    /// The slice a step's `SLI` locks, from the slices of the sample the step plays: first by each
    /// letter naming one, as `'a  03  0.214 s`, which keeps to its slice as slices before it come
    /// and go, and then by number.
    pub(crate) fn slice_lock_picker(services: &Services, slot: usize) -> Picker {
        let current = services
            .current_slot(slot)
            .filter(|cell| cell.command == Some(FxCommand::Set(SettingId::SLICE)));
        let number = services.step_entry();
        let mut items = services.mark_items(number, |letter| Pick::SliceMarkLock { slot, letter });
        let named = items.len();
        items.extend(services.slice_items(number, |number| Pick::SliceLock { slot, number }));
        let at = match current.map(|cell| (cell.slice_mark(), cell.coarse())) {
            Some((Some(letter), _)) => items[..named]
                .iter()
                .position(|item| item.pick == Pick::SliceMarkLock { slot, letter }),
            Some((None, number)) => Some(named + usize::from(number).min(items.len() - named - 1)),
            None => Some(named),
        };
        Picker::new("Slice lock", items).opened_on(at)
    }
}

impl Services {
    /// The FX commands a track has, and an item for each choosing it as `pick` says, in categories
    /// as `Dest` is: the filter, output, the voice, each slot's effect, then the events.
    pub(crate) fn fx_command_items(
        &self,
        track: usize,
        pick: impl Fn(FxCommand) -> Pick,
    ) -> (Vec<FxCommand>, Vec<Item>) {
        let track = self.song().tracks.get(track);
        // Only what this track has, from its machine and its slots.
        let commands: Vec<FxCommand> =
            track.map_or_else(|| FxCommand::EVENTS.to_vec(), FxCommand::on_track);
        let items = commands
            .iter()
            .map(|command| {
                let (name, category) = match (command, track) {
                    (FxCommand::Set(id), Some(track)) => (id.name_on(track), id.group_on(track)),
                    (FxCommand::Set(id), None) => (command.meaning(), id.group()),
                    _ => (command.meaning(), "Events".to_owned()),
                };
                Item::new(command.name(), pick(*command))
                    .meaning(name)
                    .category(category)
            })
            .collect();
        (commands, items)
    }

    /// Play the cursor's phrase's rows in an order.
    pub(crate) fn set_order(&mut self, order: Order) {
        let Some(phrase) = self.phrase_id() else {
            self.status = "This track has no phrase here".to_owned();
            return;
        };
        let rows = self.phrase_here().map_or(0, |phrase| phrase.len());
        self.apply(&Action::SetOrder {
            track: self.cursor.track,
            phrase,
            order,
        });
        self.status = format!("This phrase plays {}: {}", order, order.describe(rows));
        // The track's `Phrase` settings may move it (decision 400).
        let heard = self
            .phrase_here()
            .and_then(|phrase| walked_order(phrase, self.walk(self.cursor.track)));
        if let Some(heard) = heard {
            self.status = format!(
                "This phrase's own order is {order}, and the track's Phrase settings play it as \
                 {heard}"
            );
        }
    }
}

/// Text with its first letter a capital, as a list's meanings start.
fn capitalised(text: &str) -> String {
    let mut letters = text.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    })
}
