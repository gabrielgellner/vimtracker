//! What keys do on the project page: the song's own settings, its grooves, a row for the audio
//! page, and the actions that save, render and open projects. Each row changes its value as its
//! `:` command does, through the same parser, so the two can't disagree.

use ratatui::crossterm::event::KeyCode;
use std::sync::Arc;

use vt_model::action::Action;
use vt_model::constants::{
    BAR_STEPS, BEAT_STEPS, DEFAULT_MILLIBPM, MAX_MILLIBPM, MAX_PROJECT_NAME_CHARS, MIN_MILLIBPM,
};
use vt_model::groove::{
    CUSTOM_GROOVES, FULL_VELOCITY, Groove, GrooveId, MAX_GROOVE_STEPS, MAX_GROOVE_VELOCITY,
    MAX_HUMANISE, MAX_PUSH,
};
use vt_model::names::is_project_name;
use vt_model::scale::{ROOTS, Scale, ScaleKind, parse_root};
use vt_model::selection::Motion;
use vt_model::song::{LAUNCH_WORDS, Launch};

use crate::app::{App, Entry, GRID_SIZES};
use crate::explorer::Purpose;
use crate::input::bindings::Cmd;
use crate::input::command::{Line, parse};
use crate::input::keys::Key;
use crate::input::mode::{Mode, View};
use crate::input::picker::{Item, Pick, Picker};
use crate::views::status::grid_name;

/// How far a small and a large step move the tempo, in thousandths of a BPM, and its fine part.
const BPM_NUDGE: i64 = 1000;
const BPM_JUMP: i64 = 10_000;
const FINE_NUDGE: i64 = 10;
const FINE_JUMP: i64 = 100;
/// How far a large step moves the seed.
const SEED_JUMP: u64 = 10;
/// How far a large step moves a push, in ticks, a humanise, in ticks, and a velocity, in percent.
const PUSH_JUMP: i64 = 10;
const HUMANISE_JUMP: i64 = 4;
const VELOCITY_JUMP: i64 = 10;
/// The longest value typed on the page other than a name.
const LONGEST_TYPED: usize = 12;

/// One row's setting, or what it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectControl {
    Name,
    /// Where it's saved, and whether there are changes since. It only shows.
    Folder,
    Bpm,
    /// The groove every track plays with unless it has its own.
    Groove,
    Launch,
    /// When every track starts again, in the section the cursor is in.
    Reset,
    Seed,
    Scale,
    Root,
    Grid,
    AutoJump,
    /// The groove drawn below, to look at or change: the project's unless another is chosen.
    GrooveShown,
    /// How many steps it has, and how far either way its humanise moves them.
    GrooveSteps,
    Humanise,
    /// One step's push and velocity.
    Push(u8),
    Velocity(u8),
    /// The output playing, which opens the audio page.
    Audio,
    Save,
    SaveAs,
    Render,
    Open,
    New,
}

/// A field for each of a groove's steps, of which as many as it has are used.
const PUSHES: [ProjectControl; MAX_GROOVE_STEPS] = {
    let mut row = [ProjectControl::Audio; MAX_GROOVE_STEPS];
    let mut at = 0;
    while at < MAX_GROOVE_STEPS {
        row[at] = ProjectControl::Push(at as u8);
        at += 1;
    }
    row
};
const VELOCITIES: [ProjectControl; MAX_GROOVE_STEPS] = {
    let mut row = [ProjectControl::Audio; MAX_GROOVE_STEPS];
    let mut at = 0;
    while at < MAX_GROOVE_STEPS {
        row[at] = ProjectControl::Velocity(at as u8);
        at += 1;
    }
    row
};

/// The page's rows, top to bottom: how it plays on the left, and how notes go in on the right,
/// then the groove drawn and changed. The limiter isn't here: it's after the master's `Level`, on
/// the master's Sound page, where it's heard.
pub const PROJECT_ROWS: [&[ProjectControl]; 17] = [
    &[ProjectControl::Name],
    &[ProjectControl::Folder],
    &[ProjectControl::Bpm, ProjectControl::Scale],
    &[ProjectControl::Groove, ProjectControl::Root],
    &[ProjectControl::Launch, ProjectControl::Grid],
    &[ProjectControl::Reset, ProjectControl::AutoJump],
    &[ProjectControl::Seed],
    &[ProjectControl::GrooveShown],
    &[ProjectControl::GrooveSteps, ProjectControl::Humanise],
    &PUSHES,
    &VELOCITIES,
    &[ProjectControl::Audio],
    &[ProjectControl::Save],
    &[ProjectControl::SaveAs],
    &[ProjectControl::Render],
    &[ProjectControl::Open],
    &[ProjectControl::New],
];

impl ProjectControl {
    /// The row's name on screen.
    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Folder => "Folder",
            Self::Bpm => "BPM",
            Self::Groove => "Groove",
            Self::Launch => "Launch",
            Self::Reset => "Reset",
            Self::Seed => "Seed",
            Self::Scale => "Scale",
            Self::Root => "Root",
            Self::Grid => "Grid",
            Self::AutoJump => "Auto-jump",
            Self::GrooveShown => "Edit",
            Self::GrooveSteps => "Steps",
            Self::Humanise => "Humanise",
            Self::Push(_) => "Push",
            Self::Velocity(_) => "Velocity",
            Self::Audio => "Audio",
            Self::Save => "Save",
            Self::SaveAs => "Save as",
            Self::Render => "Render",
            Self::Open => "Open",
            Self::New => "New",
        }
    }

    /// What an action does, with the command that does the same, drawn dim beside it.
    pub fn meaning(self) -> Option<&'static str> {
        match self {
            Self::Save => Some(":w, to its folder, or choose one"),
            Self::SaveAs => Some(":w path, a copy in a new folder, to carry on in"),
            Self::Render => Some(":render, the loop to a WAV"),
            Self::Open => Some(":e, another project"),
            Self::New => Some(":new, an empty project"),
            _ => None,
        }
    }

    /// Whether this is something to do rather than a value.
    pub fn is_action(self) -> bool {
        self.meaning().is_some()
    }

    /// Which column it's in: how notes go in is on the right, and everything else on the left.
    pub fn column(self) -> usize {
        usize::from(matches!(
            self,
            Self::Scale | Self::Root | Self::Grid | Self::AutoJump | Self::Humanise
        ))
    }

    /// Whether `i` and `K` choose this from a list rather than typing it.
    fn is_choice(self) -> bool {
        matches!(self, Self::Scale | Self::Groove | Self::GrooveShown)
    }

    /// Whether it's part of the groove drawn below, which changing makes a groove of its own.
    fn shapes_groove(self) -> bool {
        matches!(
            self,
            Self::GrooveSteps | Self::Humanise | Self::Push(_) | Self::Velocity(_)
        )
    }
}

/// The next item in a list, going round.
fn next_in<T: Copy + PartialEq>(items: &[T], current: T, up: bool) -> T {
    let position = items.iter().position(|item| *item == current).unwrap_or(0);
    let step = if up { 1 } else { items.len() - 1 };
    items[(position + step) % items.len()]
}

/// Every launch `:launch` has a name for, in order, ending with launching when a chain ends.
fn launches() -> Vec<Launch> {
    LAUNCH_WORDS
        .iter()
        .filter_map(|word| Launch::named(word))
        .collect()
}

/// How many steps a launch waits for, with a chain's end after every grid.
fn launch_steps(launch: Launch) -> u64 {
    match launch {
        Launch::Every(steps) => u64::from(steps.get()),
        Launch::ChainEnd => u64::MAX,
    }
}

/// The scale's choices: off, then each kind on, keeping the root.
fn scales(scale: Scale) -> Vec<Scale> {
    std::iter::once(Scale { on: false, ..scale })
        .chain(ScaleKind::ALL.map(|kind| Scale {
            on: true,
            kind,
            ..scale
        }))
        .collect()
}

/// What the scale row shows and the list holds: off, or the kind of scale that's on.
fn scale_here(scale: Scale) -> Scale {
    if scale.on {
        scale
    } else {
        Scale {
            on: false,
            kind: ScaleKind::default(),
            ..scale
        }
    }
}

/// A tempo with its fine part moved, typed as the digits after the point: `5` is `.5`, and `63`
/// is `.63`.
fn with_fraction(millibpm: u32, digits: &str) -> Option<u32> {
    if digits.is_empty() || digits.len() > 3 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let thousandths: u32 = format!("{digits:0<3}").parse().ok()?;
    Some(millibpm / 1000 * 1000 + thousandths)
}

impl App {
    /// What the cursor is on.
    pub fn project_control(&self) -> ProjectControl {
        let row = self.project_row.min(PROJECT_ROWS.len() - 1);
        PROJECT_ROWS[row][self.project_field.min(self.project_row_length(row) - 1)]
    }

    /// How many fields a row has: a groove's rows have one for each of its steps.
    fn project_row_length(&self, row: usize) -> usize {
        let fields = PROJECT_ROWS[row];
        match fields.first() {
            Some(ProjectControl::Push(_) | ProjectControl::Velocity(_)) => {
                self.shown_groove().1.steps().min(fields.len())
            }
            _ => fields.len(),
        }
    }

    /// The groove the page draws, and its pushes.
    pub fn shown_groove(&self) -> (GrooveId, Groove) {
        let id = self.groove_shown.unwrap_or(self.song().groove);
        (id, self.song().groove(id))
    }

    /// Put the cursor on a row and a field of it, as near as there is. The field `j` and `k` aim
    /// for is this one from now on, as Vim's `0` and `l` set the column `j` keeps.
    fn move_project(&mut self, row: usize, field: usize) {
        self.project_wanted = None;
        self.project_row = row.min(PROJECT_ROWS.len() - 1);
        self.project_field = field.min(self.project_row_length(self.project_row) - 1);
        self.project_fine = false;
    }

    /// The field `j` and `k` aim for: the one before a shorter row moved the cursor in, if it hasn't
    /// moved since, as on the track page.
    fn project_wanted_field(&self) -> usize {
        match self.project_wanted {
            Some((row, field, wanted))
                if (row, field) == (self.project_row, self.project_field) =>
            {
                wanted
            }
            _ => self.project_field,
        }
    }

    /// Up or down to a row, aiming for the field `j` and `k` aim for.
    fn move_project_row(&mut self, row: usize) {
        let wanted = self.project_wanted_field();
        self.move_project(row, wanted);
        self.project_wanted = Some((self.project_row, self.project_field, wanted));
    }

    /// `l`: onto the tempo's fine part, and then on to the next.
    fn project_right(&mut self) {
        if !self.project_fine && self.project_control() == ProjectControl::Bpm {
            self.project_fine = true;
        } else {
            self.move_project(self.project_row, self.project_field.saturating_add(1));
        }
    }

    /// `h`: back off the fine part, and onto the tempo's from the setting beside it.
    fn project_left(&mut self) {
        if self.project_fine {
            self.project_fine = false;
            return;
        }
        let before = self.project_field;
        self.move_project(self.project_row, before.saturating_sub(1));
        self.project_fine =
            self.project_field != before && self.project_control() == ProjectControl::Bpm;
    }

    /// Run a command on the project page. Returns false if it means the same as anywhere.
    pub(crate) fn run_project_page(&mut self, cmd: Cmd) -> bool {
        let count = self.count.take();
        let times = count.unwrap_or(1).max(1);
        let last = PROJECT_ROWS.len() - 1;
        match cmd {
            Cmd::Move(Motion::Down) => {
                self.move_project_row(self.project_row.saturating_add(times).min(last));
            }
            Cmd::Move(Motion::Up) => {
                self.move_project_row(self.project_row.saturating_sub(times));
            }
            Cmd::Move(Motion::First) => {
                self.move_project_row(count.map_or(0, |row| row.saturating_sub(1)));
            }
            Cmd::Move(Motion::Last) => {
                self.move_project_row(count.map_or(last, |row| row.saturating_sub(1)).min(last));
            }
            Cmd::Move(Motion::Right) => (0..times).for_each(|_| self.project_right()),
            Cmd::Move(Motion::Left) => (0..times).for_each(|_| self.project_left()),
            Cmd::Move(Motion::RowStart) => self.move_project(self.project_row, 0),
            Cmd::Move(Motion::RowEnd) => self.move_project(self.project_row, usize::MAX),
            Cmd::Increase => self.adjust_project(true, times, false),
            Cmd::Decrease => self.adjust_project(false, times, false),
            Cmd::IncreaseLarge => self.adjust_project(true, times, true),
            Cmd::DecreaseLarge => self.adjust_project(false, times, true),
            Cmd::ClearField => self.reset_project_control(),
            Cmd::PickValue => self.choose_project_control(),
            Cmd::EnterInsert => self.type_project_control(),
            _ => {
                self.count = count;
                return false;
            }
        }
        true
    }

    /// Run a command line as the page's own change, saying why if it doesn't parse.
    fn project_line(&mut self, text: &str) {
        match parse(text) {
            Ok(line) => self.line(line),
            Err(error) => self.status = error.to_string(),
        }
    }

    /// `=` `-` `+` `_`: move the value under the cursor, as its command would set it.
    fn adjust_project(&mut self, up: bool, count: usize, large: bool) {
        let song = Arc::clone(self.song());
        let sign = if up { 1 } else { -1 };
        let count = count.min(0x1000) as i64;
        match self.project_control() {
            ProjectControl::Bpm => {
                let size = match (self.project_fine, large) {
                    (true, false) => FINE_NUDGE,
                    (true, true) => FINE_JUMP,
                    (false, false) => BPM_NUDGE,
                    (false, true) => BPM_JUMP,
                };
                let moved = (i64::from(song.millibpm) + sign * size * count)
                    .clamp(i64::from(MIN_MILLIBPM), i64::from(MAX_MILLIBPM));
                self.line(Line::Bpm(moved as u32));
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
                    self.line(Line::Groove(ids[to]));
                }
            }
            ProjectControl::GrooveShown => {
                let (id, _) = self.shown_groove();
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
                let (_, groove) = self.shown_groove();
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
                self.shape_groove(control, shaped);
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
                    self.line(Line::Launch(Some(next)));
                }
            }
            ProjectControl::Reset => {
                let section = self.cursor.section;
                let from = song
                    .sections
                    .get(section)
                    .and_then(|found| found.reset)
                    .unwrap_or(0) as i64;
                let size = if large { BAR_STEPS as i64 } else { 1 };
                let moved = (from + sign * size * count).max(0) as usize;
                self.line(Line::Reset((moved > 0).then_some(moved)));
            }
            ProjectControl::Seed => {
                let size = if large { SEED_JUMP } else { 1 } * count as u64;
                let seed = if up {
                    song.seed.saturating_add(size)
                } else {
                    song.seed.saturating_sub(size)
                };
                self.line(Line::Seed(seed));
            }
            ProjectControl::Scale => {
                let scale = next_in(&scales(self.scale), scale_here(self.scale), up);
                self.set_scale(scale);
            }
            ProjectControl::Root => {
                let roots: Vec<u8> = (0..ROOTS.len() as u8).collect();
                let root = next_in(&roots, self.scale.root, up);
                self.set_scale(Scale { root, ..self.scale });
            }
            ProjectControl::Grid => {
                let skip = song.skip;
                let next = if up {
                    GRID_SIZES.iter().find(|size| **size > skip)
                } else {
                    GRID_SIZES.iter().rfind(|size| **size < skip)
                };
                if let Some(next) = next {
                    self.line(Line::SetSkip(*next));
                }
            }
            ProjectControl::AutoJump => {
                if self.auto_jump != up {
                    self.toggle_auto_jump();
                }
            }
            ProjectControl::Name => {
                self.status = "i renames the project, and C-r there rolls a new name".to_owned();
            }
            ProjectControl::Folder => {
                self.status = "Save as puts the project in another folder".to_owned();
            }
            ProjectControl::Audio => {
                self.status = "Enter opens the audio page, to choose the device".to_owned();
            }
            ProjectControl::Save
            | ProjectControl::SaveAs
            | ProjectControl::Render
            | ProjectControl::Open
            | ProjectControl::New => {
                self.status = "Enter does this".to_owned();
            }
        }
    }

    /// `x`: put the value under the cursor back as a new project has it. If it's there already,
    /// nothing changes and no undo step is left.
    fn reset_project_control(&mut self) {
        let song = Arc::clone(self.song());
        match self.project_control() {
            ProjectControl::Bpm if self.project_fine => {
                let whole = song.millibpm / 1000 * 1000;
                if whole != song.millibpm {
                    self.line(Line::Bpm(whole));
                }
            }
            ProjectControl::Bpm if song.millibpm != DEFAULT_MILLIBPM => {
                self.line(Line::Bpm(DEFAULT_MILLIBPM));
            }
            ProjectControl::Groove if song.groove != GrooveId::STRAIGHT => {
                self.line(Line::Groove(GrooveId::STRAIGHT));
            }
            ProjectControl::GrooveShown => self.groove_shown = None,
            control if control.shapes_groove() => {
                let (_, groove) = self.shown_groove();
                let shaped = match control {
                    ProjectControl::GrooveSteps => groove.with_steps(MAX_GROOVE_STEPS),
                    ProjectControl::Humanise => groove.with_humanise(0),
                    ProjectControl::Push(step) => groove.with_push(usize::from(step), 0),
                    ProjectControl::Velocity(step) => {
                        groove.with_velocity(usize::from(step), FULL_VELOCITY)
                    }
                    _ => groove,
                };
                self.shape_groove(control, shaped);
            }
            ProjectControl::Launch if song.launch != Launch::default() => {
                self.line(Line::Launch(Some(Launch::default())));
            }
            ProjectControl::Reset if self.section_here().is_some_and(|s| s.reset.is_some()) => {
                self.line(Line::Reset(None));
            }
            ProjectControl::Seed if song.seed != 0 => self.line(Line::Seed(0)),
            ProjectControl::Scale if self.scale.on => {
                self.set_scale(Scale {
                    on: false,
                    ..self.scale
                });
            }
            ProjectControl::Root if self.scale.root != 0 => {
                self.set_scale(Scale {
                    root: 0,
                    ..self.scale
                });
            }
            ProjectControl::Grid if song.skip != BEAT_STEPS as u32 => {
                self.line(Line::SetSkip(BEAT_STEPS as u32));
            }
            ProjectControl::AutoJump if self.auto_jump => self.toggle_auto_jump(),
            ProjectControl::Name => {
                self.status = "i renames the project, and C-r there rolls a new name".to_owned();
            }
            _ => {}
        }
    }

    /// `K` and `Enter`: list the choices, switch something on or off, open the audio page, or run
    /// an action.
    fn choose_project_control(&mut self) {
        let control = self.project_control();
        let items: Vec<Item>;
        let (title, current) = match control {
            ProjectControl::Launch => {
                let here = self.song().launch;
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
                let choices = scales(self.scale);
                let here = scale_here(self.scale);
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
                        let scale = Scale { root, ..self.scale };
                        Item::new(scale.root_name(), Pick::ScaleTo(scale))
                    })
                    .collect();
                ("Root", Some(usize::from(self.scale.root)))
            }
            ProjectControl::Grid => {
                let skip = self.song().skip;
                items = GRID_SIZES
                    .iter()
                    .map(|size| Item::new(grid_name(*size), Pick::Line(Line::SetSkip(*size))))
                    .collect();
                let at = GRID_SIZES.iter().position(|size| *size == skip);
                ("Grid. i types any number of steps", at)
            }
            ProjectControl::Groove => return self.open_groove_picker(false),
            ProjectControl::GrooveShown => {
                let shown = self.shown_groove().0;
                items = GrooveId::all()
                    .map(|id| Item::new(id.name(), Pick::ShowGroove(id)).meaning(id.meaning()))
                    .collect();
                ("The groove to draw and change", Some(shown.index()))
            }
            ProjectControl::AutoJump => return self.toggle_auto_jump(),
            ProjectControl::Audio => return self.jump_to_view(View::Audio),
            ProjectControl::Save => return self.line(Line::Write(None)),
            ProjectControl::SaveAs => {
                return self.open_naming_explorer(Purpose::Save, "w", String::new());
            }
            ProjectControl::Render => {
                return self.line(Line::Render {
                    path: None,
                    rows: None,
                    tail: false,
                    stems: false,
                    each_track: false,
                });
            }
            ProjectControl::Open => {
                return self.line(Line::Edit {
                    path: None,
                    force: false,
                });
            }
            ProjectControl::New => {
                return self.line(Line::New {
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
                self.status =
                    "K lists the choices on Groove, Edit, Launch, Scale, Root and Grid; i types the rest"
                        .to_owned();
                return;
            }
        };
        self.show_picker(Picker::new(title, items).opened_on(current));
    }

    /// `i`: type a value, or for what's chosen, switched or run, the same as `K`. A name starts as
    /// it is, to change, and anything else starts empty.
    fn type_project_control(&mut self) {
        let control = self.project_control();
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
                self.status = "Save as puts the project in another folder".to_owned();
                return;
            }
            self.choose_project_control();
            return;
        }
        self.entry = Entry::default();
        if control == ProjectControl::Name {
            self.entry.text = self.project_name();
        }
        self.mode = Mode::Insert;
    }

    /// A key while typing on the project page: `Enter` sets what's typed, `Esc` leaves it, and on
    /// the name `C-r` rolls a new one and `C-u` clears it.
    pub(crate) fn insert_project_page(&mut self, key: Key) {
        let control = self.project_control();
        match key.code {
            KeyCode::Esc => {
                self.entry = Entry::default();
                self.mode = Mode::Normal;
            }
            KeyCode::Char('r') if key.ctrl && control == ProjectControl::Name => {
                let taken = self
                    .folder
                    .as_deref()
                    .and_then(std::path::Path::parent)
                    .map_or_else(std::collections::HashSet::new, crate::exploring::taken_in);
                self.entry.text = self.names.fresh(|name| taken.contains(name));
            }
            KeyCode::Char('u') if key.ctrl => self.entry.text.clear(),
            KeyCode::Backspace => {
                self.entry.text.pop();
            }
            KeyCode::Enter => {
                let text = std::mem::take(&mut self.entry.text);
                self.mode = Mode::Normal;
                self.commit_project_control(control, text.trim());
            }
            KeyCode::Char(c) if !key.ctrl && !key.alt => {
                let room = if control == ProjectControl::Name {
                    MAX_PROJECT_NAME_CHARS
                } else {
                    LONGEST_TYPED
                };
                if self.entry.text.chars().count() < room {
                    self.entry.text.push(c);
                }
            }
            _ => {}
        }
    }

    /// Set what was typed, through the row's command where it has one.
    fn commit_project_control(&mut self, control: ProjectControl, text: &str) {
        if text.is_empty() {
            return;
        }
        match control {
            ProjectControl::Name if text == self.project_name() => {}
            ProjectControl::Name if is_project_name(text) => {
                self.apply(&Action::SetSongName(text.to_owned()));
                self.status = match &self.folder {
                    Some(_) => format!("Named {text}: Save as puts it in a folder of that name"),
                    None => format!("Named {text}"),
                };
            }
            ProjectControl::Name => {
                self.status = format!(
                    "A name is up to {MAX_PROJECT_NAME_CHARS} characters, with no / and not starting with a dot"
                );
            }
            ProjectControl::Bpm if self.project_fine => {
                match with_fraction(self.song().millibpm, text) {
                    Some(millibpm) => self.line(Line::Bpm(millibpm)),
                    None => self.status = "The fine part is the digits after the point".to_owned(),
                }
            }
            ProjectControl::Bpm => self.project_line(&format!("bpm {text}")),
            control if control.shapes_groove() => self.commit_groove(control, text),
            ProjectControl::Launch => self.project_line(&format!("launch {text}")),
            ProjectControl::Reset => self.project_line(&format!("reset {text}")),
            ProjectControl::Seed => self.project_line(&format!("seed {text}")),
            ProjectControl::Grid => self.project_line(&format!("set skip={text}")),
            ProjectControl::Root => match parse_root(text) {
                Some(root) => self.set_scale(Scale { root, ..self.scale }),
                None => self.status = "A root is a note, such as d or f#".to_owned(),
            },
            _ => {}
        }
    }

    /// Set a number typed into the groove drawn: its steps, its humanise in ticks, a push in
    /// ticks either way, or a velocity in percent.
    fn commit_groove(&mut self, control: ProjectControl, text: &str) {
        let (_, groove) = self.shown_groove();
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
            Some(shaped) => self.shape_groove(control, shaped),
            None => {
                self.status = match control {
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
    fn shape_groove(&mut self, control: ProjectControl, shaped: Groove) {
        let (id, groove) = self.shown_groove();
        if control == ProjectControl::Push(0) {
            self.status =
                "The first step stays on the grid: the others are pushed from it".to_owned();
            return;
        }
        if shaped == groove {
            return;
        }
        if let Some(custom) = id.custom_number() {
            self.apply(&Action::SetCustomGroove {
                custom,
                groove: shaped,
            });
            return;
        }
        let custom = self.song().grooves.len();
        let Some(copy) = GrooveId::custom(custom) else {
            self.status = format!(
                "The project's {CUSTOM_GROOVES} grooves are all made: change one of them instead"
            );
            return;
        };
        let depth = self.history.undo_depth();
        self.apply(&Action::SetCustomGroove {
            custom,
            groove: shaped,
        });
        let plays = self.song().groove == id;
        if plays {
            self.apply(&Action::SetGroove(copy));
            self.history.squash_to(depth);
        }
        self.groove_shown = Some(copy);
        self.status = if plays {
            format!("{id} copied to {copy} to change it, which the project now plays")
        } else {
            format!("{id} copied to {copy} to change it")
        };
    }
}
