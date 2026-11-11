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
use vt_model::constants::{BAR_STEPS, TICKS_PER_STEP};
use vt_model::lfo::{Rate, note_rates};

use vt_model::settings::{SettingId, glide_ms};
use vt_model::song::{SynthVoice, Track, trim_text};
use vt_model::step::{FxCommand, Note};

use crate::app::{App, Field};
use crate::banking::sample_shown;
use crate::glyphs;
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
        View::Sample => match app.sample_page.setting {
            Some(setting) => format!("Sample: {}", setting.name()),
            None => "Sample".to_owned(),
        },
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
    if app.view == View::Track && app.track_control() == Control::Sample {
        return Some(sample_shown(app.song(), track.sample()));
    }
    if app.view == View::Track && app.track_control() == Control::Reset {
        return Some(reset_shown(track.reset));
    }
    if matches!(app.view, View::Track | View::Mixer) && app.track_control() == Control::Trim {
        return Some(trim_text(track.trim));
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
                if cell.command == Some(FxCommand::Set(SettingId::SAMPLE)) && track.is_sound() {
                    return Some(sample_shown(app.song(), cell.coarse()));
                }
                if cell.command == Some(FxCommand::Set(SettingId::SLICE)) && track.is_sound() {
                    return Some(app.slice_shown(cell));
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
/// What a track's own `Reset` does, as it's set.
fn reset_shown(reset: Option<u16>) -> String {
    let Some(steps) = reset else {
        return "Off: its chain plays on".to_owned();
    };
    let bars = usize::from(steps) / BAR_STEPS;
    let length = match bars {
        _ if !usize::from(steps).is_multiple_of(BAR_STEPS) => format!("{steps} steps"),
        1 => "bar".to_owned(),
        _ => format!("{bars} bars"),
    };
    format!("its chain starts again every {length} of the song")
}

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

/// The transport as glyphs in place of words: play in the active colour, pause and stop dim, and a
/// red record beside play while recording. Fill changes what the conditions play, so it's named
/// while it's on, and a realign and a cued launch are named, with the glyph for waiting, until the
/// launch grid comes round.
pub fn transport_spans(app: &App) -> Vec<Span<'static>> {
    let (glyph, style) = match app.transport {
        Transport::Playing => (glyphs::PLAY, theme::active()),
        Transport::Paused => (glyphs::PAUSE, theme::dim()),
        Transport::Stopped => (glyphs::STOP, theme::dim()),
    };
    let mut spans = vec![Span::raw(" "), Span::styled(glyph.to_string(), style)];
    if app.transport == Transport::Playing && app.recording() {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            glyphs::RECORD.to_string(),
            Style::new().fg(theme::RED),
        ));
    }
    if app.fill {
        spans.push(Span::raw(" FILL"));
    }
    let realigning = app.realigning || app.realigning_tracks.contains(&true);
    let cued = app.jam.iter().any(|jam| jam.cued.is_some());
    for (waiting, name) in [(realigning, "REALIGN"), (cued, "LAUNCH")] {
        if waiting {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(glyphs::WAITING.to_string(), theme::dim()));
            spans.push(Span::raw(format!(" {name}")));
        }
    }
    spans
}

/// What goes at the right of the status line, only while there's any: a spinner while work goes on
/// in the background, and a warning glyph in the warning colour, with a count, once audio has
/// dropped out.
pub fn right_spans(app: &App) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    if let Some((work, turn)) = app.background_work() {
        spans.push(Span::styled(format!("{turn} {work}  "), theme::dim()));
    }
    let dropouts = app.audio.late + app.audio.xruns;
    if dropouts > 0 {
        spans.push(Span::styled(
            format!("{} {dropouts}  ", glyphs::WARNING),
            theme::warning(),
        ));
    }
    spans
}

/// Draw the status line and the message or command line below it.
pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    // The sample page's modes in their own colours: the keys mode's, Visual's, and with the focus
    // on the settings rows, the cursor's, as the setting it's on is drawn.
    let badge = match app.sample_mode() {
        Some("SETTINGS") => Some(theme::ROSEWATER),
        Some(mode) if mode == Mode::VisualField.label() => Some(theme::MAUVE),
        _ => None,
    };
    let mode_style = Style::new()
        .fg(theme::BASE)
        .bg(badge.unwrap_or(match app.mode {
            Mode::Normal
            | Mode::OperatorPending
            | Mode::Chain
            | Mode::Track
            | Mode::Mixer
            | Mode::Song
            | Mode::Audio
            | Mode::Project
            | Mode::Sample => theme::BLUE,
            Mode::VisualField | Mode::VisualStep => theme::MAUVE,
            Mode::Insert => theme::GREEN,
            Mode::Command => theme::PEACH,
            Mode::Help | Mode::Guide | Mode::Picker | Mode::Shape | Mode::Jump => theme::TEAL,
            Mode::Replace | Mode::Restore => theme::RED,
            Mode::Keys => theme::YELLOW,
        }))
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
    let mut status = vec![
        // The keyboard takes the page's keys, but is its own mode.
        Span::styled(
            format!(
                " {} ",
                // The sample page's modes keep its keys, but each is named, so `h` is never a
                // pad or a setting's move without the badge saying so.
                if let Some(mode) = app.sample_mode() {
                    mode
                } else if app.mode == Mode::Keys {
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
    ];
    status.extend(transport_spans(app));
    status.push(Span::raw(format!(
        "  {} BPM  Groove {groove}  {reset}Oct {} Vel {:02X}{grid}{scale}  TVol {level:02X}{dirty}{pending}{typed}",
        format_bpm(app.song().millibpm),
        app.octave,
        app.note_velocity,
    )));
    let message = if app.mode == Mode::Command {
        format!(":{}", app.command_line)
    } else if let Some(jump) = &app.field_jump {
        format!("/{}", jump.query)
    } else if app.status.is_empty()
        && let Some(hint) = app.sample_mode_hint().or_else(|| app.audio_page_hint())
    {
        hint
    } else if app.status.is_empty()
        && let Some(total) = app.adjusted_total()
    {
        // What the changes kept on the selection add up to stays in sight as it moves.
        total
    } else {
        app.status.clone()
    };
    frame.render_widget(
        Paragraph::new(vec![Line::from(status), Line::raw(message)]),
        area,
    );
    // The right end of the status line, over whatever reaches it.
    let right = Line::from(right_spans(app));
    let width = (right.width() as u16).min(area.width);
    if width > 0 {
        let at = Rect::new(area.right() - width, area.y, width, 1);
        frame.render_widget(Paragraph::new(right), at);
    }
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
