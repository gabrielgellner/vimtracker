//! The status line: mode, transport, tempo and messages.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vt_engine::engine::Transport;
use vt_model::accumulator::{Accumulation, MOST_DEGREES, Moves, amount, amount_text};
use vt_model::arpeggio::{Arpeggio, arp_text, octaves_text, retrig_meaning};
use vt_model::condition::Condition;
use vt_model::constants::TICKS_PER_STEP;
use vt_model::lfo::{Rate, note_rates};

use vt_model::settings::{SettingId, glide_ms};
use vt_model::song::{SynthVoice, Track};
use vt_model::step::{FxCommand, Note};

use crate::app::{App, Field};
use crate::input::keys::Key;
use crate::input::mode::{Mode, View};
use crate::song_view::{COLUMNS_BEFORE_TRACKS, LENGTH_COLUMN, REPEAT_COLUMN, START_COLUMN};
use crate::theme;
use crate::track_page::{AccumulatorField, ArpField, Control};
use crate::tree::ChainColumn;
use crate::views::phrase::{heard_note, playhead_step};

/// Show a tempo without trailing zeros.
pub fn format_bpm(millibpm: u32) -> String {
    let whole = millibpm / 1000;
    let fraction = millibpm % 1000;
    if fraction == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{fraction:03}")
            .trim_end_matches('0')
            .to_owned()
    }
}

/// An FX cell's name: what it locks, by its group and name, and whether it's on this track, or
/// which part of an empty cell the cursor is on.
fn fx_name(app: &App, slot: usize) -> String {
    let command = app.current_slot(slot).and_then(|cell| cell.command);
    match (command, app.track_here()) {
        (Some(FxCommand::Set(id)), Some(track)) if id.exists_on(track) => {
            format!("{}: {}", id.group(), id.name())
        }
        (Some(FxCommand::Set(id)), _) => {
            format!("{}: {}, not on this track", id.group(), id.name())
        }
        (Some(event), _) => event.meaning(),
        (None, _) if matches!(app.cursor.field, Field::FxValue(_)) => {
            format!("FX{} value", slot + 1)
        }
        (None, _) => format!("FX{} command", slot + 1),
    }
}

/// The name of what the cursor is on, for the status line.
pub fn column_name(app: &App) -> String {
    match app.view {
        View::Audio => app.audio_row_here().name().to_owned(),
        View::Project => app.project_control().label().to_owned(),
        View::Phrase => match app.cursor.field {
            Field::Note => "Note".to_owned(),
            Field::Velocity => "Velocity".to_owned(),
            Field::Nudge => "Nudge".to_owned(),
            Field::Gate => "Gate".to_owned(),
            Field::GateLength if app.current_gate().and_then(|gate| gate.length).is_some() => {
                "Gate length".to_owned()
            }
            Field::GateLength => "Gate length, the track's".to_owned(),
            Field::FxCommand(slot) | Field::FxValue(slot) | Field::FxFine(slot) => {
                fx_name(app, slot)
            }
        },
        View::Chain => match app.chain_column {
            ChainColumn::Phrase => "Phrase".to_owned(),
            ChainColumn::Transpose => "Transpose".to_owned(),
        },
        View::Track | View::Mixer => {
            let control = app.track_control();
            app.song().tracks.get(app.cursor.track).map_or_else(
                || control.label().to_owned(),
                |track| control.name_on(track).to_owned(),
            )
        }
        View::Song => match app.song_column {
            LENGTH_COLUMN => "Length".to_owned(),
            REPEAT_COLUMN => "Rpt".to_owned(),
            START_COLUMN => "Start".to_owned(),
            column => {
                let index = column - COLUMNS_BEFORE_TRACKS;
                let name = match app.song().tracks.get(index) {
                    Some(track) if !track.is_sound() => track.kind.title().to_owned(),
                    _ => format!("Track {:02}", index + 1),
                };
                let label = app.song_label();
                if label.is_empty() {
                    format!("{name} chain")
                } else {
                    format!("{name} chain: {label}")
                }
            }
        },
    }
}

/// The value under the cursor in its setting's units, like `1.21 kHz`, where it's a setting the
/// track has: a lock in the grid, or a row of the track page.
pub fn value_shown(app: &App) -> Option<String> {
    let track = app.track_here()?;
    if app.view == View::Phrase && app.cursor.field == Field::Nudge {
        return Some(nudge_shown(app.current_nudge()));
    }
    // While an accumulator moves the row playing, the Note column says what it's heard as, as the
    // grid shows notes as written.
    if app.view == View::Phrase && app.cursor.field == Field::Note {
        playhead_step(app)?;
        let heard = heard_note(app)?;
        return Some(format!("the row playing is heard as {}", Note::On(heard)));
    }
    if app.view == View::Track
        && let Control::Accumulator(field) = app.track_control()
    {
        return Some(accumulator_shown(field, track.accumulation, app.scale.on));
    }
    if app.view == View::Track
        && let Control::Arpeggio(field) = app.track_control()
    {
        return Some(arpeggio_shown(field, track.arpeggio));
    }
    if app.view == View::Track && app.track_control() == Control::Glide {
        return Some(glide_shown(track));
    }
    let (id, value) = match app.view {
        View::Phrase => match app.cursor.field {
            Field::FxCommand(slot) | Field::FxValue(slot) | Field::FxFine(slot) => {
                let cell = app.current_slot(slot)?;
                if cell.command == Some(FxCommand::Cnd) {
                    return Some(condition_shown(cell.coarse()));
                }
                if cell.command == Some(FxCommand::Arp) {
                    return Some(arp_shown(cell.coarse(), track.arpeggio, app.scale.on));
                }
                if cell.command == Some(FxCommand::Acu) {
                    return Some(amount_shown(
                        cell.coarse(),
                        track.accumulation,
                        app.scale.on,
                    ));
                }
                let id = cell.command?.setting()?;
                (id, cell.value)
            }
            // With none of its own, the length is the track's `Gate`.
            Field::GateLength => {
                let length = app.current_gate().and_then(|gate| gate.length);
                let length = length.unwrap_or_else(|| track.gate_length());
                (SettingId::GATE_LENGTH, u16::from(length) << 8)
            }
            _ => return None,
        },
        View::Track | View::Mixer => app.track_control().value_on(track)?,
        _ => return None,
    };
    id.exists_on(track).then(|| shown(id, value))
}

/// A `CND`'s condition, by Elektron's name and when its row plays.
fn condition_shown(value: u8) -> String {
    match Condition::from_value(value) {
        Some(condition) => format!(
            "{}, {}",
            condition.label(),
            condition.meaning().to_lowercase()
        ),
        None => "not a condition, so the row plays".to_owned(),
    }
}

/// How far an `ACU` moves its note, and when, in degrees or semitones as the scale is on or off.
fn amount_shown(value: u8, accumulation: Accumulation, scale_on: bool) -> String {
    match amount(value) {
        Some(0) => "doesn't move the note".to_owned(),
        Some(by) => format!(
            "{} {}, {} {}",
            amount_text(by, scale_on),
            accumulation.adds.each(),
            accumulation.at_limit.name().to_lowercase(),
            match accumulation.moves {
                Moves::Row => "on this row",
                Moves::Track => "with the whole track",
            }
        ),
        None => format!("past {MOST_DEGREES} either way, so it doesn't move the note"),
    }
}

/// What one of the track's accumulator settings does, as it's set.
fn accumulator_shown(
    field: AccumulatorField,
    accumulation: Accumulation,
    scale_on: bool,
) -> String {
    let unit = if scale_on { "degrees" } else { "semitones" };
    match field {
        AccumulatorField::Up => format!("{} {unit} above the note as written", accumulation.up),
        AccumulatorField::Down => {
            format!("{} {unit} below the note as written", accumulation.down)
        }
        AccumulatorField::Moves => lowered(accumulation.moves.meaning()),
        AccumulatorField::AtLimit => lowered(accumulation.at_limit.meaning()),
        AccumulatorField::Adds => lowered(accumulation.adds.meaning()),
        AccumulatorField::Reset => lowered(&accumulation.reset.meaning()),
    }
}

/// What an `ARP` goes round, and how the track plays it.
fn arp_shown(value: u8, arpeggio: Arpeggio, scale_on: bool) -> String {
    format!(
        "{}, {} at {}, {}, {}",
        arp_text(value, scale_on),
        arpeggio.mode.name(),
        arpeggio.rate.name(),
        octaves_text(arpeggio.octaves),
        if arpeggio.retrig { "retrig" } else { "legato" }
    )
}

/// How long a track's ties glide, and what off does on its voice.
fn glide_shown(track: &Track) -> String {
    match glide_ms(f32::from(track.glide)) {
        Some(_) => SettingId::GLIDE
            .entry()
            .kind
            .shown(u16::from(track.glide) << 8),
        None if track.voice() == Some(SynthVoice::Acid) => {
            "Off: a tie slides by Acid's own Slide".to_owned()
        }
        None => "Off: a tie jumps to the next note".to_owned(),
    }
}

/// What one of the track's arpeggio settings does, as it's set.
fn arpeggio_shown(field: ArpField, arpeggio: Arpeggio) -> String {
    match field {
        ArpField::Rate => lowered(arpeggio.rate.meaning()),
        ArpField::Mode => lowered(arpeggio.mode.meaning()),
        ArpField::Octaves if arpeggio.octaves == 1 => {
            "1 octave: the notes alone, not climbing".to_owned()
        }
        ArpField::Octaves => format!(
            "{}: the notes climb that many before it starts again",
            octaves_text(arpeggio.octaves)
        ),
        ArpField::Retrig => lowered(retrig_meaning(arpeggio.retrig)),
    }
}

/// A meaning to follow a name on the status line, its first letter small.
fn lowered(text: &str) -> String {
    let mut letters = text.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(letters).collect()
    })
}

/// How far a nudge moves its row, in 24ths of a step.
fn nudge_shown(nudge: i8) -> String {
    match nudge {
        0 => "on the grid".to_owned(),
        late if late > 0 => format!("{late}/24 of a step late"),
        early => format!("{}/24 of a step early", early.unsigned_abs()),
    }
}

/// A value of a setting in its units.
fn shown(id: SettingId, value: u16) -> String {
    id.entry().kind.shown(value)
}

/// A grid size as a note division where it is one, or otherwise in steps.
pub(crate) fn grid_name(skip: u32) -> String {
    let rate = Rate::Note(skip.saturating_mul(TICKS_PER_STEP as u32));
    if note_rates().contains(&rate) {
        rate.to_string()
    } else {
        Rate::Steps(skip).to_string()
    }
}

/// Draw the status line and the message or command line below it.
pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let transport = match app.transport {
        Transport::Stopped => "stopped",
        Transport::Playing if app.recording() => "playing REC",
        Transport::Playing => "playing",
        Transport::Paused => "paused",
    };
    // Fill changes what the conditions play, so it's shown while it's on, and a realign while
    // it waits for the grid.
    let mut transport = transport.to_owned();
    if app.fill {
        transport.push_str(" FILL");
    }
    if app.realigning {
        transport.push_str(" REALIGN");
    }
    let mode_style = Style::new()
        .fg(theme::BASE)
        .bg(match app.mode {
            Mode::Normal
            | Mode::OperatorPending
            | Mode::Chain
            | Mode::Track
            | Mode::Mixer
            | Mode::Song
            | Mode::Audio
            | Mode::Project => theme::BLUE,
            Mode::VisualField | Mode::VisualStep => theme::MAUVE,
            Mode::Insert => theme::GREEN,
            Mode::Command => theme::PEACH,
            Mode::Help | Mode::Guide | Mode::Picker | Mode::Shape | Mode::Jump => theme::TEAL,
            Mode::Replace | Mode::Restore => theme::RED,
            Mode::Keys => theme::YELLOW,
        })
        .add_modifier(Modifier::BOLD);
    let pending = if app.pending().is_empty() {
        String::new()
    } else {
        let keys: Vec<String> = app.pending().iter().map(Key::to_string).collect();
        format!("  {}", keys.join(" "))
    };
    let dirty = if app.dirty { " *" } else { "" };
    let typed = if app.entry.text.is_empty() {
        String::new()
    } else {
        format!("  typed {}", app.entry.text)
    };
    // The cursor's track shows its own groove, marked so it isn't mistaken for the project's.
    let groove = match app
        .song()
        .tracks
        .get(app.cursor.track)
        .and_then(|t| t.groove)
    {
        Some(own) => format!("{own} (track)"),
        None => app.song().groove.to_string(),
    };
    let level = app
        .song()
        .tracks
        .get(app.cursor.track)
        .map_or(0, |t| t.level);
    // Only shown when on, since it changes what you hear and tracks otherwise run freely.
    let reset = app
        .section_here()
        .and_then(|s| s.reset)
        .map_or_else(String::new, |steps| format!("Reset {steps}  "));
    let sections = app.song().sections.len();
    let section = if sections > 1 {
        let looping = if app.looping.is_some() { " loop" } else { "" };
        format!("Sec {}/{sections}{looping}  ", app.cursor.section + 1)
    } else {
        String::new()
    };
    let reset = format!("{section}{reset}");
    // The grid only means something where notes are entered.
    let grid = if app.view == View::Phrase {
        let jump = if app.auto_jump { " Jump" } else { "" };
        format!("  Grid {}{jump}", grid_name(app.song().skip))
    } else {
        String::new()
    };
    let scale = if app.view == View::Phrase && app.scale.on {
        format!(
            "  Scale {} {}",
            app.scale.root_name(),
            app.scale.kind.short_name()
        )
    } else {
        String::new()
    };
    let status = Line::from(vec![
        // The keyboard takes the page's keys, but is its own mode.
        Span::styled(
            format!(
                " {} ",
                if app.mode == Mode::Keys {
                    Mode::Keys.label()
                } else {
                    app.key_scope().label()
                }
            ),
            mode_style,
        ),
        Span::styled(
            match value_shown(app) {
                Some(value) => format!(" {}, {value} ", column_name(app)),
                None => format!(" {} ", column_name(app)),
            },
            theme::selected(),
        ),
        Span::raw(format!(
            " {transport}  {} BPM  Groove {groove}  {reset}Oct {} Vel {:02X}{grid}{scale}  TVol {level:02X}{dirty}{pending}{typed}",
            format_bpm(app.song().millibpm),
            app.octave,
            app.note_velocity,
        )),
    ]);
    let message = if app.mode == Mode::Command {
        format!(":{}", app.command_line)
    } else if let Some(jump) = &app.field_jump {
        format!("/{}", jump.query)
    } else {
        app.status.clone()
    };
    frame.render_widget(Paragraph::new(vec![status, Line::raw(message)]), area);
}

#[cfg(test)]
mod tests {
    use super::{format_bpm, grid_name};

    #[test]
    fn a_grid_reads_as_a_note_division_where_it_is_one() {
        for (skip, name) in [
            (1, "1/16"),
            (2, "1/8"),
            (3, "1/8d"),
            (4, "1/4"),
            (6, "1/4d"),
            (8, "1/2"),
            (12, "1/2d"),
            (16, "1 bar"),
            (32, "2 bars"),
            (7, "7st"),
            (0, "0st"),
        ] {
            assert_eq!(grid_name(skip), name);
        }
    }

    #[test]
    fn tempos_drop_trailing_zeros() {
        assert_eq!(format_bpm(120_000), "120");
        assert_eq!(format_bpm(137_630), "137.63");
        assert_eq!(format_bpm(99_001), "99.001");
    }
}
