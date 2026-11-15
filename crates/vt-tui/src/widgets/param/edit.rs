//! Editing a setting from its view: where `=`, `-`, `+` and `_` take it, its fine part, what
//! `i`'s text sets it to, what `x` puts back and what `K` lists (the UI plan's §3.3, #739). These
//! read only the [`ParamView`], so they know no device and no track: the page writes what they
//! give back with `pages::track::set`, the one way every described setting is written.

use vt_model::step::Note;

use super::view::{Choice, Pace, ParamView, Reads, Steps};

/// How many small steps of a fine part a large one is.
const FINE_JUMP: i64 = 0x10;

/// What `K` lists for a setting: its title, its choices as they're labelled, and the one it has,
/// if it's one of them.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ChoiceList {
    pub title: &'static str,
    /// Each choice, by its place, which is the value it sets.
    pub items: Vec<Choice>,
    pub at: Option<usize>,
}

/// Where `=`, `-`, `+` and `_` take a value: its small or large step that way, `count` times,
/// kept within its bounds, or round them where it wraps. A step that depends on where the value is
/// is worked out again after each, by its pace; and a list that steps once whatever the count
/// moves one choice.
pub(crate) fn stepped(view: &ParamView, up: bool, large: bool, count: usize) -> i32 {
    let by = |nudge: Steps, jump: Steps| {
        let steps = if large { jump } else { nudge };
        i64::from(if up { steps.up } else { -steps.down })
    };
    let count = match view.pace {
        Pace::Once => 1,
        _ => count.max(1),
    };
    if view.pace.steps(view.value).is_none() {
        let moved = i64::from(view.value) + by(view.nudge, view.jump) * count as i64;
        return within(view, moved);
    }
    let (mut value, mut nudge, mut jump) = (view.value, view.nudge, view.jump);
    for _ in 0..count {
        let next = within(view, i64::from(value) + by(nudge, jump));
        if next == value {
            break;
        }
        value = next;
        (nudge, jump) = view.pace.steps(value).unwrap_or((nudge, jump));
    }
    value
}

/// A value kept within a view's bounds, or taken round them where it wraps.
fn within(view: &ParamView, value: i64) -> i32 {
    let (low, high) = (i64::from(view.bounds.0), i64::from(view.bounds.1));
    let kept = match view.wraps && high > low {
        true => low + (value - low).rem_euclid(high - low + 1),
        false => value.clamp(low, high),
    };
    kept as i32
}

/// Where `=`, `-`, `+` and `_` take a value with a fine part, on its fine digits: the fine byte
/// moved by one or `10`, `count` times, carrying into the value past `FF` or below `00`, and
/// stopping at its top with no fine part. The value and the fine byte it leaves.
pub(crate) fn stepped_fine(view: &ParamView, up: bool, large: bool, count: usize) -> (i32, u8) {
    let size = if large { FINE_JUMP } else { 1 } * count as i64;
    let value = i64::from(view.value) << 8 | i64::from(view.fine.unwrap_or(0));
    let (low, high) = (i64::from(view.bounds.0) << 8, i64::from(view.bounds.1) << 8);
    let moved = (value + if up { size } else { -size }).clamp(low, high);
    ((moved >> 8) as i32, (moved & 0xFF) as u8)
}

/// What `i`'s text sets a value to, as the view reads it, or why not, for the status line. A
/// value that only hex digits are typed into, which can't be wrong, says nothing.
pub(crate) fn typed(view: &ParamView, text: &str) -> Result<i32, String> {
    let (low, high) = view.bounds;
    let refused = || view.why_not.clone().into_owned();
    let kept = |value: Option<i32>, lowest: i32| {
        value
            .filter(|value| (lowest..=high).contains(value))
            .ok_or_else(refused)
    };
    match view.reads {
        Reads::No => Err(String::new()),
        Reads::Hex { shift } => u8::from_str_radix(text, 16)
            .map(|byte| (i32::from(byte) << shift).clamp(low, high))
            .map_err(|_| String::new()),
        Reads::HexWithin { signed } => {
            let (sign, digits) = match text.strip_prefix('-') {
                Some(digits) if signed => (-1, digits),
                _ if signed => (1, text.strip_prefix('+').unwrap_or(text)),
                _ => (1, text.trim()),
            };
            let magnitude = u32::from_str_radix(digits, 16).ok();
            let value = magnitude.and_then(|magnitude| i32::try_from(magnitude).ok());
            kept(value.map(|value| sign * value), low)
        }
        Reads::Number {
            before,
            after,
            off,
            from_one,
        } => {
            let text = text.trim();
            if off && text.eq_ignore_ascii_case("off") {
                return Ok(low);
            }
            let digits = match before {
                Some(letter) => text
                    .trim_start_matches([letter.to_ascii_uppercase(), letter.to_ascii_lowercase()]),
                None => text,
            };
            let digits = match after.is_empty() {
                true => digits,
                false => digits.trim_end_matches(after).trim(),
            };
            let number = digits.parse::<i32>().ok();
            let number = number.map(|number| if from_one { number - 1 } else { number });
            // A number starts above `off`.
            kept(number, if off { low + 1 } else { low })
        }
        Reads::Glide => {
            let text = text.trim();
            if text.eq_ignore_ascii_case("off") {
                return Ok(0);
            }
            let ms = text.trim_end_matches("ms").trim().parse::<f32>().ok();
            ms.filter(|ms| (0.0..=vt_model::settings::LONGEST_GLIDE_MS).contains(ms))
                .map(|ms| i32::from(vt_model::settings::glide_value(ms)))
                .ok_or_else(refused)
        }
        Reads::Reset => vt_model::song::reset_named(text)
            .map(|reset| reset.map_or(0, i32::from))
            .ok_or_else(refused),
        // `Note` is below no time.
        Reads::Hold => {
            let text = text.trim();
            if text.eq_ignore_ascii_case("note") {
                return Ok(-1);
            }
            let ms = text.trim_end_matches("ms").trim().parse::<i32>().ok();
            kept(ms, 0)
        }
        Reads::Note => {
            let midi = match text.trim().parse::<Note>() {
                Ok(Note::On(midi)) => Some(i32::from(midi)),
                _ => None,
            };
            kept(midi, low)
        }
    }
}

/// The value a knob on its `step`th place puts it at, counted from its lowest.
pub(crate) fn at_place(view: &ParamView, step: usize) -> i32 {
    view.bounds.0 + step as i32
}

/// Where a knob turned a share of the way from a value's lowest to its highest puts it, to the
/// nearest.
pub(crate) fn shared(view: &ParamView, share: f64) -> i32 {
    let (low, high) = view.bounds;
    low + (share * f64::from(high - low)).round() as i32
}

/// The same for a value with a fine part, to the nearest of its fine steps: the value and its
/// fine byte, which is 0 at its top.
pub(crate) fn shared_fine(view: &ParamView, share: f64) -> (i32, u8) {
    let (low, high) = (view.bounds.0 << 8, view.bounds.1 << 8);
    let at = low + (share * f64::from(high - low)).round() as i32;
    let value = at >> 8;
    let fine = if value == view.bounds.1 { 0 } else { at & 0xFF };
    (value, fine as u8)
}

/// What `x` puts back.
pub(crate) fn reset(view: &ParamView) -> i32 {
    view.default
}

/// `K`'s list for a setting, opened on the value it has, with a machine's own lists numbered,
/// or none for a value that isn't a choice.
pub(crate) fn choices(view: &ParamView) -> Option<ChoiceList> {
    if view.choices.is_empty() {
        return None;
    }
    let items = view
        .choices
        .iter()
        .take(0x100)
        .enumerate()
        .map(|(at, choice)| Choice {
            name: match view.numbered {
                true => format!("{at:02X}  {}", choice.name).into(),
                false => choice.name.clone(),
            },
            ..choice.clone()
        });
    Some(ChoiceList {
        title: view.title,
        items: items.collect(),
        at: usize::try_from(view.value).ok(),
    })
}

#[cfg(test)]
mod tests {
    use vt_model::modulation::{ModField, ModKind};
    use vt_model::song::{Song, Track};

    use super::*;
    use crate::pages::track::{Control, view};

    fn shown(control: Control, track: &Track) -> ParamView {
        view(control, track, &Song::new(1), false).unwrap()
    }

    #[test]
    fn a_count_of_time_steps_grows_as_the_time_does() {
        let mut track = Track::default();
        track.mods[0].kind = ModKind::Adsr;
        track.mods[0].adsr.attack_ms = 8;
        let attack = shown(Control::Mod(0, ModField::Attack), &track);
        // 9 and 10 by 1 ms, then 15 by 5.
        assert_eq!(stepped(&attack, true, false, 3), 15);
        assert_eq!(stepped(&attack, false, false, 20), 0);
    }

    #[test]
    fn a_hold_steps_down_to_none_and_then_to_the_note() {
        let mut track = Track::default();
        track.mods[0].kind = ModKind::Ahd;
        track.mods[0].ahd.hold = vt_model::envelope::Hold::Ms(5);
        let hold = shown(Control::Mod(0, ModField::Hold), &track);
        assert_eq!(stepped(&hold, false, true, 1), 0);
        assert_eq!(stepped(&hold, false, true, 2), -1);
        assert_eq!(typed(&hold, "note"), Ok(-1));
        assert_eq!(typed(&hold, " 12ms "), Ok(12));
        assert!(typed(&hold, "-1").is_err());
    }

    #[test]
    fn a_list_of_the_tracks_own_steps_once_whatever_the_count() {
        let track = Track::default();
        let machine = shown(Control::Machine, &track);
        assert_eq!(stepped(&machine, true, false, 3), machine.value + 1);
        let pan = shown(Control::Base(vt_model::action::TrackControl::Pan), &track);
        assert_eq!(stepped(&pan, true, true, 3), pan.value + 0x30);
    }

    #[test]
    fn a_fine_part_carries_into_its_value_and_stops_at_the_top() {
        let track = Track::default();
        let cutoff = shown(
            Control::Base(vt_model::action::TrackControl::Cutoff),
            &track,
        );
        let top = cutoff.bounds.1;
        assert_eq!(stepped_fine(&cutoff, true, false, 1), (top, 0));
        assert_eq!(stepped_fine(&cutoff, false, false, 1), (top - 1, 0xFF));
    }

    #[test]
    fn typing_off_or_a_number_above_it() {
        let track = Track::default();
        let choke = shown(Control::Choke, &track);
        assert_eq!(typed(&choke, "off"), Ok(0));
        assert_eq!(typed(&choke, "2"), Ok(2));
        assert!(typed(&choke, "0").is_err());
        let group = shown(Control::Group, &track);
        assert_eq!(typed(&group, "g3"), Ok(3));
    }
}
