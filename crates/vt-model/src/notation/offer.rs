//! The lists a grammar's offer is made of, for `K` and `Tab` in a field (decision 458): the scale's
//! notes round a note, its degrees named by their notes, a column's values named where `K` names
//! them on a row, and the groups a group's place can hold. Each grammar takes those it reads, so
//! a note means the same in every field.

use super::Offer;
use crate::accumulator::{MOST_DEGREES, amount_text, amount_value};
use crate::condition::Condition;
use crate::scale::Scale;
use crate::shape::{ShapeColumn, degree_count, degree_name, degree_of};
use crate::step::{FxCommand, MAX_NOTE, MIN_NOTE, Note};

/// The semitones in an octave.
const OCTAVE: u8 = 12;

fn offer(text: impl Into<String>, meaning: impl Into<String>) -> Offer {
    Offer {
        text: text.into(),
        meaning: meaning.into(),
    }
}

/// `1st`, `2nd`, `3rd`, `4th`.
fn ordinal(number: usize) -> String {
    let suffix = match (number % 10, number % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{number}{suffix}")
}

/// What a degree is, counted from 0: `the root`, or `the 3rd`.
fn degree_meaning(degree: usize) -> String {
    match degree {
        0 => "the root".to_owned(),
        _ => format!("the {}", ordinal(degree + 1)),
    }
}

/// The scale as a meaning names it, `C Minor`, or the semitones from its root while it's off.
fn scale_name(scale: Scale) -> String {
    if scale.on {
        format!("{} {}", scale.root_name(), scale.kind.name())
    } else {
        format!("semitones from {}", scale.root_name())
    }
}

/// The scale's notes round a note, from an octave below it to an octave above, each named by
/// its degree: `D#2  the 3rd of C Minor`. The twelve semitones while the scale is off.
pub fn notes_round(scale: Scale, note: u8) -> Vec<Offer> {
    let low = note.saturating_sub(OCTAVE).max(MIN_NOTE);
    let high = note.saturating_add(OCTAVE).min(MAX_NOTE);
    (low..=high)
        .filter(|near| !scale.on || scale.contains(*near))
        .map(|near| {
            let degree = degree_of(scale, near).unwrap_or(0);
            let mut meaning = format!("{} of {}", degree_meaning(degree), scale_name(scale));
            if near == note {
                meaning.push_str(", as it is");
            }
            offer(Note::On(near).to_string(), meaning)
        })
        .collect()
}

/// The scale's degrees, from `1` for the root, each named by its note: `5  G, the 5th`.
pub fn degrees(scale: Scale) -> Vec<Offer> {
    (0..degree_count(scale))
        .map(|degree| {
            offer(
                (degree + 1).to_string(),
                format!("{}, {}", degree_name(scale, degree), degree_meaning(degree)),
            )
        })
        .collect()
}

/// A column's values as a token shows them: on an FX column a condition's or an `ACU`'s by its
/// name, as `K` names them on a row, with its hex and what it does, and any other command's in
/// hex; a note by its name, a nudge with its sign, and the rest in hex.
pub fn values(column: ShapeColumn) -> Vec<Offer> {
    match column {
        ShapeColumn::Fx {
            command: FxCommand::Cnd,
            ..
        } => Condition::all()
            .map(|condition| {
                offer(
                    condition.label(),
                    format!("{:02X}  {}", condition.value(), condition.meaning()),
                )
            })
            .collect(),
        ShapeColumn::Fx {
            command: FxCommand::Acu,
            ..
        } => {
            let most = MOST_DEGREES as i8;
            (1..=most)
                .chain((1..=most).map(|down| -down))
                .map(|amount| {
                    offer(
                        format!("{amount:+}"),
                        format!(
                            "{:02X}  {}",
                            amount_value(amount),
                            amount_text(amount, true)
                        ),
                    )
                })
                .collect()
        }
        _ => (column.lowest()..=column.highest())
            .map(|value| offer(column.show(value), ""))
            .collect(),
    }
}

/// What a group's place can hold: the other kinds of group, and a rest, written `rest`, in place
/// of the group and all it holds.
pub fn groups(rest: &str) -> Vec<Offer> {
    vec![
        offer("[ ]", "a group: split its place"),
        offer("< >", "one of them each lap"),
        offer(rest, "a rest, in place of the group"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::ScaleKind;

    fn minor() -> Scale {
        Scale {
            on: true,
            kind: ScaleKind::Minor,
            ..Scale::default()
        }
    }

    fn texts(offers: &[Offer]) -> Vec<&str> {
        offers.iter().map(|offer| offer.text.as_str()).collect()
    }

    /// An octave of C minor either side of D#3, its own place said.
    #[test]
    fn a_notes_offer_is_the_scale_round_it_an_octave_each_way() {
        let d_sharp = 51;
        let offers = notes_round(minor(), d_sharp);
        assert_eq!(offers.len(), 15, "seven notes an octave, and D# either end");
        assert_eq!(offers.first().unwrap().text, Note::On(39).to_string());
        assert_eq!(offers.last().unwrap().text, Note::On(63).to_string());
        let own = offers
            .iter()
            .find(|offer| offer.text == Note::On(d_sharp).to_string())
            .unwrap();
        assert!(own.meaning.starts_with("the 3rd of C"), "{}", own.meaning);
        assert!(own.meaning.ends_with("as it is"));
        // With the scale off, every semitone.
        assert_eq!(notes_round(Scale::default(), d_sharp).len(), 25);
    }

    #[test]
    fn degrees_count_from_1_and_are_named_by_their_notes() {
        let offers = degrees(minor());
        assert_eq!(texts(&offers), ["1", "2", "3", "4", "5", "6", "7"]);
        assert_eq!(offers[0].meaning, "C, the root");
        assert_eq!(offers[4].meaning, "G, the 5th");
    }

    /// A condition is named as `K` names it on a row, and a plain command's values are hex.
    #[test]
    fn an_fx_columns_values_are_named_where_k_names_them_on_a_row() {
        let conditions = values(ShapeColumn::Fx {
            slot: 0,
            command: FxCommand::Cnd,
        });
        assert_eq!(conditions.len(), Condition::all().count());
        let first = Condition::all().next().unwrap();
        assert_eq!(conditions[0].text, first.label());
        assert!(conditions[0].meaning.contains(&first.meaning()));
        let amounts = values(ShapeColumn::Fx {
            slot: 0,
            command: FxCommand::Acu,
        });
        assert_eq!(amounts[0].text, "+1");
        assert_eq!(amounts.len(), 2 * usize::from(MOST_DEGREES));
        let velocities = values(ShapeColumn::Velocity);
        assert_eq!(velocities[0x40].text, "40");
    }

    #[test]
    fn a_groups_place_takes_another_group_or_a_rest() {
        assert_eq!(texts(&groups(".")), ["[ ]", "< >", "."]);
    }
}
