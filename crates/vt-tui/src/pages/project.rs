//! What keys do on the project page, `Space 9`: on its Song sub-page, the song's own settings,
//! its grooves, a row for the Audio sub-page, and the actions that save, render and open
//! projects, and `H` and `L` between its sub-pages. Each row changes its value as its
//! `:` command does, through the same parser, so the two can't disagree.
//!
//! `ProjectPage` is the Song sub-page as a component (#320): its cursor and the groove it shows,
//! its moves, its edits, in `editing`, and its drawing, in `draw`, given a [`Ctx`]. It's a
//! [`Page`], with its rows of the binding table, [`Page::KEYS`], here beside it. An edit that runs
//! its row's command asks for it in its [`Outcome`], as one that opens a picker, the explorer or
//! the Audio sub-page does (#784).

pub mod draw;
pub(crate) mod editing;

use ratatui::Frame;
use ratatui::layout::Rect;

use vt_model::groove::{Groove, GrooveId, MAX_GROOVE_STEPS};
use vt_model::scale::{Scale, ScaleKind};
use vt_model::selection::Motion;
use vt_model::song::{LAUNCH_WORDS, Launch, Song};

use crate::app::App;
use crate::input::bindings::Cmd;
use crate::input::mode::View;
use crate::pages::project::editing::page_beside;
use crate::pages::{Ctx, CtxMut, Outcome, Page};

/// How far a small and a large step move the tempo, in thousandths of a BPM, and its fine part.
pub(crate) const BPM_NUDGE: i64 = 1000;
const BPM_JUMP: i64 = 10_000;
pub(crate) const FINE_NUDGE: i64 = 10;
const FINE_JUMP: i64 = 100;
/// How far a large step moves the seed.
const SEED_JUMP: u64 = 10;
/// How far a large step moves a push, in ticks, a humanise, in ticks, and a velocity, in percent.
const PUSH_JUMP: i64 = 10;
const HUMANISE_JUMP: i64 = 4;
const VELOCITY_JUMP: i64 = 10;
/// The longest value typed on the page other than a name.
const LONGEST_TYPED: usize = 12;

/// The project page's sub-pages, in order, moved between with `H` and `L`: the song's own
/// settings, then the audio of the machine it plays on, which is kept in the config folder
/// rather than the song (decision 297). Each is a view of its own, so each keeps its cursor.
pub(crate) const PROJECT_PAGES: [View; 2] = [View::Project, View::Audio];

/// One row's setting, or what it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectControl {
    Name,
    /// Where it's saved, and whether there are changes since. It only shows.
    Folder,
    /// Whether `:w` refuses to write over it, as `:set readonly` sets (decision 313).
    ReadOnly,
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
    /// The output playing, which opens the Audio sub-page.
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
pub const PROJECT_ROWS: [&[ProjectControl]; 18] = [
    &[ProjectControl::Name],
    &[ProjectControl::Folder],
    &[ProjectControl::ReadOnly],
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
            Self::ReadOnly => "Read-only",
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
    pub(crate) fn is_action(self) -> bool {
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

/// The project page's Song sub-page as a component: where its cursor is, and the groove it draws
/// and changes, kept while other views are shown. How long a groove's rows are depends on the
/// groove it shows, so each move is given the song, and the page holds none of its own. Its edits
/// are `App`'s, as each runs its row's `:` command. It's drawn with [`Page::draw`], given the rest
/// of the program as a [`Ctx`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectPage {
    /// The row and setting the cursor is on, and whether it's on the tempo's fine part.
    pub row: usize,
    pub field: usize,
    pub fine: bool,
    /// Where `j` and `k` last left the cursor, as its row and field, and the field it was on
    /// before a shorter row moved it in, as on the track page.
    pub(crate) wanted: Option<(usize, usize, usize)>,
    /// The groove the page draws and changes, or the project's if none was chosen.
    pub groove_shown: Option<GrooveId>,
}

impl ProjectPage {
    /// What the cursor is on.
    pub fn control(&self, song: &Song) -> ProjectControl {
        let row = self.row.min(PROJECT_ROWS.len() - 1);
        PROJECT_ROWS[row][self.field.min(self.row_length(row, song) - 1)]
    }

    /// How many fields a row has: a groove's rows have one for each of its steps.
    fn row_length(&self, row: usize, song: &Song) -> usize {
        let fields = PROJECT_ROWS[row];
        match fields.first() {
            Some(ProjectControl::Push(_) | ProjectControl::Velocity(_)) => {
                self.shown_groove(song).1.steps().min(fields.len())
            }
            _ => fields.len(),
        }
    }

    /// The groove the page draws, and its pushes.
    pub fn shown_groove(&self, song: &Song) -> (GrooveId, Groove) {
        let id = self.groove_shown.unwrap_or(song.groove);
        (id, song.groove(id))
    }

    /// Put the cursor on a row and a field of it, as near as there is. The field `j` and `k` aim
    /// for is this one from now on, as Vim's `0` and `l` set the column `j` keeps.
    fn move_to(&mut self, row: usize, field: usize, song: &Song) {
        self.wanted = None;
        self.row = row.min(PROJECT_ROWS.len() - 1);
        self.field = field.min(self.row_length(self.row, song) - 1);
        self.fine = false;
    }

    /// The field `j` and `k` aim for: the one before a shorter row moved the cursor in, if it hasn't
    /// moved since, as on the track page.
    fn wanted_field(&self) -> usize {
        match self.wanted {
            Some((row, field, wanted)) if (row, field) == (self.row, self.field) => wanted,
            _ => self.field,
        }
    }

    /// Up or down to a row, aiming for the field `j` and `k` aim for.
    fn move_row(&mut self, row: usize, song: &Song) {
        let wanted = self.wanted_field();
        self.move_to(row, wanted, song);
        self.wanted = Some((self.row, self.field, wanted));
    }

    /// `l`: onto the tempo's fine part, and then on to the next.
    fn right(&mut self, song: &Song) {
        if !self.fine && self.control(song) == ProjectControl::Bpm {
            self.fine = true;
        } else {
            self.move_to(self.row, self.field.saturating_add(1), song);
        }
    }

    /// `h`: back off the fine part, and onto the tempo's from the setting beside it.
    fn left(&mut self, song: &Song) {
        if self.fine {
            self.fine = false;
            return;
        }
        let before = self.field;
        self.move_to(self.row, before.saturating_sub(1), song);
        self.fine = self.field != before && self.control(song) == ProjectControl::Bpm;
    }

    /// Run one of the page's own keys, its moves, on a song. `times` is its count, at least 1,
    /// and `count` the count as typed, as `gg` and `G` go to the row it names.
    pub fn key_on(&mut self, cmd: Cmd, times: usize, count: Option<usize>, song: &Song) -> Outcome {
        let last = PROJECT_ROWS.len() - 1;
        match cmd {
            Cmd::Move(Motion::Down) => {
                self.move_row(self.row.saturating_add(times).min(last), song);
            }
            Cmd::Move(Motion::Up) => self.move_row(self.row.saturating_sub(times), song),
            Cmd::Move(Motion::First) => {
                self.move_row(count.map_or(0, |row| row.saturating_sub(1)), song);
            }
            Cmd::Move(Motion::Last) => {
                let row = count.map_or(last, |row| row.saturating_sub(1)).min(last);
                self.move_row(row, song);
            }
            Cmd::Move(Motion::Right) => (0..times).for_each(|_| self.right(song)),
            Cmd::Move(Motion::Left) => (0..times).for_each(|_| self.left(song)),
            Cmd::Move(Motion::RowStart) => self.move_to(self.row, 0, song),
            Cmd::Move(Motion::RowEnd) => self.move_to(self.row, usize::MAX, song),
            _ => return Outcome::Passed,
        }
        Outcome::Done
    }
}

impl Page for ProjectPage {
    const KEYS: &'static [(&'static str, Cmd, &'static str)] = &[
        ("j", Cmd::Move(Motion::Down), "Next row"),
        ("k", Cmd::Move(Motion::Up), "Previous row"),
        ("g g", Cmd::Move(Motion::First), "First row, or row n"),
        ("G", Cmd::Move(Motion::Last), "Last row, or row n"),
        ("h", Cmd::Move(Motion::Left), "Previous setting"),
        (
            "l",
            Cmd::Move(Motion::Right),
            "Next setting, or the tempo's fine part",
        ),
        ("0", Cmd::Move(Motion::RowStart), "First setting"),
        ("$", Cmd::Move(Motion::RowEnd), "Last setting"),
        ("=", Cmd::Increase, "Increase the value, or switch on"),
        ("-", Cmd::Decrease, "Decrease the value, or switch off"),
        ("+", Cmd::IncreaseLarge, "Increase the value, a large step"),
        ("_", Cmd::DecreaseLarge, "Decrease the value, a large step"),
        ("C-a", Cmd::Increase, "Increase the value"),
        ("C-x", Cmd::Decrease, "Decrease the value"),
        ("x", Cmd::ClearField, "Back to a new project's"),
        ("K", Cmd::PickValue, "Choose from a list, switch, or do it"),
        (
            "Enter",
            Cmd::PickValue,
            "Do this action, or open the Audio page",
        ),
        ("H", Cmd::PreviousPage, "Previous page: Song, Audio"),
        ("L", Cmd::NextPage, "Next page: Song, Audio"),
    ];

    type Drawn = ();

    /// Run a key as the program gives it to the page: one of its moves, [`ProjectPage::key_on`],
    /// an edit, which mostly asks for its row's command to run, or `H` and `L`, which show the
    /// sub-page beside it, or pass it back.
    fn key(&mut self, cmd: Cmd, times: usize, ctx: &mut CtxMut) -> Outcome {
        let count = ctx.modes.count;
        match self.key_on(cmd, times, count, ctx.song()) {
            Outcome::Passed => self.edit(cmd, times, ctx.modes.view, ctx.services),
            outcome => outcome,
        }
    }

    /// Draw the Song sub-page.
    fn draw(&self, frame: &mut Frame, area: Rect, ctx: &Ctx) {
        crate::pages::project::draw::draw(frame, area, self, ctx);
    }

    fn column(&self, ctx: &Ctx) -> String {
        self.control(ctx.song()).label().to_owned()
    }
}

impl App {
    /// `Space 9`: the project page, on whichever of its sub-pages showed last, as a jump.
    pub(crate) fn show_project(&mut self) {
        let last = self
            .recent_views
            .iter()
            .find(|view| PROJECT_PAGES.contains(view))
            .copied()
            .unwrap_or(View::Project);
        self.jump_to_view(last);
    }

    /// `H` and `L` on the Audio sub-page: the sub-page beside this one, or this one at either end.
    pub(crate) fn show_project_page_beside(&mut self, next: bool) {
        self.show_view(page_beside(self.modes.view, next));
    }
}
