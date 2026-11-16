//! `Tune` and `Cents` on the track page: `K`'s list of their values and the edits that write the
//! sample's tuning, each one undo step on the bank entry. What the two are of and how they read
//! is [`crate::controls::tuning`]'s.

use vt_model::action::Action;
use vt_model::bank::BankEntry;

use crate::controls::tuning::{TuneOf, Tuning, steps, tuning, tuning_most, tuning_text};
use crate::overlays::picker::{Item, Pick, Picker};
use crate::pages::Outcome;
use crate::services::Services;

use crate::controls::Control;

/// `K` on `Tune` or `Cents`: its values, and for a slice's own tune, the sample's first.
pub(super) fn tuning_picker(control: Control, services: &mut Services) -> Outcome {
    let Some((number, entry, of)) = tuning_here(services) else {
        return Outcome::Done;
    };
    let tuning = Tuning {
        number,
        entry: &entry,
        of,
    };
    let current = tuning.value(control);
    let mut items = Vec::new();
    if control == Control::Tune
        && let TuneOf::Slice(_) = of
    {
        let sample = tuning_text(control, entry.edit.play.tune);
        items.push(
            Item::new(
                format!("As the sample, {sample}"),
                Pick::Tuning {
                    control,
                    value: None,
                },
            )
            .meaning("Play as the sample does"),
        );
    }
    let first = items.len();
    let choices: Vec<i8> = if control == Control::Cents {
        (-9..=9).map(|at| at * 10).collect()
    } else {
        (-24..=24).collect()
    };
    items.extend(choices.iter().map(|value| {
        Item::new(
            tuning_text(control, *value),
            Pick::Tuning {
                control,
                value: Some(*value),
            },
        )
    }));
    let at = choices
        .iter()
        .position(|value| *value == current)
        .map(|at| first + at);
    let title = if control == Control::Cents {
        "Sample cents"
    } else {
        "Sample tune"
    };
    Outcome::Open(Box::new(Picker::new(title, items).opened_on(at)))
}

/// The tuning of the cursor's track, or none, saying why.
fn tuning_here(services: &mut Services) -> Option<(u8, BankEntry, TuneOf)> {
    let song = services.song();
    let found = song
        .tracks
        .get(services.cursor.track)
        .and_then(|track| tuning(song, track))
        .map(|tuning| (tuning.number, tuning.entry.clone(), tuning.of));
    if found.is_none() {
        services.status = format!(
            "Track {:02} plays no sample to tune: K on Sample chooses one",
            services.cursor.track + 1
        );
    }
    found
}

/// Set `Tune` or `Cents`, or with none put it back, as one undo step, and say what it is now.
pub(crate) fn set_tuning(services: &mut Services, control: Control, value: Option<i8>) {
    let Some((number, entry, of)) = tuning_here(services) else {
        return;
    };
    let tuning = Tuning {
        number,
        entry: &entry,
        of,
    };
    let changed = tuning.with(control, value);
    let name = control.label().to_lowercase();
    let whose = tuning.whose(control);
    if changed == entry {
        services.status = format!("{whose} {name} is already that");
        return;
    }
    services.apply(&Action::SetBankEntry {
        number,
        entry: Some(changed.clone()),
    });
    let now = Tuning {
        entry: &changed,
        ..tuning
    };
    let back = match (control, of, value) {
        (Control::Tune, TuneOf::Slice(_), None) => ", as the sample",
        _ => "",
    };
    let text = tuning_text(control, now.value(control));
    services.status = format!("{whose} {name} {text}{back}: u to undo");
}

/// `=` `-` `+` `_` on `Tune` or `Cents`: a small or a large step, `count` over.
pub(super) fn adjust_tuning(
    services: &mut Services,
    control: Control,
    up: bool,
    count: usize,
    large: bool,
) {
    let Some((number, entry, of)) = tuning_here(services) else {
        return;
    };
    let value = Tuning {
        number,
        entry: &entry,
        of,
    }
    .value(control);
    let (small, big) = steps(control);
    let by = if large { big } else { small } * count.min(0x100) as i32;
    let limit = i32::from(tuning_most(control));
    let to = (i32::from(value) + if up { by } else { -by }).clamp(-limit, limit);
    set_tuning(services, control, Some(to as i8));
}

/// `i` on `Tune` or `Cents`: semitones or cents, signed or not, as `-5` or `+20`, with their
/// units or without.
pub(super) fn type_tuning(services: &mut Services, control: Control, text: &str) {
    let limit = tuning_most(control);
    let units = if control == Control::Cents {
        "ct"
    } else {
        "st"
    };
    let digits = text.trim().trim_end_matches(units).trim();
    let digits = digits.strip_prefix('+').unwrap_or(digits);
    match digits.parse::<i8>() {
        Ok(value) if value.unsigned_abs() <= limit.unsigned_abs() => {
            set_tuning(services, control, Some(value));
        }
        _ => {
            let what = if control == Control::Cents {
                "cents"
            } else {
                "semitones"
            };
            services.status = format!(
                "{} is {what} from -{limit} to +{limit}",
                control.name_on(&services.page_track())
            );
        }
    }
}
