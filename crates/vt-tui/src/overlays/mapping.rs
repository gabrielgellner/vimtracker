//! Macros on the track page (`docs/designs/generators-page.md` §4.3): `M` puts the setting under
//! the cursor on one of the track's eight macros, or moves it, or takes it off; `gm` opens a
//! macro's mapping, each setting it holds with its range; `gd` goes from a setting to the macro
//! holding it; `:macro` names one; and a setting a macro holds refuses a turn of its own, as Live
//! greys a mapped control out.

pub mod draw;

use std::time::Instant;

use ratatui::Frame;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;

use crate::controls::macro_name;
use vt_model::action::Action;
use vt_model::macros::{
    GEN_ZERO, GenColumn, MACROS, MacroMap, MacroTarget, Zone, top_value, valid_name,
};
use vt_model::settings::{Kind, Owner};
use vt_model::shape::params::{BEND_JUMP, BEND_NUDGE, Typing};
use vt_model::song::{Song, Track};

use crate::app::App;
use crate::controls::Control;
use crate::graphics::plot::{MappingDrawing, Shade};
use crate::graphics::{PanelIds, PanelKey, PanelWant, Shape};
use crate::input::bindings::{Cmd, DESCRIBE, adjusting_key};
use crate::input::keys::Key;
use crate::input::mode::{Mode, View};
use crate::input::typed::{Typed, TypedValue};
use crate::overlays::Overlay;
use crate::overlays::mapping::draw::{RESPONSE_HEIGHT, RESPONSE_WIDTH};
use crate::overlays::picker::{Item, Pick, Picker};
use crate::pages::{Ctx, CtxMut, Outcome};
use crate::services::Services;

/// The macro picker's title, which `M` opens and a digit chooses in at once.
pub(crate) const MACRO_PICKER: &str = "Map to a macro";
/// The list `gd` opens on a setting more than one macro holds, to go to one.
pub(crate) const HOLDING_PICKER: &str = "Go to a macro";

/// How far a large step moves an end of a range, in coarse steps.
const RANGE_JUMP: i32 = 0x10;

/// What of a mapping the view's cursor is on, across its row: the ends of its range, then its
/// response's `Curve` and `S`, then the start and the end of its zone, the part of the macro's
/// turn it acts over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MapField {
    #[default]
    Min,
    Max,
    Curve,
    S,
    ZoneStart,
    ZoneEnd,
}

impl MapField {
    /// Left to right, as `h` and `l` go.
    pub const ALL: [Self; 6] = [
        Self::Min,
        Self::Max,
        Self::Curve,
        Self::S,
        Self::ZoneStart,
        Self::ZoneEnd,
    ];

    /// Its heading in the view: the zone's two ends share one, `Zone`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Min => "Min",
            Self::Max => "Max",
            Self::Curve => "Curve",
            Self::S => "S",
            Self::ZoneStart => "Zone",
            Self::ZoneEnd => "",
        }
    }

    /// Whether it bends the response rather than being an end of the range.
    pub fn bends(self) -> bool {
        matches!(self, Self::Curve | Self::S)
    }

    /// Whether it's an end of the zone.
    pub fn zones(self) -> bool {
        matches!(self, Self::ZoneStart | Self::ZoneEnd)
    }

    /// The one beside it, `by` places to the right, kept to the row.
    fn beside(self, by: isize) -> Self {
        let at = Self::ALL
            .iter()
            .position(|field| *field == self)
            .unwrap_or(0);
        Self::ALL[at.saturating_add_signed(by).min(Self::ALL.len() - 1)]
    }
}

/// A `Curve` or `S` as the view and the status line show it, as the shape panel does: `+20`,
/// `-5`, or `0` straight.
pub(crate) fn bend_text(bend: i8) -> String {
    if bend > 0 {
        format!("+{bend}")
    } else {
        bend.to_string()
    }
}

/// The open mapping view: which macro, which of its mappings the cursor's on, and what of it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MappingView {
    pub at: usize,
    pub row: usize,
    /// The end or the bend the cursor's on.
    pub field: MapField,
    /// Whether `d` was pressed, waiting for a second to unmap.
    pub(crate) deleting: bool,
    /// What's typed so far after `i`, if it's typing: hex digits for an end, a signed whole
    /// number for a bend.
    pub typing: Option<TypedValue>,
    /// The id of the terminal's image of the response of the mapping under the cursor.
    pub images: PanelIds,
}

impl MappingView {
    /// The picture the view wants on screen for the cursor's track: the response of the mapping
    /// under its cursor, drawn again as its `Curve`, its `S`, its way, its zone or its macro's turn
    /// changes.
    pub(crate) fn panels(&self, track: &Track) -> Vec<PanelWant> {
        let held = &track.macros[self.at];
        let Some(map) = held
            .maps
            .get(self.row.min(held.maps.len().saturating_sub(1)))
        else {
            return Vec::new();
        };
        vec![PanelWant {
            key: PanelKey::Mapping,
            shade: Shade::Focused,
            shape: Shape::Mapping(MappingDrawing::of(map, held.value)),
            cells: (RESPONSE_WIDTH, RESPONSE_HEIGHT),
        }]
    }
}

/// An end of a mapping as the view shows it: a list's choice by name, and anything else as its
/// coarse hex, as the track page shows it, or a generator's setting as its number.
pub(crate) fn end_text(target: MacroTarget, value: u16) -> String {
    let Some(id) = target.setting() else {
        return (i32::from(value) - GEN_ZERO).to_string();
    };
    match id.entry().kind {
        Kind::Stepped { choices, .. } => choices
            .get(usize::from(value >> 8))
            .map_or_else(String::new, |choice| (*choice).to_owned()),
        Kind::Continuous { .. } => format!("{:02X}", value >> 8),
    }
}

/// An end of a mapping in its setting's units, for the status line: a mod slot's `Rate` as the
/// kind of rate the slot has.
fn end_shown(track: &Track, target: MacroTarget, value: u16) -> String {
    let Some(id) = target.setting() else {
        return (i32::from(value) - GEN_ZERO).to_string();
    };
    match id.owner() {
        (Owner::ModOwn(slot), at) => track.mods[slot].own_shown(at, value),
        _ => None,
    }
    .unwrap_or_else(|| id.entry().kind.shown(value))
}

/// The track a mapping of a macro on the track at `index` reaches: a group's on one of its tracks
/// that track (decision 408), and any other the macro's own. None for a group's on a place its
/// group has no track at.
pub(crate) fn reached<'a>(song: &'a Song, index: usize, map: &MacroMap) -> Option<&'a Track> {
    match map.member {
        None => song.tracks.get(index),
        Some(place) => song.tracks.get(song.group_member(index, place)?),
    }
}

/// A mapping's setting by name on the track it reaches, a group's on one of its tracks after
/// that track's name, as `Kick Cut`, or its place in the group while it has no track there.
pub(crate) fn map_name(song: &Song, index: usize, map: &MacroMap) -> String {
    match (map.member, reached(song, index, map)) {
        (None, Some(track)) => map.target.name_on(track),
        (Some(place), Some(track)) => {
            let member = song.group_member(index, place).unwrap_or_default();
            format!("{} {}", track.label(member), map.target.name_on(track))
        }
        (Some(place), None) => format!("Place {} {}", place + 1, map.target.name()),
        (None, None) => map.target.name(),
    }
}

/// An end of a mapping in its setting's units on the track it reaches.
fn end_on(song: &Song, index: usize, map: &MacroMap, value: u16) -> String {
    let fresh = Track::default();
    let track = reached(song, index, map).unwrap_or(&fresh);
    end_shown(track, map.target, value)
}

/// What a macro of the track at `index` holds, for the picker and the status line: its settings
/// by name, a group's on its tracks after the track's name, or that it holds nothing yet.
pub fn holds(song: &Song, index: usize, at: usize) -> String {
    let maps = &song.tracks[index].macros[at].maps;
    if maps.is_empty() {
        return "nothing yet".to_owned();
    }
    maps.iter()
        .map(|map| map_name(song, index, map))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A mapping's zone as the lines that describe it add it: ` over 00–7F`, or nothing for the whole
/// turn.
fn over(map: &MacroMap) -> String {
    if map.zone.is_whole() {
        String::new()
    } else {
        format!(" over {}", map.zone.text())
    }
}

/// What a macro moves, for the line that describes it: each setting by name, from where to where
/// in its units, and over what part of the turn if not all of it, as `Cut 40 to C0 and Res 20 to
/// 60 over 80–FF`, or none while it holds nothing.
pub(crate) fn moves(song: &Song, index: usize, at: usize) -> Option<String> {
    let each: Vec<String> = song.tracks[index].macros[at]
        .maps
        .iter()
        .map(|map| {
            format!(
                "{} {} to {}{}",
                map_name(song, index, map),
                end_on(song, index, map, map.min),
                end_on(song, index, map, map.max),
                over(map)
            )
        })
        .collect();
    match each.split_last() {
        None => None,
        Some((last, [])) => Some(last.clone()),
        Some((last, rest)) => Some(format!("{} and {last}", rest.join(", "))),
    }
}

/// What the status line says of a setting a macro holds: its macro, and where the macro takes it,
/// in its units, and for a generator's `Wave` or `Rate`, that it's by hand.
/// The macro holding a setting on the track at `index`, as the status line names it, `macro 2`
/// or a group's, `G1's macro 2`, with the macro's place and its mapping there: its group's, if
/// one holds it, as that's the one heard (decision 408), or its own.
pub(crate) fn holder(
    song: &Song,
    index: usize,
    target: MacroTarget,
) -> Option<(String, usize, &MacroMap)> {
    if let Some((at, map)) = song.group_holding(index, target) {
        let group = song.group_place(index)?.0;
        let on = format!("{}'s macro", song.tracks[group].label(group));
        return Some((on, at, map));
    }
    let (at, map) = song.tracks.get(index)?.macro_holding(target)?;
    Some(("macro".to_owned(), at, map))
}

/// What the status line says of a setting a macro holds on the track at `index`: its macro, and
/// where the macro takes it, in its units, and for a generator's `Wave` or `Rate`, that it's by
/// hand.
pub(crate) fn held_text(song: &Song, index: usize, target: MacroTarget) -> Option<String> {
    let track = song.tracks.get(index)?;
    let by_hand = if target.by_hand() { ", by hand" } else { "" };
    let (on, at, map) = holder(song, index, target)?;
    let macros = match map.member {
        Some(_) => &song.tracks[song.group_place(index)?.0].macros,
        None => &track.macros,
    };
    Some(format!(
        "on {on} {} {}, {} to {}{}{by_hand}",
        at + 1,
        macro_name(&macros[at], at),
        end_shown(track, target, map.min),
        end_shown(track, target, map.max),
        over(map)
    ))
}

/// The line describing what's under the mapping view's cursor, `Space d`: an end of the range, a
/// bend or the zone, and what it means for the setting the mapping turns. `index` is the track
/// whose macro it is.
pub(crate) fn mapping_line(song: &Song, index: usize, view: &MappingView) -> Option<String> {
    let maps = &song.tracks.get(index)?.macros[view.at].maps;
    let map = maps.get(view.row.min(maps.len().checked_sub(1)?))?;
    let name = map_name(song, index, map);
    let (min, max) = (
        end_on(song, index, map, map.min),
        end_on(song, index, map, map.max),
    );
    let zone = map.zone;
    let (start, end) = (format!("{:02X}", zone.start), format!("{:02X}", zone.end));
    Some(match view.field {
        MapField::Min => format!(
            "Min {min}: where the macro puts {name} at {start}, the start of its zone, and below it."
        ),
        MapField::Max => format!(
            "Max {max}: where the macro puts {name} at {end}, the end of its zone, and above it."
        ),
        MapField::Curve => format!(
            "Curve {}: bends {name}'s way from Min to Max, early above 0 and late below; 0 is straight.",
            bend_text(map.curve)
        ),
        MapField::S => format!(
            "S {}: eases both ends of {name}'s way, an S above 0 and the reverse below; 0 is straight.",
            bend_text(map.s)
        ),
        MapField::ZoneStart | MapField::ZoneEnd if zone.is_whole() => format!(
            "Zone {}: the part of the macro's turn that moves {name}, here all of it. A part, as 00–7F, holds {name} at Min below it and at Max above, so one macro can turn two settings in turn.",
            zone.text()
        ),
        MapField::ZoneStart | MapField::ZoneEnd => format!(
            "Zone {}: the part of the macro's turn that moves {name}, from {min} at {start} to {max} at {end}; below {start} it holds at {min}, above {end} at {max}.",
            zone.text()
        ),
    })
}

/// What a macro would hold of a control on a track: the registry's setting it is, or a live
/// generator's setting by the generator's column.
pub(crate) fn target_of(control: Control, track: &Track) -> Option<MacroTarget> {
    Some(match control {
        Control::Gen {
            phrase,
            generator,
            param,
        } => {
            let found = track.phrases.get(&phrase)?.generators().get(generator)?;
            MacroTarget::Gen {
                column: GenColumn::of(found.column),
                param,
            }
        }
        control => MacroTarget::Setting(control.setting_for_macro(track)?),
    })
}

/// The macros holding a control's setting on a track, with their mappings there: what the track
/// page marks a setting by and `gd` goes to.
pub(crate) fn holding(control: Control, track: &Track) -> impl Iterator<Item = (usize, &MacroMap)> {
    target_of(control, track)
        .into_iter()
        .flat_map(move |target| track.macros_holding(target))
}

impl Services {
    /// The setting under the cursor on the track page, `control`, if it's one a macro can hold: one
    /// of the registry's, or a live generator's on the Gen page, by its column.
    pub(crate) fn macro_target(&self, control: Control) -> Option<MacroTarget> {
        let track = self.song().tracks.get(self.cursor.track)?;
        let target = target_of(control, track)?;
        target.mappable().then_some(target)
    }

    /// A generator setting's bounds and where it is now, on the generator under the cursor.
    fn gen_numbers(&self, control: Control, view: View) -> Option<((i32, i32), i32)> {
        let Control::Gen {
            phrase,
            generator,
            param,
        } = control
        else {
            return None;
        };
        let live = self.live_generator(self.cursor.track, phrase, generator, view)?;
        let context = live.context();
        let bounds = param.bounds(&context)?;
        let now = param.number(&live.generator.heard(), &context)?;
        Some((bounds, now))
    }

    /// The macro holding the setting under the cursor, and its mapping there, if one does.
    pub(crate) fn macro_here(&self, control: Control) -> Option<(MacroTarget, usize, MacroMap)> {
        let target = self.macro_target(control)?;
        let track = self.song().tracks.get(self.cursor.track)?;
        let (at, map) = track.macro_holding(target)?;
        Some((target, at, *map))
    }

    /// Whether a command would turn a setting a macro holds, which is refused, saying where to
    /// turn it instead.
    pub(crate) fn refuses_turn(&mut self, cmd: Cmd, control: Control) -> bool {
        let turns = matches!(
            cmd,
            Cmd::Increase
                | Cmd::Decrease
                | Cmd::IncreaseLarge
                | Cmd::DecreaseLarge
                | Cmd::ClearField
                | Cmd::ToNothing
                | Cmd::PickValue
                | Cmd::EnterInsert
        );
        if !turns {
            return false;
        }
        if let Some((target, at, _)) = self.group_macro_here(control) {
            let name = target.name_on(&self.page_track());
            let song = self.song();
            let (on, ..) = holder(song, self.cursor.track, target).expect("a group's holds it");
            self.status = format!("{name} is on {on} {}: turn it there, gd goes to it", at + 1);
            return true;
        }
        let Some((target, at, _)) = self.macro_here(control) else {
            return false;
        };
        let name = target.name_on(&self.page_track());
        self.status = format!(
            "{name} is on macro {}: turn it there, or gm for its range",
            at + 1
        );
        true
    }

    /// The macro of its group's track holding the setting under the cursor on a track in a group,
    /// and its mapping there, if one does (decision 408).
    pub(crate) fn group_macro_here(
        &self,
        control: Control,
    ) -> Option<(MacroTarget, usize, MacroMap)> {
        let target = self.macro_target(control)?;
        let (at, map) = self.song().group_holding(self.cursor.track, target)?;
        Some((target, at, *map))
    }

    /// `M`: list the eight macros, each with what it holds, to put the setting under the cursor
    /// on one, opened on the one it's on already, or nothing, the status line saying why.
    pub(crate) fn macro_picker(&mut self, control: Control) -> Outcome {
        let Some(target) = self.macro_target(control) else {
            self.status = match control {
                Control::Macro(_) => {
                    "A macro can't hold a macro: M maps the settings around it".to_owned()
                }
                _ => "M maps a setting to a macro, and this isn't one a macro reaches".to_owned(),
            };
            return Outcome::Done;
        };
        let index = self.cursor.track;
        let song = self.song();
        let track = &song.tracks[index];
        let item = |owner: usize, at: usize, label: String, pick: Pick, on: bool| {
            let mut meaning = holds(song, owner, at);
            if on {
                meaning = format!("{meaning}; Enter takes it off");
            }
            Item::new(label, pick).meaning(meaning)
        };
        let own = track.macro_holding(target).map(|(at, _)| at);
        let mut items: Vec<Item> = (0..MACROS)
            .map(|at| {
                let label = format!("{} {}", at + 1, macro_name(&track.macros[at], at));
                item(index, at, label, Pick::Macro(at), own == Some(at))
            })
            .collect();
        // On a track in a group, its group's macros come after its own (decision 408).
        let mut on = own;
        if let Some((group, _)) = song.group_place(index) {
            let held = song.group_holding(index, target).map(|(at, _)| at);
            let macros = &song.tracks[group].macros;
            let name = song.tracks[group].label(group);
            items.extend((0..MACROS).map(|at| {
                let label = format!("{name} {} {}", at + 1, macro_name(&macros[at], at));
                item(group, at, label, Pick::GroupMacro(at), held == Some(at))
            }));
            on = held.map(|at| MACROS + at).or(on);
        }
        self.status = format!(
            "Enter maps {} to the macro chosen, from bottom to top to start with; a digit chooses at once",
            target.name_on(track)
        );
        Outcome::Open(Box::new(Picker::new(MACRO_PICKER, items).opened_on(on)))
    }

    /// Put the setting under the cursor on a macro, from its bottom to its top, or take it off
    /// the macro it's on, or move it to this one with its range, as one undo step. The first
    /// mapping of a macro holding nothing turns the macro to where the setting is, so nothing
    /// jumps. `control` is the setting under the cursor, and `view` the view on screen.
    pub(crate) fn map_to_macro(&mut self, at: usize, control: Control, view: View) {
        self.put_on_macro(false, at, control, view);
    }

    /// Put the setting under the cursor, on a track in a group, on a macro of the group's track,
    /// or take it off, or move it there, as [`Services::map_to_macro`] does with the track's own
    /// (decision 408).
    pub(crate) fn map_to_group_macro(&mut self, at: usize, control: Control, view: View) {
        self.put_on_macro(true, at, control, view);
    }

    /// Put the setting under the cursor on a macro of its own track's, or with `group` of its
    /// group's track's, by the track's place in the group, or take it off that macro, or move it
    /// there from wherever it is, its own track's or its group's, with its range.
    fn put_on_macro(&mut self, group: bool, at: usize, control: Control, view: View) {
        let Some(target) = self.macro_target(control) else {
            return;
        };
        let index = self.cursor.track;
        let song = self.song().clone();
        let track = &song.tracks[index];
        let name = target.name_on(track);
        let place = song.group_place(index);
        let (owner, member) = match (group, place) {
            (false, _) => (index, None),
            (true, Some((owner, place))) => (owner, Some(place)),
            (true, None) => return,
        };
        let held = &song.tracks[owner].macros[at];
        // Where it is now: on the track's own macros, or its group's, which have the last word.
        let own = track
            .macro_holding(target)
            .map(|(on, map)| (None, on, *map));
        let grouped = place.and_then(|(_, place)| {
            let (on, map) = song.group_holding(index, target)?;
            Some((Some(place), on, *map))
        });
        let label = |member: Option<u8>, at: usize| match member {
            Some(_) => format!("{}'s macro {}", song.tracks[owner].label(owner), at + 1),
            None => format!("macro {}", at + 1),
        };
        let owner_of = |member: Option<u8>| if member.is_some() { owner } else { index };
        let unmap = |member: Option<u8>| Action::UnmapMacro {
            track: owner_of(member),
            target,
            member,
        };
        let this = if group { grouped } else { own };
        let other = if group { own } else { grouped };
        match (this, other) {
            (Some((_, on, _)), _) if on == at => {
                self.apply(&unmap(member));
                self.status = format!("{name} is off {}, and back to its own", label(member, at));
            }
            (Some((from, on, map)), _) | (None, Some((from, on, map))) => {
                let mut actions = Vec::new();
                if from != member {
                    actions.push(unmap(from));
                }
                actions.push(Action::MapMacro {
                    track: owner,
                    at,
                    map: MacroMap { member, ..map },
                });
                self.apply(&Action::Batch(actions));
                let now = &self.song().tracks[owner].macros[at];
                self.status = format!(
                    "{name} moved from {} to {}, {}",
                    label(from, on),
                    label(member, at),
                    macro_name(now, at)
                );
            }
            (None, None) => {
                let (map, now) = match target.setting() {
                    Some(id) => (MacroMap::whole(target), track.value_of(id)),
                    None => {
                        let Some((bounds, now)) = self.gen_numbers(control, view) else {
                            return;
                        };
                        let now = (now + GEN_ZERO).clamp(0, i32::from(u16::MAX)) as u16;
                        (MacroMap::numbers(target, bounds), now)
                    }
                };
                let map = MacroMap { member, ..map };
                let mut actions = vec![Action::MapMacro {
                    track: owner,
                    at,
                    map,
                }];
                let first = held.maps.is_empty();
                if first {
                    actions.push(Action::SetMacro {
                        track: owner,
                        at,
                        value: map.macro_for(now),
                    });
                }
                self.apply(&Action::Batch(actions));
                let now = &self.song().tracks[owner].macros[at];
                let on = label(member, at);
                self.status = if first {
                    format!(
                        "{name} is on {on}, {}, from its bottom to its top: gm for its range",
                        macro_name(now, at)
                    )
                } else {
                    format!(
                        "{name} is on {on}, {}, and jumps to where the macro is: gm for its range",
                        macro_name(now, at)
                    )
                };
            }
        }
    }

    /// `gm`: open the mapping of the macro under the cursor, or of the one the setting under the
    /// cursor, `control`, is on, on its row; or nothing, the status line saying why.
    pub(crate) fn mapping_of(&mut self, control: Control) -> Outcome {
        let track = self.page_track();
        let (at, row) = match control {
            Control::Macro(at) => (at, 0),
            _ => match self.macro_here(control) {
                Some((target, at, _)) => (at, track.macros[at].map_of(target).unwrap_or(0)),
                None if let Some((target, at, _)) = self.group_macro_here(control) => {
                    let song = self.song();
                    let (on, ..) = holder(song, self.cursor.track, target).expect("it's held");
                    self.status = format!(
                        "{} is on {on} {}: gd goes there, and gm there opens its mapping",
                        target.name_on(&track),
                        at + 1
                    );
                    return Outcome::Done;
                }
                None => {
                    self.status =
                        "This isn't on a macro: M maps it to one, and gm on a macro opens it"
                            .to_owned();
                    return Outcome::Done;
                }
            },
        };
        MappingView::open(at, row, self)
    }

    /// `:macro Name`: name the macro under the cursor, the mapping view's or the one the setting
    /// under the cursor is on; `:macro` alone gives it back its first mapping's name. `open` is
    /// the macro whose mapping view is open, `control` the setting under the cursor and `view`
    /// the view on screen.
    pub(crate) fn name_macro(
        &mut self,
        name: &str,
        open: Option<usize>,
        control: Control,
        view: View,
    ) {
        let at = match (open, control) {
            (Some(at), _) | (None, Control::Macro(at)) => Some(at),
            (None, _) => self.macro_here(control).map(|(_, at, _)| at),
        };
        let Some(at) = at.filter(|_| view == View::Track || open.is_some()) else {
            self.status = ":macro names the macro under the cursor, on the track page".to_owned();
            return;
        };
        let name = name.trim();
        if !name.is_empty() && !valid_name(name) {
            self.status = "A macro's name is up to 12 plain characters".to_owned();
            return;
        }
        self.apply(&Action::NameMacro {
            track: self.cursor.track,
            at,
            name: (!name.is_empty()).then(|| name.to_owned()),
        });
        let held = &self.page_track().macros[at];
        self.status = format!("Macro {} is {}", at + 1, macro_name(held, at));
    }
}

impl Services {
    /// The macros holding the setting `control` on the cursor's track, each with its mapping.
    pub(crate) fn macros_holding(&self, control: Control) -> Vec<(usize, MacroMap)> {
        holding(control, &self.page_track())
            .map(|(at, map)| (at, *map))
            .collect()
    }

    /// The list `gd` opens on a setting more than one macro holds, `held`, to go to one, each
    /// with its range.
    pub(crate) fn holding_picker(&self, held: &[(usize, MacroMap)]) -> Picker {
        let track = self.page_track();
        let items = held
            .iter()
            .map(|(at, map)| {
                let label = format!("{} {}", at + 1, macro_name(&track.macros[*at], *at));
                let range = format!(
                    "{} to {}{}",
                    end_shown(&track, map.target, map.min),
                    end_shown(&track, map.target, map.max),
                    over(map)
                );
                Item::new(label, Pick::MacroHolding(*at)).meaning(range)
            })
            .collect();
        Picker::new(HOLDING_PICKER, items)
    }
}

impl MappingView {
    /// A macro's mapping view open on one of its rows, or nothing, saying it holds nothing.
    pub(crate) fn open(at: usize, row: usize, services: &mut Services) -> Outcome {
        if services.page_track().macros[at].maps.is_empty() {
            services.status = format!("Macro {} holds nothing yet: M on a setting maps it", at + 1);
            return Outcome::Done;
        }
        let view = MappingView {
            at,
            row,
            ..MappingView::default()
        };
        view.say(services);
        Outcome::Mapping(view)
    }
}

/// A key of a macro's mapping view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappingKey {
    Close,
    Down,
    Up,
    Left,
    Right,
    Increase,
    Decrease,
    IncreaseLarge,
    DecreaseLarge,
    /// `x`: a bend back to straight, or a zone back to the whole turn.
    Straighten,
    /// `~`: swap the ends of the range.
    Swap,
    /// `i`: type what's under the cursor.
    Type,
    /// `d`, twice: take the setting off the macro.
    Delete,
    Undo,
    /// `Space d`: describe the mapping under the cursor, or stop, as on the pages.
    Describe,
}

impl Overlay for MappingView {
    const MODE: Mode = Mode::Mapping;

    const KEYS: &'static [(&'static str, Cmd, &'static str)] = &[
        ("Esc", Cmd::InMapping(MappingKey::Close), "Close"),
        ("q", Cmd::InMapping(MappingKey::Close), "Close"),
        ("j", Cmd::InMapping(MappingKey::Down), "The next setting"),
        ("Down", Cmd::InMapping(MappingKey::Down), "The next setting"),
        ("k", Cmd::InMapping(MappingKey::Up), "The setting before"),
        ("Up", Cmd::InMapping(MappingKey::Up), "The setting before"),
        (
            "h",
            Cmd::InMapping(MappingKey::Left),
            "Left, across Min, Max, Curve, S and Zone",
        ),
        (
            "Left",
            Cmd::InMapping(MappingKey::Left),
            "Left, across Min, Max, Curve, S and Zone",
        ),
        (
            "l",
            Cmd::InMapping(MappingKey::Right),
            "Right, across Min, Max, Curve, S and Zone",
        ),
        (
            "Right",
            Cmd::InMapping(MappingKey::Right),
            "Right, across Min, Max, Curve, S and Zone",
        ),
        (
            "=",
            Cmd::InMapping(MappingKey::Increase),
            "Up a step, an end's coarse step, a bend's 5 or a zone's 01",
        ),
        (
            "-",
            Cmd::InMapping(MappingKey::Decrease),
            "Down a step, an end's coarse step, a bend's 5 or a zone's 01",
        ),
        (
            "+",
            Cmd::InMapping(MappingKey::IncreaseLarge),
            "Up a large step, 10 coarse steps, a bend's 25 or a zone's 10",
        ),
        (
            "_",
            Cmd::InMapping(MappingKey::DecreaseLarge),
            "Down a large step, 10 coarse steps, a bend's 25 or a zone's 10",
        ),
        (
            "x",
            Cmd::InMapping(MappingKey::Straighten),
            "On Curve or S: back to straight; on Zone: the whole turn",
        ),
        (
            "~",
            Cmd::InMapping(MappingKey::Swap),
            "Swap the range's ends",
        ),
        (
            "i",
            Cmd::InMapping(MappingKey::Type),
            "Type it, an end or a zone's in hex and a bend from -100 to 100",
        ),
        (
            "d",
            Cmd::InMapping(MappingKey::Delete),
            "Twice: take the setting off the macro",
        ),
        ("u", Cmd::InMapping(MappingKey::Undo), "Undo"),
        ("Space d", Cmd::InMapping(MappingKey::Describe), DESCRIBE),
    ];

    fn key(&mut self, cmd: Cmd, ctx: &mut CtxMut) -> Outcome {
        let Cmd::InMapping(key) = cmd else {
            return Outcome::Passed;
        };
        let services = &mut *ctx.services;
        let count = services.page_track().macros[self.at].maps.len();
        let deleting = std::mem::take(&mut self.deleting);
        match key {
            MappingKey::Close => {
                services.status.clear();
                return Outcome::Close;
            }
            MappingKey::Down => self.move_row(|row| (row + 1).min(count - 1), services),
            MappingKey::Up => self.move_row(|row| row.saturating_sub(1), services),
            MappingKey::Left => self.across(-1, services),
            MappingKey::Right => self.across(1, services),
            MappingKey::Increase => self.nudge(true, false, services),
            MappingKey::Decrease => self.nudge(false, false, services),
            MappingKey::IncreaseLarge => self.nudge(true, true, services),
            MappingKey::DecreaseLarge => self.nudge(false, true, services),
            MappingKey::Straighten if self.field.bends() => self.set_bend(0, services),
            MappingKey::Straighten if self.field.zones() => {
                if let Some((_, map)) = self.row_here(services) {
                    self.set_range(map.zoned(Zone::WHOLE), services);
                }
            }
            MappingKey::Straighten => {}
            MappingKey::Swap => {
                if let Some((_, map)) = self.row_here(services) {
                    self.set_range(
                        MacroMap {
                            min: map.max,
                            max: map.min,
                            ..map
                        },
                        services,
                    );
                }
            }
            MappingKey::Type => {
                self.typing = Some(TypedValue::default());
                services.status = if self.field.bends() {
                    "From -100 to 100, then Enter".to_owned()
                } else if self.field.zones() {
                    "Two hex digits for this end of the zone, 00 to FF".to_owned()
                } else {
                    "Two hex digits for this end".to_owned()
                };
            }
            MappingKey::Delete if deleting => return self.unmap_row(services),
            MappingKey::Delete => self.deleting = true,
            // `App` undoes, and then keeps the view open on a mapping, if one is left.
            MappingKey::Undo => return Outcome::Do(Cmd::Undo),
            MappingKey::Describe => return Outcome::Do(Cmd::ToggleDescription),
        }
        Outcome::Done
    }

    /// Cmd with `=`'s and its neighbours' Cmd keys is that key, and nothing else held is read.
    fn reads(&self, key: Key) -> Key {
        let key = if key.command {
            adjusting_key(key).unwrap_or(key)
        } else {
            key
        };
        Key::new(key.code)
    }

    fn takes_text(&self) -> bool {
        self.typing.is_some()
    }

    /// What's typed after `i`; any other key does nothing, but let a `d` go.
    fn text(&mut self, key: Key, ctx: &mut CtxMut) -> Outcome {
        if self.typing.is_some() {
            self.typed(key.code, ctx.services);
        } else {
            self.deleting = false;
        }
        Outcome::Done
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect, ctx: &Ctx) {
        draw::draw(frame, area, ctx, self);
    }
}

impl MappingView {
    /// The mapping under the cursor, with its place, if the macro holds one.
    fn row_here(&self, services: &Services) -> Option<(usize, MacroMap)> {
        let maps = &services.page_track().macros[self.at].maps;
        let row = self.row.min(maps.len().checked_sub(1)?);
        Some((row, maps[row]))
    }

    /// Say on the status line what the mapping under the cursor does.
    pub(crate) fn say(&self, services: &mut Services) {
        let Some((_, map)) = self.row_here(services) else {
            return;
        };
        let track = services.page_track();
        let target = map.target;
        let held = &track.macros[self.at];
        let now = map.at(f32::from(held.value) / 256.0);
        let zone = map.zone;
        let by_hand = if target.by_hand() {
            ", by hand: a turn writes it in, but locks and modulation don't reach a list setting"
        } else {
            ""
        };
        if self.field.zones() {
            services.status = format!(
                "Zone {}: the macro turns {} from {} at {:02X} to {} at {:02X}, and holds it at {} below and {} above; it's at {:02X}, so {} is {}{by_hand}",
                zone.text(),
                target.name(),
                end_shown(&track, target, map.min),
                zone.start,
                end_shown(&track, target, map.max),
                zone.end,
                end_shown(&track, target, map.min),
                end_shown(&track, target, map.max),
                held.value >> 8,
                target.name(),
                end_shown(&track, target, now)
            );
            return;
        }
        if self.field.bends() {
            services.status = format!(
                "Curve {} bends {} from Min to Max, early above 0 and late below; S {} shapes both ends, an S above 0 and the reverse below",
                bend_text(map.curve),
                target.name(),
                bend_text(map.s)
            );
            return;
        }
        let along = if map.is_straight() {
            String::new()
        } else {
            format!(
                ", along Curve {} S {}",
                bend_text(map.curve),
                bend_text(map.s)
            )
        };
        let outside = if zone.is_whole() {
            ""
        } else {
            ", held there outside"
        };
        services.status = format!(
            "At {:02X} the macro sets {} to {}, at {:02X} to {}{along}{outside}; it's at {:02X}, so {} is {}{by_hand}",
            zone.start,
            target.name_on(&track),
            end_shown(&track, target, map.min),
            zone.end,
            end_shown(&track, target, map.max),
            held.value >> 8,
            target.name(),
            end_shown(&track, target, now)
        );
    }

    /// Put the mapping under the cursor's range at these ends.
    fn set_range(&self, map: MacroMap, services: &mut Services) {
        services.apply(&Action::MapMacro {
            track: services.cursor.track,
            at: self.at,
            map,
        });
        self.say(services);
    }

    /// `=` `-` `+` or `_`: move what's under the cursor up or down a small step or a large one,
    /// an end by a coarse step or `10` of them, and a bend by 5 or 25, as the shape panel's are.
    fn nudge(&self, up: bool, large: bool, services: &mut Services) {
        let sign = if up { 1 } else { -1 };
        if self.field.zones() {
            self.move_zone(sign * if large { RANGE_JUMP } else { 1 }, services);
        } else if self.field.bends() {
            self.move_bend(sign * if large { BEND_JUMP } else { BEND_NUDGE }, services);
        } else {
            self.move_end(sign * if large { RANGE_JUMP } else { 1 }, services);
        }
    }

    /// Put the bend under the cursor, `Curve` or `S`, at a value, kept to -100 to 100.
    fn set_bend(&self, to: i32, services: &mut Services) {
        let Some((_, map)) = self.row_here(services) else {
            return;
        };
        let (curve, s) = (i32::from(map.curve), i32::from(map.s));
        self.set_range(
            match self.field {
                MapField::Curve => map.bent(to, s),
                _ => map.bent(curve, to),
            },
            services,
        );
    }

    /// Move the bend under the cursor by so much.
    fn move_bend(&self, by: i32, services: &mut Services) {
        let Some((_, map)) = self.row_here(services) else {
            return;
        };
        let now = if self.field == MapField::Curve {
            map.curve
        } else {
            map.s
        };
        self.set_bend(i32::from(now) + by, services);
    }

    /// Put the end of the zone under the cursor at a value, kept below its end or above its start,
    /// as a zone is never empty.
    fn set_zone(&self, to: i32, services: &mut Services) {
        let Some((_, map)) = self.row_here(services) else {
            return;
        };
        let Zone { start, end } = map.zone;
        let zone = if self.field == MapField::ZoneStart {
            Zone::new(to.clamp(0, i32::from(end) - 1) as u8, end)
        } else {
            Zone::new(start, to.clamp(i32::from(start) + 1, 0xFF) as u8)
        };
        if let Some(zone) = zone {
            self.set_range(map.zoned(zone), services);
        }
    }

    /// Move the end of the zone under the cursor by so much.
    fn move_zone(&self, by: i32, services: &mut Services) {
        let Some((_, map)) = self.row_here(services) else {
            return;
        };
        let now = if self.field == MapField::ZoneStart {
            map.zone.start
        } else {
            map.zone.end
        };
        self.set_zone(i32::from(now) + by, services);
    }

    /// Move the end of the range under the cursor by so many coarse steps.
    fn move_end(&self, by: i32, services: &mut Services) {
        let Some((_, map)) = self.row_here(services) else {
            return;
        };
        let max = self.field == MapField::Max;
        let end = if max { map.max } else { map.min };
        // A setting's coarse step, or a generator's whole number.
        let moved = match map.target.setting() {
            Some(id) => {
                let top = i32::from(top_value(id.entry().kind));
                (i32::from(end >> 8 << 8) + by * 0x100).clamp(0, top) as u16
            }
            None => (i32::from(end) + by).clamp(0, i32::from(u16::MAX)) as u16,
        };
        let map = if max {
            MacroMap { max: moved, ..map }
        } else {
            MacroMap { min: moved, ..map }
        };
        self.set_range(map, services);
    }

    /// What's typed after `i`: hex digits for an end, set at the second, or a bend's signed number
    /// ended with `Enter`, as the shape panel's is; `Esc` gives up.
    fn typed(&mut self, code: KeyCode, services: &mut Services) {
        let bends = self.field.bends();
        let Some(typed) = self.typing.as_mut() else {
            return;
        };
        // An end's digits are neither taken back nor ended early.
        if !bends && matches!(code, KeyCode::Backspace | KeyCode::Enter) {
            return;
        }
        let rule = if bends {
            Typing::Whole { signed: true }
        } else {
            Typing::Hex
        };
        let before = typed.clone();
        match typed.key(code, rule.into()) {
            Typed::Editing => {
                if code == KeyCode::Backspace || *typed != before {
                    services.status = typed.shown();
                }
            }
            Typed::Cancel => {
                self.typing = None;
                self.say(services);
            }
            Typed::Commit(typed) if bends => {
                self.typing = None;
                match typed.parse::<i32>() {
                    Ok(value) => self.set_bend(value, services),
                    Err(_) => services.status = "Curve and S run from -100 to 100".to_owned(),
                }
            }
            Typed::Commit(typed) if self.field.zones() => {
                self.typing = None;
                self.set_zone(i32::from_str_radix(&typed, 16).unwrap_or(0), services);
            }
            Typed::Commit(typed) => {
                self.typing = None;
                let Some((_, map)) = self.row_here(services) else {
                    return;
                };
                let typed = u8::from_str_radix(&typed, 16).unwrap_or(0);
                let value = match map.target.setting() {
                    Some(id) => (u16::from(typed) << 8).min(top_value(id.entry().kind)),
                    // A generator's setting typed as a number, in hex as every value is.
                    None => (i32::from(typed) + GEN_ZERO) as u16,
                };
                self.set_range(
                    if self.field == MapField::Max {
                        MacroMap { max: value, ..map }
                    } else {
                        MacroMap { min: value, ..map }
                    },
                    services,
                );
            }
        }
    }

    fn move_row(&mut self, to: impl Fn(usize) -> usize, services: &mut Services) {
        self.row = to(self.row);
        self.say(services);
    }

    fn across(&mut self, by: isize, services: &mut Services) {
        self.field = self.field.beside(by);
        self.say(services);
    }

    /// `dd`: take the setting under the cursor off the macro, closing the view once it holds
    /// nothing.
    fn unmap_row(&mut self, services: &mut Services) -> Outcome {
        let Some((_, map)) = self.row_here(services) else {
            return Outcome::Done;
        };
        let track = services.page_track();
        let name = map.target.name_on(&track);
        services.apply(&Action::UnmapMacro {
            track: services.cursor.track,
            target: map.target,
            member: map.member,
        });
        if self.keep_open(services) {
            services.status = format!("{name} is off the macro: u brings it back");
            Outcome::Done
        } else {
            services.status =
                format!("{name} is off the macro, which holds nothing now: u brings it back");
            Outcome::Close
        }
    }

    /// Keep the cursor on a mapping after a change, saying whether the macro holds any, for the
    /// view to stay open.
    pub(crate) fn keep_open(&mut self, services: &Services) -> bool {
        let count = services
            .history
            .current()
            .tracks
            .get(services.cursor.track)
            .map_or(0, |track| track.macros[self.at].maps.len());
        if count == 0 {
            return false;
        }
        self.row = self.row.min(count - 1);
        true
    }
}

impl App {
    /// Give the mapping view a key. After an undo, which `App` does for it, the view stays open
    /// on a mapping, or closes once the macro holds none.
    pub(crate) fn mapping_key(&mut self, key: Key, now: Instant) {
        let Some(view) = self.overlays.mapping.as_mut() else {
            self.modes.mode = Mode::Normal;
            return;
        };
        let outcome = crate::overlays::give(
            view,
            &self.table,
            &mut self.grammar,
            CtxMut {
                services: &mut self.services,
                modes: &self.modes,
                wheel: &mut self.wheel,
            },
            key,
            now,
        );
        let undid = matches!(outcome, Outcome::Do(Cmd::Undo));
        self.settled(outcome, false);
        if undid && let Some(view) = self.overlays.mapping.as_mut() {
            if view.keep_open(&self.services) {
                self.modes.mode = Mode::Mapping;
            } else {
                self.overlays.mapping = None;
                self.modes.mode = Mode::Normal;
            }
        }
    }
}
