//! The Song sub-page's edits: `=` `-` `+` `_`, `x`, `K`, `i` and what's typed, and `H` and `L`.
//! Each row changes its value as its `:` command does, so most edits ask for that command's
//! [`Line`] to run, in [`Outcome::Run`]; the rest change the song, the scale or the groove drawn
//! through the [`Services`], or ask for a picker, the explorer or the Audio sub-page.

use std::sync::Arc;

use ratatui::crossterm::event::KeyCode;
use vt_model::action::Action;
use vt_model::constants::{
    BAR_STEPS, BEAT_STEPS, DEFAULT_MILLIBPM, MAX_MILLIBPM, MAX_PROJECT_NAME_CHARS, MIN_MILLIBPM,
};
use vt_model::groove::{
    CUSTOM_GROOVES, FULL_VELOCITY, Groove, GrooveId, MAX_GROOVE_STEPS, MAX_GROOVE_VELOCITY,
    MAX_HUMANISE, MAX_PUSH,
};
use vt_model::names::is_project_name;
use vt_model::scale::{ROOTS, Scale, parse_root};
use vt_model::song::Launch;

use super::{
    BPM_JUMP, BPM_NUDGE, FINE_JUMP, FINE_NUDGE, HUMANISE_JUMP, LONGEST_TYPED, PROJECT_PAGES,
    PUSH_JUMP, ProjectControl, ProjectPage, SEED_JUMP, VELOCITY_JUMP, launch_steps, launches,
    next_in, scale_here, scales, with_fraction,
};
use crate::app::{Entry, GRID_SIZES};
use crate::input::bindings::Cmd;
use crate::input::keys::Key;
use crate::input::mode::View;
use crate::input::wheel::Places;
use crate::overlays::command::parse::{Line, parse};
use crate::overlays::picker::explorer::Purpose;
use crate::overlays::picker::exploring::Naming;
use crate::overlays::picker::{Item, Pick, Picker};
use crate::pages::Outcome;
use crate::screen::status::grid_name;
use crate::services::Services;

/// The sub-page beside `view` on the project page, or `view` itself at either end.
pub(crate) fn page_beside(view: View, next: bool) -> View {
    let at = PROJECT_PAGES
        .iter()
        .position(|page| *page == view)
        .unwrap_or(0);
    let to = if next { at + 1 } else { at.saturating_sub(1) };
    PROJECT_PAGES[to.min(PROJECT_PAGES.len() - 1)]
}

/// A command line typed as the page's own change, to run, saying why if it doesn't parse.
fn typed_line(text: &str, services: &mut Services) -> Outcome {
    match parse(text) {
        Ok(line) => Outcome::Run(line),
        Err(error) => {
            services.status = error.to_string();
            Outcome::Done
        }
    }
}

impl ProjectPage {
    /// Run a key the page's moves didn't take: an edit on the setting under the cursor, or `H`
    /// and `L`, from the `view` on screen, or pass it back.
    pub(crate) fn edit(
        &mut self,
        cmd: Cmd,
        times: usize,
        view: View,
        services: &mut Services,
    ) -> Outcome {
        match cmd {
            Cmd::Increase => self.adjust(true, times, false, services),
            Cmd::Decrease => self.adjust(false, times, false, services),
            Cmd::IncreaseLarge => self.adjust(true, times, true, services),
            Cmd::DecreaseLarge => self.adjust(false, times, true, services),
            Cmd::ClearField => self.reset_control(services),
            Cmd::PickValue => self.choose(services),
            Cmd::EnterInsert => self.type_control(services),
            Cmd::PreviousPage | Cmd::NextPage => {
                Outcome::Show(page_beside(view, cmd == Cmd::NextPage))
            }
            _ => Outcome::Passed,
        }
    }

    /// The places of the setting under the cursor for the mouse wheel, where it's a list or off
    /// and on.
    pub(crate) fn wheel_places(&self, services: &Services) -> Option<Places> {
        let song = services.song();
        let live = &services.live;
        let place = |at: Option<usize>, count: usize| Some(Places::of(at, count));
        match self.control(song) {
            ProjectControl::Groove => place(Some(song.groove.index()), GrooveId::all().count()),
            ProjectControl::GrooveShown => place(
                Some(self.shown_groove(song).0.index()),
                GrooveId::all().count(),
            ),
            ProjectControl::Launch => {
                let named = launches();
                place(
                    named.iter().position(|launch| *launch == song.launch),
                    named.len(),
                )
            }
            ProjectControl::Grid => Some(Places {
                up: GRID_SIZES.iter().any(|size| *size > song.skip),
                down: GRID_SIZES.iter().any(|size| *size < song.skip),
            }),
            ProjectControl::Scale => {
                let scales = scales(live.scale);
                let here = scale_here(live.scale);
                place(scales.iter().position(|scale| *scale == here), scales.len())
            }
            ProjectControl::Root => place(Some(usize::from(live.scale.root)), ROOTS.len()),
            ProjectControl::AutoJump => Some(Places::switch(live.auto_jump)),
            ProjectControl::ReadOnly => Some(Places::switch(live.read_only)),
            _ => None,
        }
    }

    /// `=` `-` `+` `_`: move the value under the cursor, as its command would set it.
    fn adjust(&mut self, up: bool, count: usize, large: bool, services: &mut Services) -> Outcome {
        let song = Arc::clone(services.song());
        let sign = if up { 1 } else { -1 };
        let count = count.min(0x1000) as i64;
        match self.control(&song) {
            ProjectControl::Bpm => {
                let size = match (self.fine, large) {
                    (true, false) => FINE_NUDGE,
                    (true, true) => FINE_JUMP,
                    (false, false) => BPM_NUDGE,
                    (false, true) => BPM_JUMP,
                };
                let moved = (i64::from(song.millibpm) + sign * size * count)
                    .clamp(i64::from(MIN_MILLIBPM), i64::from(MAX_MILLIBPM));
                return Outcome::Run(Line::Bpm(moved as u32));
            }
            ProjectControl::Groove => {
                // One at a time, as a large step would skip through the list.
                let ids: Vec<GrooveId> = GrooveId::all().collect();
                let at = song.groove.index();
                let to = if up {
                    (at + 1).min(ids.len() - 1)
                } else {
                    at.saturating_sub(1)
                };
                if to != at {
                    return Outcome::Run(Line::Groove(ids[to]));
                }
            }
            ProjectControl::GrooveShown => {
                let (id, _) = self.shown_groove(&song);
                let last = GrooveId::all().count() - 1;
                let to = if up {
                    (id.index() + 1).min(last)
                } else {
                    id.index().saturating_sub(1)
                };
                self.groove_shown = GrooveId::from_index(to);
            }
            control @ (ProjectControl::GrooveSteps
            | ProjectControl::Humanise
            | ProjectControl::Push(_)
            | ProjectControl::Velocity(_)) => {
                let (_, groove) = self.shown_groove(&song);
                let by = |small: i64, jump: i64| sign * if large { jump } else { small } * count;
                let shaped = match control {
                    ProjectControl::GrooveSteps => {
                        let size = if large { BEAT_STEPS as i64 } else { 1 };
                        let steps = (groove.steps() as i64 + sign * size * count)
                            .clamp(1, MAX_GROOVE_STEPS as i64);
                        groove.with_steps(steps as usize)
                    }
                    ProjectControl::Humanise => {
                        let ticks = i64::from(groove.humanise()) + by(1, HUMANISE_JUMP);
                        groove.with_humanise(ticks.clamp(0, i64::from(MAX_HUMANISE)) as u8)
                    }
                    ProjectControl::Push(step) => {
                        let step = usize::from(step);
                        let push = i64::from(groove.push(step)) + by(1, PUSH_JUMP);
                        let push = push.clamp(-i64::from(MAX_PUSH), i64::from(MAX_PUSH));
                        groove.with_push(step, push as i16)
                    }
                    ProjectControl::Velocity(step) => {
                        let step = usize::from(step);
                        let velocity = i64::from(groove.velocity(step)) + by(1, VELOCITY_JUMP);
                        let velocity = velocity.clamp(0, i64::from(MAX_GROOVE_VELOCITY));
                        groove.with_velocity(step, velocity as u8)
                    }
                    _ => groove,
                };
                self.shape_groove(control, shaped, services);
            }
            ProjectControl::Launch => {
                // One way at a time, as a large step would skip through a short list. A grid with
                // no name goes to the named one beside it.
                let named = launches();
                let here = song.launch;
                let next = if named.contains(&here) {
                    let at = named.iter().position(|launch| *launch == here).unwrap_or(0);
                    let to = if up {
                        (at + 1).min(named.len() - 1)
                    } else {
                        at.saturating_sub(1)
                    };
                    named[to]
                } else if up {
                    let found = named
                        .iter()
                        .find(|l| launch_steps(**l) > launch_steps(here));
                    found.copied().unwrap_or(here)
                } else {
                    let found = named
                        .iter()
                        .rfind(|l| launch_steps(**l) < launch_steps(here));
                    found.copied().unwrap_or(here)
                };
                if next != here {
                    return Outcome::Run(Line::Launch(Some(next)));
                }
            }
            ProjectControl::Reset => {
                let section = services.cursor.section;
                let from = song
                    .sections
                    .get(section)
                    .and_then(|found| found.reset)
                    .unwrap_or(0) as i64;
                let size = if large { BAR_STEPS as i64 } else { 1 };
                let moved = (from + sign * size * count).max(0) as usize;
                return Outcome::Run(Line::Reset((moved > 0).then_some(moved)));
            }
            ProjectControl::Seed => {
                let size = if large { SEED_JUMP } else { 1 } * count as u64;
                let seed = if up {
                    song.seed.saturating_add(size)
                } else {
                    song.seed.saturating_sub(size)
                };
                return Outcome::Run(Line::Seed(seed));
            }
            ProjectControl::Scale => {
                let scale = next_in(
                    &scales(services.live.scale),
                    scale_here(services.live.scale),
                    up,
                );
                services.set_scale(scale);
            }
            ProjectControl::Root => {
                let roots: Vec<u8> = (0..ROOTS.len() as u8).collect();
                let root = next_in(&roots, services.live.scale.root, up);
                services.set_scale(Scale {
                    root,
                    ..services.live.scale
                });
            }
            ProjectControl::Grid => {
                let skip = song.skip;
                let next = if up {
                    GRID_SIZES.iter().find(|size| **size > skip)
                } else {
                    GRID_SIZES.iter().rfind(|size| **size < skip)
                };
                if let Some(next) = next {
                    return Outcome::Run(Line::SetSkip(*next));
                }
            }
            ProjectControl::AutoJump => {
                if services.live.auto_jump != up {
                    services.toggle_auto_jump();
                }
            }
            ProjectControl::ReadOnly => {
                if services.live.read_only != up {
                    return Outcome::Run(Line::ReadOnly(up));
                }
            }
            ProjectControl::Name => {
                services.status =
                    "i renames the project, and Ctrl-r there rolls a new name".to_owned();
            }
            ProjectControl::Folder => {
                services.status = "Save as puts the project in another folder".to_owned();
            }
            ProjectControl::Audio => {
                services.status =
                    "Enter or L opens the Audio page, to choose the device".to_owned();
            }
            ProjectControl::Save
            | ProjectControl::SaveAs
            | ProjectControl::Render
            | ProjectControl::Open
            | ProjectControl::New => {
                services.status = "Enter does this".to_owned();
            }
        }
        Outcome::Done
    }

    /// `x`: put the value under the cursor back as a new project has it. If it's there already,
    /// nothing changes and no undo step is left.
    fn reset_control(&mut self, services: &mut Services) -> Outcome {
        let song = Arc::clone(services.song());
        let line = match self.control(&song) {
            ProjectControl::Bpm if self.fine => {
                let whole = song.millibpm / 1000 * 1000;
                (whole != song.millibpm).then_some(Line::Bpm(whole))
            }
            ProjectControl::Bpm if song.millibpm != DEFAULT_MILLIBPM => {
                Some(Line::Bpm(DEFAULT_MILLIBPM))
            }
            ProjectControl::Groove if song.groove != GrooveId::STRAIGHT => {
                Some(Line::Groove(GrooveId::STRAIGHT))
            }
            ProjectControl::GrooveShown => {
                self.groove_shown = None;
                None
            }
            control if control.shapes_groove() => {
                let (_, groove) = self.shown_groove(&song);
                let shaped = match control {
                    ProjectControl::GrooveSteps => groove.with_steps(MAX_GROOVE_STEPS),
                    ProjectControl::Humanise => groove.with_humanise(0),
                    ProjectControl::Push(step) => groove.with_push(usize::from(step), 0),
                    ProjectControl::Velocity(step) => {
                        groove.with_velocity(usize::from(step), FULL_VELOCITY)
                    }
                    _ => groove,
                };
                self.shape_groove(control, shaped, services);
                None
            }
            ProjectControl::Launch if song.launch != Launch::default() => {
                Some(Line::Launch(Some(Launch::default())))
            }
            ProjectControl::Reset if services.section_here().is_some_and(|s| s.reset.is_some()) => {
                Some(Line::Reset(None))
            }
            ProjectControl::Seed if song.seed != 0 => Some(Line::Seed(0)),
            ProjectControl::Scale if services.live.scale.on => {
                services.set_scale(Scale {
                    on: false,
                    ..services.live.scale
                });
                None
            }
            ProjectControl::Root if services.live.scale.root != 0 => {
                services.set_scale(Scale {
                    root: 0,
                    ..services.live.scale
                });
                None
            }
            ProjectControl::Grid if song.skip != BEAT_STEPS as u32 => {
                Some(Line::SetSkip(BEAT_STEPS as u32))
            }
            ProjectControl::AutoJump if services.live.auto_jump => {
                services.toggle_auto_jump();
                None
            }
            ProjectControl::ReadOnly if services.live.read_only => Some(Line::ReadOnly(false)),
            ProjectControl::Name => {
                services.status =
                    "i renames the project, and Ctrl-r there rolls a new name".to_owned();
                None
            }
            _ => None,
        };
        line.map_or(Outcome::Done, Outcome::Run)
    }

    /// `K` and `Enter`: list the choices, switch something on or off, open the audio page, or run
    /// an action.
    fn choose(&self, services: &mut Services) -> Outcome {
        let song = Arc::clone(services.song());
        let control = self.control(&song);
        let items: Vec<Item>;
        let (title, current) = match control {
            ProjectControl::Launch => {
                let here = song.launch;
                let named = launches();
                items = named
                    .iter()
                    .map(|launch| {
                        Item::new(launch.to_string(), Pick::Line(Line::Launch(Some(*launch))))
                    })
                    .collect();
                let at = named.iter().position(|launch| *launch == here);
                ("Launch. i types a number of steps", at)
            }
            ProjectControl::Scale => {
                let choices = scales(services.live.scale);
                let here = scale_here(services.live.scale);
                items = choices
                    .iter()
                    .map(|scale| {
                        let name = if scale.on { scale.kind.name() } else { "off" };
                        Item::new(name, Pick::ScaleTo(*scale))
                    })
                    .collect();
                ("Scale", choices.iter().position(|scale| *scale == here))
            }
            ProjectControl::Root => {
                items = (0..ROOTS.len() as u8)
                    .map(|root| {
                        let scale = Scale {
                            root,
                            ..services.live.scale
                        };
                        Item::new(scale.root_name(), Pick::ScaleTo(scale))
                    })
                    .collect();
                ("Root", Some(usize::from(services.live.scale.root)))
            }
            ProjectControl::Grid => {
                let skip = song.skip;
                items = GRID_SIZES
                    .iter()
                    .map(|size| Item::new(grid_name(*size), Pick::Line(Line::SetSkip(*size))))
                    .collect();
                let at = GRID_SIZES.iter().position(|size| *size == skip);
                ("Grid. i types any number of steps", at)
            }
            ProjectControl::Groove => {
                return Outcome::Open(Box::new(services.groove_picker(false)));
            }
            ProjectControl::GrooveShown => {
                let shown = self.shown_groove(&song).0;
                items = GrooveId::all()
                    .map(|id| Item::new(id.name(), Pick::ShowGroove(id)).meaning(id.meaning()))
                    .collect();
                ("The groove to draw and change", Some(shown.index()))
            }
            ProjectControl::AutoJump => {
                services.toggle_auto_jump();
                return Outcome::Done;
            }
            ProjectControl::ReadOnly => {
                return Outcome::Run(Line::ReadOnly(!services.live.read_only));
            }
            ProjectControl::Audio => return Outcome::Jump(View::Audio),
            ProjectControl::Save => {
                return Outcome::Run(Line::Write {
                    path: None,
                    force: false,
                });
            }
            ProjectControl::SaveAs => {
                return Outcome::Explore(Naming {
                    purpose: Purpose::Save,
                    command: "w",
                    after: String::new(),
                });
            }
            ProjectControl::Render => {
                return Outcome::Run(Line::Render {
                    path: None,
                    rows: None,
                    tail: false,
                    stems: false,
                    each_track: false,
                });
            }
            ProjectControl::Open => {
                return Outcome::Run(Line::Edit {
                    path: None,
                    force: false,
                });
            }
            ProjectControl::New => {
                return Outcome::Run(Line::New {
                    path: None,
                    force: false,
                });
            }
            ProjectControl::Name
            | ProjectControl::Folder
            | ProjectControl::Bpm
            | ProjectControl::Reset
            | ProjectControl::Seed
            | ProjectControl::GrooveSteps
            | ProjectControl::Humanise
            | ProjectControl::Push(_)
            | ProjectControl::Velocity(_) => {
                services.status =
                    "K lists the choices on Groove, Edit, Launch, Scale, Root and Grid; i types the rest"
                        .to_owned();
                return Outcome::Done;
            }
        };
        Outcome::Open(Box::new(Picker::new(title, items).opened_on(current)))
    }

    /// `i`: type a value, or for what's chosen, switched or run, the same as `K`. A name starts as
    /// it is, to change, and anything else starts empty.
    fn type_control(&self, services: &mut Services) -> Outcome {
        let control = self.control(services.song());
        let typed = matches!(
            control,
            ProjectControl::Name
                | ProjectControl::Bpm
                | ProjectControl::Launch
                | ProjectControl::Reset
                | ProjectControl::Seed
                | ProjectControl::Root
                | ProjectControl::Grid
                | ProjectControl::GrooveSteps
                | ProjectControl::Humanise
                | ProjectControl::Push(_)
                | ProjectControl::Velocity(_)
        );
        if control.is_choice() || !typed {
            if control == ProjectControl::Folder {
                services.status = "Save as puts the project in another folder".to_owned();
                return Outcome::Done;
            }
            return self.choose(services);
        }
        let mut entry = Entry::default();
        if control == ProjectControl::Name {
            entry.text = services.project_name().into();
        }
        Outcome::Insert(entry)
    }

    /// A key while typing on the page, into what Insert has typed: `Enter` sets what's typed,
    /// `Esc` leaves it, and on the name `C-r` rolls a new random one and `C-u` clears it.
    /// Gives what ending Insert does, once it ends.
    pub(crate) fn insert(
        &mut self,
        key: Key,
        entry: &mut Entry,
        services: &mut Services,
    ) -> Option<Outcome> {
        let control = self.control(services.song());
        match key.code {
            KeyCode::Esc => {
                *entry = Entry::default();
                return Some(Outcome::Done);
            }
            KeyCode::Char('r') if key.ctrl && control == ProjectControl::Name => {
                let taken = services
                    .folder
                    .as_deref()
                    .and_then(std::path::Path::parent)
                    .map_or_else(
                        std::collections::HashSet::new,
                        crate::overlays::picker::exploring::taken_in,
                    );
                entry.text = services.names.fresh(|name| taken.contains(name)).into();
            }
            KeyCode::Char('u') if key.ctrl => entry.text.clear(),
            KeyCode::Backspace => {
                entry.text.pop();
            }
            KeyCode::Enter => {
                let text = entry.text.take();
                return Some(self.commit(control, text.trim(), services));
            }
            KeyCode::Char(c) if !key.ctrl && !key.alt => {
                let room = if control == ProjectControl::Name {
                    MAX_PROJECT_NAME_CHARS
                } else {
                    LONGEST_TYPED
                };
                if entry.text.chars().count() < room {
                    entry.text.push(c);
                }
            }
            _ => {}
        }
        None
    }

    /// Set what was typed, through the row's command where it has one.
    fn commit(&mut self, control: ProjectControl, text: &str, services: &mut Services) -> Outcome {
        if text.is_empty() {
            return Outcome::Done;
        }
        match control {
            ProjectControl::Name if text == services.project_name() => {}
            ProjectControl::Name if is_project_name(text) => {
                services.apply(&Action::SetSongName(text.to_owned()));
                services.status = match &services.folder {
                    Some(_) => format!("Named {text}: Save as puts it in a folder of that name"),
                    None => format!("Named {text}"),
                };
            }
            ProjectControl::Name => {
                services.status = format!(
                    "A name is up to {MAX_PROJECT_NAME_CHARS} characters, with no / and not starting with a dot"
                );
            }
            ProjectControl::Bpm if self.fine => match with_fraction(services.song().millibpm, text)
            {
                Some(millibpm) => return Outcome::Run(Line::Bpm(millibpm)),
                None => services.status = "The fine part is the digits after the point".to_owned(),
            },
            ProjectControl::Bpm => return typed_line(&format!("bpm {text}"), services),
            control if control.shapes_groove() => self.commit_groove(control, text, services),
            ProjectControl::Launch => return typed_line(&format!("launch {text}"), services),
            ProjectControl::Reset => return typed_line(&format!("reset {text}"), services),
            ProjectControl::Seed => return typed_line(&format!("seed {text}"), services),
            ProjectControl::Grid => return typed_line(&format!("set skip={text}"), services),
            ProjectControl::Root => match parse_root(text) {
                Some(root) => services.set_scale(Scale {
                    root,
                    ..services.live.scale
                }),
                None => services.status = "A root is a note, such as d or f#".to_owned(),
            },
            _ => {}
        }
        Outcome::Done
    }

    /// Set a number typed into the groove drawn: its steps, its humanise in ticks, a push in
    /// ticks either way, or a velocity in percent.
    fn commit_groove(&mut self, control: ProjectControl, text: &str, services: &mut Services) {
        let (_, groove) = self.shown_groove(services.song());
        let number: Option<i64> = text
            .trim_start_matches('+')
            .trim_end_matches('%')
            .parse()
            .ok();
        let shaped = match (control, number) {
            (ProjectControl::GrooveSteps, Some(steps))
                if (1..=MAX_GROOVE_STEPS as i64).contains(&steps) =>
            {
                Some(groove.with_steps(steps as usize))
            }
            (ProjectControl::Humanise, Some(ticks))
                if (0..=i64::from(MAX_HUMANISE)).contains(&ticks) =>
            {
                Some(groove.with_humanise(ticks as u8))
            }
            (ProjectControl::Push(step), Some(push)) if push.abs() <= i64::from(MAX_PUSH) => {
                Some(groove.with_push(usize::from(step), push as i16))
            }
            (ProjectControl::Velocity(step), Some(velocity))
                if (0..=i64::from(MAX_GROOVE_VELOCITY)).contains(&velocity) =>
            {
                Some(groove.with_velocity(usize::from(step), velocity as u8))
            }
            _ => None,
        };
        match shaped {
            Some(shaped) => self.shape_groove(control, shaped, services),
            None => {
                services.status = match control {
                    ProjectControl::GrooveSteps => {
                        format!("A groove has 1 to {MAX_GROOVE_STEPS} steps")
                    }
                    ProjectControl::Humanise => format!("Humanise is 0 to {MAX_HUMANISE} ticks"),
                    ProjectControl::Push(_) => {
                        format!("A push is -{MAX_PUSH} to {MAX_PUSH} ticks, 240 to a step")
                    }
                    _ => format!("A velocity is 0 to {MAX_GROOVE_VELOCITY}%"),
                };
            }
        }
    }

    /// Keep a change to the groove drawn. One of the project's own changes in place. One that
    /// comes with the program is copied into the first of the project's not yet made, which the
    /// page then shows, and the project plays it if it played the one it came from, all as one
    /// undo step.
    fn shape_groove(&mut self, control: ProjectControl, shaped: Groove, services: &mut Services) {
        let (id, groove) = self.shown_groove(services.song());
        if control == ProjectControl::Push(0) {
            services.status =
                "The first step stays on the grid: the others are pushed from it".to_owned();
            return;
        }
        if shaped == groove {
            return;
        }
        if let Some(custom) = id.custom_number() {
            services.apply(&Action::SetCustomGroove {
                custom,
                groove: shaped,
            });
            return;
        }
        let custom = services.song().grooves.len();
        let Some(copy) = GrooveId::custom(custom) else {
            services.status = format!(
                "The project's {CUSTOM_GROOVES} grooves are all made: change one of them instead"
            );
            return;
        };
        let depth = services.history.undo_depth();
        services.apply(&Action::SetCustomGroove {
            custom,
            groove: shaped,
        });
        let plays = services.song().groove == id;
        if plays {
            services.apply(&Action::SetGroove(copy));
            services.history.squash_to(depth);
        }
        self.groove_shown = Some(copy);
        services.status = if plays {
            format!("{id} copied to {copy} to change it, which the project now plays")
        } else {
            format!("{id} copied to {copy} to change it")
        };
    }
}
