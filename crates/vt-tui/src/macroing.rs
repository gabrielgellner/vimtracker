//! Macros on the track page (decision 285, `docs/designs/generators-page.md` §4.3): `M` puts the
//! setting under the cursor on one of the track's eight macros, or moves it, or takes it off; `gm`
//! opens a macro's mapping, each setting it holds with its range; `:macro` names one; and a
//! setting a macro holds refuses a turn of its own, as Live greys a mapped control out.

use ratatui::crossterm::event::KeyCode;
use vt_model::action::Action;
use vt_model::macros::{
    GEN_ZERO, GenColumn, MACROS, Macro, MacroMap, MacroTarget, top_value, valid_name,
};
use vt_model::settings::{Kind, Owner};
use vt_model::shape::params::{BEND_JUMP, BEND_NUDGE};
use vt_model::song::Track;

use crate::app::App;
use crate::input::bindings::Cmd;
use crate::input::keys::Key;
use crate::input::mode::Mode;
use crate::input::picker::{Item, Pick, Picker};
use crate::track_page::Control;

/// The macro picker's title, which `M` opens and a digit chooses in at once.
pub const MACRO_PICKER: &str = "Map to a macro";

/// How far a large step moves an end of a range, in coarse steps.
const RANGE_JUMP: i32 = 0x10;

/// The most characters a typed `Curve` or `S` takes, as the shape panel's: a sign and three
/// digits.
const BEND_TEXT: usize = 4;

/// What of a mapping the view's cursor is on, across its row: the ends of its range, then its
/// response's `Curve` and `S` (decision 314).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MapField {
    #[default]
    Min,
    Max,
    Curve,
    S,
}

impl MapField {
    /// Left to right, as `h` and `l` go.
    pub const ALL: [Self; 4] = [Self::Min, Self::Max, Self::Curve, Self::S];

    /// Its heading in the view.
    pub fn name(self) -> &'static str {
        match self {
            Self::Min => "Min",
            Self::Max => "Max",
            Self::Curve => "Curve",
            Self::S => "S",
        }
    }

    /// Whether it bends the response rather than being an end of the range.
    pub fn bends(self) -> bool {
        matches!(self, Self::Curve | Self::S)
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
pub fn bend_text(bend: i8) -> String {
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
    pub deleting: bool,
    /// What's typed so far after `i`, if it's typing: hex digits for an end, a signed whole
    /// number for a bend.
    pub typing: Option<String>,
}

/// An end of a mapping as the view shows it: a list's choice by name, and anything else as its
/// coarse hex, as the track page shows it, or a generator's setting as its number.
pub fn end_text(target: MacroTarget, value: u16) -> String {
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

/// A macro's name as the band and the lists show it: its own or its first mapping's, or
/// `Macro 3` while it holds nothing.
pub fn macro_name(held: &Macro, at: usize) -> String {
    held.shown_name()
        .unwrap_or_else(|| format!("Macro {}", at + 1))
}

/// What a macro holds, for the picker and the status line: its settings by name on this track,
/// or that it holds nothing yet.
pub fn holds(track: &Track, at: usize) -> String {
    let maps = &track.macros[at].maps;
    if maps.is_empty() {
        return "nothing yet".to_owned();
    }
    maps.iter()
        .map(|map| map.target.name_on(track))
        .collect::<Vec<_>>()
        .join(", ")
}

/// What the status line says of a setting a macro holds: its macro, and where the macro takes it,
/// in its units, and for a generator's `Wave` or `Rate`, that it's by hand.
pub fn held_text(track: &Track, target: MacroTarget) -> Option<String> {
    let (at, map) = track.macro_holding(target)?;
    let by_hand = if target.by_hand() { ", by hand" } else { "" };
    Some(format!(
        "on macro {} {}, {} to {}{by_hand}",
        at + 1,
        macro_name(&track.macros[at], at),
        end_shown(track, target, map.min),
        end_shown(track, target, map.max)
    ))
}

impl App {
    /// The setting under the cursor on the track page, if it's one a macro can hold: one of the
    /// registry's, or a live generator's on the Gen page, by its column.
    fn setting_here(&self) -> Option<MacroTarget> {
        let track = self.song().tracks.get(self.cursor.track)?;
        let target = match self.track_control() {
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
        };
        target.mappable().then_some(target)
    }

    /// A generator setting's bounds and where it is now, on the generator under the cursor.
    fn gen_numbers(&self) -> Option<((i32, i32), i32)> {
        let Control::Gen {
            phrase,
            generator,
            param,
        } = self.track_control()
        else {
            return None;
        };
        let live = self.live_generator(self.cursor.track, phrase, generator)?;
        let context = live.context();
        let bounds = param.bounds(&context)?;
        let now = param.number(&live.generator.heard(), &context)?;
        Some((bounds, now))
    }

    /// The macro holding the setting under the cursor, and its mapping there, if one does.
    pub(crate) fn macro_here(&self) -> Option<(MacroTarget, usize, MacroMap)> {
        let target = self.setting_here()?;
        let track = self.song().tracks.get(self.cursor.track)?;
        let (at, map) = track.macro_holding(target)?;
        Some((target, at, *map))
    }

    /// Whether a command would turn a setting a macro holds, which is refused, saying where to
    /// turn it instead.
    pub(crate) fn refuses_turn(&mut self, cmd: Cmd) -> bool {
        let turns = matches!(
            cmd,
            Cmd::Increase
                | Cmd::Decrease
                | Cmd::IncreaseLarge
                | Cmd::DecreaseLarge
                | Cmd::ClearField
                | Cmd::PickValue
                | Cmd::EnterInsert
        );
        let Some((target, at, _)) = self.macro_here().filter(|_| turns) else {
            return false;
        };
        let name = target.name_on(&self.page_track());
        self.status = format!(
            "{name} is on macro {}: turn it there, or gm for its range",
            at + 1
        );
        true
    }

    /// `M`: list the eight macros, each with what it holds, to put the setting under the cursor
    /// on one, opened on the one it's on already.
    pub(crate) fn open_macro_picker(&mut self) {
        let Some(target) = self.setting_here() else {
            self.status = match self.track_control() {
                Control::Macro(_) => {
                    "A macro can't hold a macro: M maps the settings around it".to_owned()
                }
                _ => "M maps a setting to a macro, and this isn't one a macro reaches".to_owned(),
            };
            return;
        };
        let track = self.page_track();
        let on = track.macro_holding(target).map(|(at, _)| at);
        let items = (0..MACROS)
            .map(|at| {
                let label = format!("{} {}", at + 1, macro_name(&track.macros[at], at));
                let mut meaning = holds(&track, at);
                if on == Some(at) {
                    meaning = format!("{meaning}; Enter takes it off");
                }
                Item::new(label, Pick::Macro(at)).meaning(meaning)
            })
            .collect();
        self.show_picker(Picker::new(MACRO_PICKER, items).opened_on(on));
        self.status = format!(
            "Enter maps {} to the macro chosen, from bottom to top to start with; a digit chooses at once",
            target.name_on(&track)
        );
    }

    /// Put the setting under the cursor on a macro, from its bottom to its top, or take it off
    /// the macro it's on, or move it to this one with its range, as one undo step. The first
    /// mapping of a macro holding nothing turns the macro to where the setting is, so nothing
    /// jumps.
    pub(crate) fn map_to_macro(&mut self, at: usize) {
        let Some(target) = self.setting_here() else {
            return;
        };
        let track = self.page_track();
        let name = target.name_on(&track);
        let index = self.cursor.track;
        let held = &track.macros[at];
        match track.macro_holding(target) {
            Some((on, _)) if on == at => {
                self.apply(&Action::UnmapMacro {
                    track: index,
                    target,
                });
                self.status = format!("{name} is off macro {}, and back to its own", at + 1);
            }
            Some((on, map)) => {
                self.apply(&Action::MapMacro {
                    track: index,
                    at,
                    map: *map,
                });
                self.status = format!(
                    "{name} moved from macro {} to macro {}, {}",
                    on + 1,
                    at + 1,
                    macro_name(&self.page_track().macros[at], at)
                );
            }
            None => {
                let (map, now) = match target.setting() {
                    Some(id) => (MacroMap::whole(target), track.value_of(id)),
                    None => {
                        let Some((bounds, now)) = self.gen_numbers() else {
                            return;
                        };
                        let now = (now + GEN_ZERO).clamp(0, i32::from(u16::MAX)) as u16;
                        (MacroMap::numbers(target, bounds), now)
                    }
                };
                let mut actions = vec![Action::MapMacro {
                    track: index,
                    at,
                    map,
                }];
                if held.maps.is_empty() {
                    actions.push(Action::SetMacro {
                        track: index,
                        at,
                        value: map.macro_for(now),
                    });
                }
                let first = held.maps.is_empty();
                self.apply(&Action::Batch(actions));
                let now = self.page_track();
                self.status = if first {
                    format!(
                        "{name} is on macro {}, {}, from its bottom to its top: gm for its range",
                        at + 1,
                        macro_name(&now.macros[at], at)
                    )
                } else {
                    format!(
                        "{name} is on macro {}, {}, and jumps to where the macro is: gm for its range",
                        at + 1,
                        macro_name(&now.macros[at], at)
                    )
                };
            }
        }
    }

    /// `gm`: open the mapping of the macro under the cursor, or of the one the setting under the
    /// cursor is on, on its row.
    pub(crate) fn open_mapping(&mut self) {
        let track = self.page_track();
        let (at, row) = match self.track_control() {
            Control::Macro(at) => (at, 0),
            _ => match self.macro_here() {
                Some((target, at, _)) => (at, track.macros[at].map_of(target).unwrap_or(0)),
                None => {
                    self.status =
                        "This isn't on a macro: M maps it to one, and gm on a macro opens it"
                            .to_owned();
                    return;
                }
            },
        };
        if track.macros[at].maps.is_empty() {
            self.status = format!("Macro {} holds nothing yet: M on a setting maps it", at + 1);
            return;
        }
        self.mapping = Some(MappingView {
            at,
            row,
            ..MappingView::default()
        });
        self.mode = Mode::Mapping;
        self.say_mapping();
    }

    /// The mapping under the mapping view's cursor, with its place, if the view is open on one.
    fn mapping_row(&self) -> Option<(usize, MacroMap)> {
        let view = self.mapping.as_ref()?;
        let maps = &self.page_track().macros[view.at].maps;
        let row = view.row.min(maps.len().checked_sub(1)?);
        Some((row, maps[row]))
    }

    /// Say on the status line what the mapping under the cursor does.
    fn say_mapping(&mut self) {
        let Some(view) = self.mapping.clone() else {
            return;
        };
        let Some((_, map)) = self.mapping_row() else {
            return;
        };
        let track = self.page_track();
        let target = map.target;
        let held = &track.macros[view.at];
        let now = map.at(f32::from(held.value) / 256.0);
        let by_hand = if target.by_hand() {
            ", by hand: a turn writes it in, but locks and modulation don't reach a list setting"
        } else {
            ""
        };
        if view.field.bends() {
            self.status = format!(
                "Curve {} bends {}'s way from Min to Max, above 0 early and below late; S {} eases both ends, above 0 an S",
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
        self.status = format!(
            "At 00 the macro sets {} to {}, at FF to {}{along}; it's at {:02X}, so {} is {}{by_hand}",
            target.name_on(&track),
            end_shown(&track, target, map.min),
            end_shown(&track, target, map.max),
            held.value >> 8,
            target.name(),
            end_shown(&track, target, now)
        );
    }

    /// Put the mapping under the cursor's range at these ends.
    fn set_range(&mut self, map: MacroMap) {
        let Some(view) = self.mapping.clone() else {
            return;
        };
        self.apply(&Action::MapMacro {
            track: self.cursor.track,
            at: view.at,
            map,
        });
        self.say_mapping();
    }

    /// `=` `-` `+` or `_`: move what's under the cursor up or down a small step or a large one,
    /// an end by a coarse step or `10` of them, and a bend by 5 or 25, as the shape panel's are.
    fn nudge_mapping(&mut self, up: bool, large: bool) {
        let Some(view) = self.mapping.as_ref() else {
            return;
        };
        let sign = if up { 1 } else { -1 };
        if view.field.bends() {
            self.move_bend(sign * if large { BEND_JUMP } else { BEND_NUDGE });
        } else {
            self.move_end(sign * if large { RANGE_JUMP } else { 1 });
        }
    }

    /// Put the bend under the cursor, `Curve` or `S`, at a value, kept to -100 to 100.
    fn set_bend(&mut self, to: i32) {
        let (Some(view), Some((_, map))) = (self.mapping.clone(), self.mapping_row()) else {
            return;
        };
        let (curve, s) = (i32::from(map.curve), i32::from(map.s));
        self.set_range(match view.field {
            MapField::Curve => map.bent(to, s),
            _ => map.bent(curve, to),
        });
    }

    /// Move the bend under the cursor by so much.
    fn move_bend(&mut self, by: i32) {
        let (Some(view), Some((_, map))) = (self.mapping.clone(), self.mapping_row()) else {
            return;
        };
        let now = if view.field == MapField::Curve {
            map.curve
        } else {
            map.s
        };
        self.set_bend(i32::from(now) + by);
    }

    /// Move the end of the range under the cursor by so many coarse steps.
    fn move_end(&mut self, by: i32) {
        let (Some(view), Some((_, map))) = (self.mapping.clone(), self.mapping_row()) else {
            return;
        };
        let max = view.field == MapField::Max;
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
        self.set_range(map);
    }

    /// A key in the mapping view: `j` and `k` go between its settings, `h` and `l` across their
    /// `Min`, `Max`, `Curve` and `S`, `=` `-` `+` and `_` move the one under the cursor, `i` types
    /// it, an end in hex and a bend as a signed number, `x` puts a bend back to straight, `~`
    /// swaps the ends, `dd` unmaps, `u` takes the last change back, and `Esc` closes it.
    pub(crate) fn mapping_key(&mut self, key: Key) {
        let key = match (key.command, crate::input::bindings::adjusting_key(key)) {
            (true, Some(plain)) => plain,
            _ => key,
        };
        let Some(view) = self.mapping.clone() else {
            self.mode = Mode::Normal;
            return;
        };
        if let Some(typed) = view.typing.clone() {
            self.mapping_typing(key.code, typed);
            return;
        }
        let count = self.page_track().macros[view.at].maps.len();
        let deleting = view.deleting;
        if let Some(view) = self.mapping.as_mut() {
            view.deleting = false;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mapping = None;
                self.mode = Mode::Normal;
                self.status.clear();
            }
            KeyCode::Char('j') | KeyCode::Down => self.mapping_move(|row| (row + 1).min(count - 1)),
            KeyCode::Char('k') | KeyCode::Up => self.mapping_move(|row| row.saturating_sub(1)),
            KeyCode::Char('h') | KeyCode::Left => self.mapping_across(-1),
            KeyCode::Char('l') | KeyCode::Right => self.mapping_across(1),
            KeyCode::Char('=') => self.nudge_mapping(true, false),
            KeyCode::Char('-') => self.nudge_mapping(false, false),
            KeyCode::Char('+') => self.nudge_mapping(true, true),
            KeyCode::Char('_') => self.nudge_mapping(false, true),
            KeyCode::Char('x') if view.field.bends() => self.set_bend(0),
            KeyCode::Char('~') => {
                if let Some((_, map)) = self.mapping_row() {
                    self.set_range(MacroMap {
                        min: map.max,
                        max: map.min,
                        ..map
                    });
                }
            }
            KeyCode::Char('i') => {
                if let Some(view) = self.mapping.as_mut() {
                    view.typing = Some(String::new());
                }
                self.status = if view.field.bends() {
                    "From -100 to 100, then Enter".to_owned()
                } else {
                    "Two hex digits for this end".to_owned()
                };
            }
            KeyCode::Char('d') if deleting => self.unmap_row(),
            KeyCode::Char('d') => {
                if let Some(view) = self.mapping.as_mut() {
                    view.deleting = true;
                }
            }
            KeyCode::Char('u') => {
                self.run(Cmd::Undo);
                self.keep_mapping_open();
            }
            _ => {}
        }
    }

    /// What's typed after `i`: hex digits for an end, set at the second, or a bend's signed number
    /// ended with `Enter`, as the shape panel's is; `Esc` gives up.
    fn mapping_typing(&mut self, code: KeyCode, mut typed: String) {
        let done = |app: &mut Self| {
            if let Some(view) = app.mapping.as_mut() {
                view.typing = None;
            }
        };
        let bends = self.mapping.as_ref().is_some_and(|view| view.field.bends());
        let keep = |app: &mut Self, typed: String| {
            app.status = format!("{typed}_");
            if let Some(view) = app.mapping.as_mut() {
                view.typing = Some(typed);
            }
        };
        match code {
            KeyCode::Esc => {
                done(self);
                self.say_mapping();
            }
            KeyCode::Backspace if bends => {
                typed.pop();
                keep(self, typed);
            }
            KeyCode::Enter if bends => {
                done(self);
                match typed.parse::<i32>() {
                    Ok(value) => self.set_bend(value),
                    Err(_) => self.status = "Curve and S run from -100 to 100".to_owned(),
                }
            }
            KeyCode::Char(c)
                if bends
                    && typed.len() < BEND_TEXT
                    && (c.is_ascii_digit() || (typed.is_empty() && matches!(c, '-' | '+'))) =>
            {
                typed.push(c);
                keep(self, typed);
            }
            KeyCode::Char(c) if !bends && c.is_ascii_hexdigit() => {
                typed.push(c.to_ascii_uppercase());
                if typed.len() < 2 {
                    if let Some(view) = self.mapping.as_mut() {
                        view.typing = Some(typed.clone());
                    }
                    self.status = format!("{typed}_");
                    return;
                }
                done(self);
                let (Some(view), Some((_, map))) = (self.mapping.clone(), self.mapping_row())
                else {
                    return;
                };
                let typed = u8::from_str_radix(&typed, 16).unwrap_or(0);
                let value = match map.target.setting() {
                    Some(id) => (u16::from(typed) << 8).min(top_value(id.entry().kind)),
                    // A generator's setting typed as a number, in hex as every value is.
                    None => (i32::from(typed) + GEN_ZERO) as u16,
                };
                self.set_range(if view.field == MapField::Max {
                    MacroMap { max: value, ..map }
                } else {
                    MacroMap { min: value, ..map }
                });
            }
            _ => {}
        }
    }

    fn mapping_move(&mut self, to: impl Fn(usize) -> usize) {
        if let Some(view) = self.mapping.as_mut() {
            view.row = to(view.row);
        }
        self.say_mapping();
    }

    fn mapping_across(&mut self, by: isize) {
        if let Some(view) = self.mapping.as_mut() {
            view.field = view.field.beside(by);
        }
        self.say_mapping();
    }

    /// `dd`: take the setting under the cursor off the macro, closing the view once it holds
    /// nothing.
    fn unmap_row(&mut self) {
        let Some((_, map)) = self.mapping_row() else {
            return;
        };
        let track = self.page_track();
        let name = map.target.name_on(&track);
        self.apply(&Action::UnmapMacro {
            track: self.cursor.track,
            target: map.target,
        });
        self.keep_mapping_open();
        if self.mapping.is_some() {
            self.status = format!("{name} is off the macro: u brings it back");
        } else {
            self.status =
                format!("{name} is off the macro, which holds nothing now: u brings it back");
        }
    }

    /// Keep the view's cursor on a mapping after a change, closing it if the macro holds none.
    fn keep_mapping_open(&mut self) {
        let Some(view) = self.mapping.as_mut() else {
            return;
        };
        let count = self
            .history
            .current()
            .tracks
            .get(self.cursor.track)
            .map_or(0, |track| track.macros[view.at].maps.len());
        if count == 0 {
            self.mapping = None;
            self.mode = Mode::Normal;
        } else {
            view.row = view.row.min(count - 1);
            self.mode = Mode::Mapping;
        }
    }

    /// `:macro Name`: name the macro under the cursor, the mapping view's or the one the setting
    /// under the cursor is on; `:macro` alone gives it back its first mapping's name.
    pub(crate) fn name_macro(&mut self, name: &str) {
        let at = match (&self.mapping, self.track_control()) {
            (Some(view), _) => Some(view.at),
            (None, Control::Macro(at)) => Some(at),
            (None, _) => self.macro_here().map(|(_, at, _)| at),
        };
        let Some(at) =
            at.filter(|_| self.view == crate::input::mode::View::Track || self.mapping.is_some())
        else {
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
