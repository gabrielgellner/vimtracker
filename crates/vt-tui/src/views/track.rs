//! The track page: what a track holds apart from its steps, grouped by theme.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vt_model::action::TrackControl;
use vt_model::constants::TRACK_ENVELOPES;
use vt_model::lfo::{Destination, depth_text};
use vt_model::song::Track;

use crate::app::App;
use crate::input::mode::Mode;
use crate::theme;
use crate::track_page::{Control, ENVELOPES, EnvelopeField, LFOS, LfoField, Page};
use crate::views::braille::envelope_lines;
use crate::views::status::format_swing;

/// The size of an envelope's diagram, in characters.
const DIAGRAM_WIDTH: usize = 28;
const DIAGRAM_HEIGHT: usize = 4;

/// Room for a section's name.
const NAME_WIDTH: usize = 10;
/// Cells in a value's bar, each split into eighths. An odd number, so a centre has a cell.
const BAR_CELLS: usize = 9;
/// The lines at each end of a bar's range, touching the cells between them.
const BAR_START: &str = "▕";
const BAR_END: &str = "▏";
/// The Sound page's columns: a label, then a value and its bar.
const SOUND_LABEL: usize = 10;
const SOUND_COLUMN: usize = 36;
/// The longest file name shown beside the output's column.
const FILE_WIDTH: usize = 24;
/// The characters for a cell partly full, from one eighth to seven.
const EIGHTHS: [char; 7] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉'];

/// A bar for a value, full at `FF`, filling all of its cells. Only `00` leaves them empty.
pub fn level_bar(value: u8) -> String {
    let eighths = (usize::from(value) * BAR_CELLS * 8 / 255).max(usize::from(value > 0));
    let mut bar = "█".repeat(eighths / 8);
    if eighths % 8 > 0 {
        bar.push(EIGHTHS[eighths % 8 - 1]);
    }
    format!("{bar:<BAR_CELLS$}")
}

/// A mark for a value either side of a centre, like pan, filling all of the bar's cells.
pub fn centre_bar(value: u8) -> String {
    let cell = usize::from(value) * BAR_CELLS / 256;
    format!("{:<BAR_CELLS$}", format!("{}│", " ".repeat(cell)))
}

/// The value a control shows a bar for, and how. True is a mark either side of a centre.
fn bar_value(control: Control, track: &Track) -> Option<(u8, bool)> {
    let level = |value| Some((value, false));
    match control {
        Control::Level => level(track.level),
        Control::Base(TrackControl::Pan) => Some((track.pan, true)),
        // Those with a middle that's a setting of its own show where they are around it.
        Control::Base(TrackControl::Harmonics) => {
            Some((track.harmonics, track.synth_voice.centred()[0]))
        }
        Control::Base(TrackControl::Timbre) => level(track.timbre),
        Control::Base(TrackControl::Morph) => Some((track.morph, track.synth_voice.centred()[2])),
        Control::Base(TrackControl::Cutoff) => level(track.cutoff),
        Control::Base(TrackControl::Resonance) => level(track.resonance),
        Control::Base(TrackControl::Delay) => level(track.delay),
        Control::Base(TrackControl::Reverb) => level(track.reverb),
        Control::Base(TrackControl::Drive) => level(track.drive),
        Control::Base(TrackControl::LowCut) => level(track.low_cut),
        Control::Base(TrackControl::HighCut) => level(track.high_cut),
        _ => None,
    }
}

/// A row of the Sound page: a control on the left, and perhaps one on the right.
fn sound_row(app: &App, track: &Track, controls: &[Control]) -> Line<'static> {
    let current = app.track_control();
    let mut spans = Vec::new();
    let mut used = 0;
    for (column, control) in controls.iter().enumerate() {
        if column > 0 {
            spans.push(Span::raw(" ".repeat(SOUND_COLUMN.saturating_sub(used))));
        }
        let label = format!("{:<SOUND_LABEL$}", control.label_on(track.synth_voice));
        used = SOUND_LABEL;
        spans.push(Span::styled(label, theme::dim()));
        let typing = *control == current && app.mode == Mode::Insert;
        let mut text = if typing {
            format!("{}_", app.entry.text)
        } else {
            value(*control, track, app.song().swing)
        };
        if *control == Control::Sample {
            text = text.chars().take(FILE_WIDTH).collect();
        }
        let style = if *control == current {
            theme::cursor(app.mode)
        } else if *control == Control::Swing && track.swing.is_none() {
            theme::dim()
        } else {
            Style::new()
        };
        used += text.chars().count();
        spans.push(Span::styled(text, style));
        if let Some((value, centred)) = bar_value(*control, track) {
            let bar = if centred {
                centre_bar(value)
            } else {
                level_bar(value)
            };
            used += 1 + BAR_START.chars().count() + BAR_CELLS + BAR_END.chars().count();
            spans.push(Span::raw(" "));
            spans.push(Span::styled(BAR_START, theme::faint()));
            spans.push(Span::styled(bar, Style::new().fg(theme::MAUVE)));
            spans.push(Span::styled(BAR_END, theme::faint()));
        }
    }
    Line::from(spans)
}

/// A setting's value as text.
pub(crate) fn value(control: Control, track: &Track, project_swing: u16) -> String {
    let hex = |value: u8| format!("{value:02X}");
    match control {
        Control::Machine => track.machine.name().to_owned(),
        Control::SynthVoice => track.synth_voice.name().to_owned(),
        Control::Base(TrackControl::Harmonics) => hex(track.harmonics),
        Control::Base(TrackControl::Timbre) => hex(track.timbre),
        Control::Base(TrackControl::Morph) => hex(track.morph),
        Control::Base(TrackControl::Cutoff) => hex(track.cutoff),
        Control::Base(TrackControl::Resonance) => hex(track.resonance),
        Control::Base(TrackControl::Pan) => hex(track.pan),
        Control::Base(TrackControl::Delay) => hex(track.delay),
        Control::Base(TrackControl::Reverb) => hex(track.reverb),
        Control::Base(TrackControl::Drive) => hex(track.drive),
        Control::Base(TrackControl::LowCut) => hex(track.low_cut),
        Control::Base(TrackControl::HighCut) => hex(track.high_cut),
        Control::Sample => track.sample.as_deref().map_or_else(
            || "---".to_owned(),
            |sample| sample.rsplit('/').next().unwrap_or(sample).to_owned(),
        ),
        Control::Mute | Control::Solo => String::new(),
        Control::FilterType => track.filter.name().to_owned(),
        Control::Level => hex(track.level),
        Control::Swing => format_swing(track.swing.unwrap_or(project_swing)),
        Control::Envelope(slot, field) => {
            let envelope = &track.envelopes[slot];
            let ms = |value: u16| format!("{value} ms");
            match field {
                EnvelopeField::Destination => envelope
                    .destination
                    .map_or_else(|| "---".to_owned(), |d| d.name().to_owned()),
                EnvelopeField::Attack => ms(envelope.attack_ms),
                EnvelopeField::Decay => ms(envelope.decay_ms),
                EnvelopeField::Sustain => format!("{}%", envelope.sustain),
                EnvelopeField::Release => ms(envelope.release_ms),
                // The first envelope on `Vol` shapes each note, rather than moving a value.
                EnvelopeField::Depth
                    if slot == 0 && envelope.destination == Some(Destination::Vol) =>
                {
                    "--".to_owned()
                }
                EnvelopeField::Depth => depth_text(envelope.depth),
                EnvelopeField::Reset => if envelope.reset { "on" } else { "off" }.to_owned(),
            }
        }
        Control::Lfo(slot, field) => {
            let lfo = &track.lfos[slot];
            match field {
                LfoField::Destination => lfo
                    .destination
                    .map_or_else(|| "---".to_owned(), |d| d.name().to_owned()),
                LfoField::Shape => lfo.shape.name().to_owned(),
                LfoField::Rate => lfo.rate.to_string(),
                LfoField::Depth => depth_text(lfo.depth),
                LfoField::Phase => format!("{}°", lfo.phase),
                LfoField::Retrig => if lfo.retrig { "on" } else { "off" }.to_owned(),
            }
        }
    }
}

/// How wide a setting's column is, so columns line up and don't move as values change.
fn width(control: Control) -> usize {
    match control {
        Control::Lfo(_, LfoField::Rate) => 9,
        Control::Lfo(_, LfoField::Shape) => 8,
        Control::Base(TrackControl::Reverb) | Control::Lfo(_, LfoField::Retrig) => 8,
        Control::Base(TrackControl::LowCut | TrackControl::HighCut) => 9,
        Control::Envelope(
            _,
            EnvelopeField::Attack
            | EnvelopeField::Decay
            | EnvelopeField::Sustain
            | EnvelopeField::Release,
        ) => 9,
        _ => 7,
    }
}

/// The column names over a row.
fn heading(controls: &[Control]) -> Line<'static> {
    let mut text = " ".repeat(NAME_WIDTH);
    for control in controls {
        text.push_str(&format!(
            "{:<width$}",
            control.label(),
            width = width(*control)
        ));
    }
    Line::styled(text, theme::dim())
}

/// A row of settings, with the cursor on one of them.
fn row(app: &App, track: &Track, name: &str, controls: &[Control]) -> Line<'static> {
    let mut spans = vec![Span::raw(format!("{name:<NAME_WIDTH$}"))];
    let current = app.track_control();
    for control in controls {
        let typing = *control == current && app.mode == Mode::Insert;
        let text = if typing {
            format!("{}_", app.entry.text)
        } else {
            value(*control, track, app.song().swing)
        };
        // A track following the project's swing shows what it plays, dimmed as it isn't its own.
        let style = if *control == current {
            theme::cursor(app.mode)
        } else if *control == Control::Swing && track.swing.is_none() {
            theme::dim()
        } else {
            Style::new()
        };
        let room = width(*control);
        let gap = room.saturating_sub(text.chars().count());
        spans.push(Span::styled(text, style));
        spans.push(Span::raw(" ".repeat(gap)));
    }
    Line::from(spans)
}

/// Each envelope's shape, side by side, with the one being edited picked out.
fn diagrams(app: &App, track: &Track) -> Vec<Line<'static>> {
    let current = match app.track_control() {
        Control::Envelope(slot, _) => Some(slot),
        _ => None,
    };
    let shapes = track
        .envelopes
        .map(|envelope| envelope_lines(&envelope, DIAGRAM_WIDTH, DIAGRAM_HEIGHT));
    let style = |slot: usize| {
        if current == Some(slot) {
            theme::active()
        } else {
            theme::dim()
        }
    };
    let mut lines = vec![Line::from(
        (0..TRACK_ENVELOPES)
            .map(|slot| {
                let name = track.envelopes[slot]
                    .destination
                    .map_or("---", |destination| destination.name());
                let label = format!("Env {}  {name}", slot + 1);
                Span::styled(
                    format!("{label:<width$}", width = DIAGRAM_WIDTH + 2),
                    style(slot),
                )
            })
            .collect::<Vec<_>>(),
    )];
    lines.extend((0..DIAGRAM_HEIGHT).map(|row| {
        Line::from(
            (0..TRACK_ENVELOPES)
                .map(|slot| Span::styled(format!("{}  ", shapes[slot][row]), style(slot)))
                .collect::<Vec<_>>(),
        )
    }));
    lines
}

/// Draw the view.
pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let Some(track) = app.song().tracks.get(app.cursor.track) else {
        frame.render_widget(Paragraph::new("This track doesn't exist."), area);
        return;
    };
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let tabs = [Page::Sound, Page::Mod].map(|page| {
        let text = format!(" {} ", page.name());
        if page == app.track_page {
            Span::styled(text, bold)
        } else {
            Span::styled(text, theme::dim())
        }
    });
    let mut title = vec![Span::styled(
        format!("Track {:02} {} ", app.cursor.track + 1, track.name),
        bold,
    )];
    title.extend(tabs);
    let mut lines = vec![Line::from(title), Line::raw("")];
    match app.track_page {
        Page::Sound => {
            for controls in app.page_rows() {
                lines.push(sound_row(app, track, controls));
            }
        }
        Page::Mod => {
            lines.push(heading(&ENVELOPES[0]));
            for (slot, controls) in ENVELOPES.iter().enumerate() {
                lines.push(row(app, track, &format!("Env {}", slot + 1), controls));
            }
            lines.push(Line::raw(""));
            lines.push(heading(&LFOS[0]));
            for (slot, controls) in LFOS.iter().enumerate() {
                lines.push(row(app, track, &format!("LFO {}", slot + 1), controls));
            }
            lines.push(Line::raw(""));
            lines.extend(diagrams(app, track));
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}
