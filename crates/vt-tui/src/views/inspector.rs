//! The row inspector: the cursor's row spelled out in words, in the room right of the phrase
//! view's overview, as the Sound page spells out a voice and the M8's and Elektron's parameter
//! pages name what a value means. `Space i` shows or hides it (decision 255).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use vt_model::step::{FxCommand, FxSlot, GateKind, Mark, Note, Step};

use crate::app::{App, Field};
use crate::theme;
use crate::views::status::phrase_value;
use crate::views::widgets::{self, bar::ValueBar};

/// The narrowest it's drawn: a label, a value and room for what it means.
pub const NARROWEST: u16 = 39;
/// The widest it grows to where there's room.
pub const WIDEST: u16 = 60;
/// Its tallest: a line for each of the row's parts, a blank line above and below, and the frame.
pub const HEIGHT: u16 = 13;

/// The width of a part's label, and of its value after it.
const LABEL: usize = 9;
const VALUE: usize = 9;
/// The cells of the velocity's bar, between its ends.
const BAR_CELLS: usize = 9;

/// What a gate does over its pulses, in words.
fn gate_meaning(kind: GateKind, pulses: u8) -> String {
    let what = match kind {
        GateKind::Single => "one note",
        GateKind::Multi => "a note each",
        GateKind::Hold => "tied on",
        GateKind::Rest => "silent",
    };
    let pulses = if pulses == 1 {
        "1 pulse".to_owned()
    } else {
        format!("{pulses} pulses")
    };
    format!("{what}, {pulses}")
}

/// A mark's word, for a value that has one other than the lock everything has.
fn mark_word(slot: FxSlot) -> Option<&'static str> {
    match slot.mark {
        _ if !slot.is_value() => None,
        Mark::Lock => None,
        Mark::Hold => Some("hold"),
        Mark::Glide => Some("glide"),
    }
}

/// One of the row's parts: its label dim, its value bright, and what it means dim after it.
fn part(label: &str, value: Vec<Span<'static>>, meaning: String) -> Line<'static> {
    let shown: usize = value.iter().map(|span| span.content.chars().count()).sum();
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(format!("{label:<LABEL$}"), theme::dim()),
    ];
    spans.extend(value);
    spans.push(Span::raw(" ".repeat(VALUE.saturating_sub(shown).max(1))));
    spans.push(Span::styled(meaning, theme::dim()));
    Line::from(spans)
}

/// A value as plain text.
fn text(value: String) -> Vec<Span<'static>> {
    vec![Span::raw(value)]
}

/// The lines spelling out a row.
fn lines(app: &App, step: &Step) -> Vec<Line<'static>> {
    let mut lines = vec![Line::raw("")];
    let note = match step.note {
        Note::On(midi) => phrase_value(app, Field::Note).unwrap_or(format!("MIDI {midi}")),
        Note::Off => "ends the note sounding".to_owned(),
        Note::Empty => "nothing plays".to_owned(),
    };
    lines.push(part("Note", text(step.note.to_string()), note));
    lines.push(match step.velocity {
        Some(velocity) => {
            let set = u8::try_from(u16::from(velocity.min(0x7F)) * 255 / 127).unwrap_or(u8::MAX);
            let bar = Line::from(ValueBar::new(set, BAR_CELLS)).spans;
            let mut line = part("Velocity", text(format!("{velocity:02X}")), String::new());
            line.spans.pop();
            line.spans.extend(bar);
            line
        }
        None => part("Velocity", text("··".to_owned()), "full".to_owned()),
    });
    if step.nudge != 0 {
        let nudge = phrase_value(app, Field::Nudge).unwrap_or_default();
        lines.push(part("Nudge", text(format!("{:+X}", step.nudge)), nudge));
    }
    lines.push(match step.gate {
        Some(gate) => part(
            "Gate",
            text(format!("{}{}", gate.kind.name(), gate.pulses)),
            gate_meaning(gate.kind, gate.pulses),
        ),
        None => part(
            "Gate",
            text("····".to_owned()),
            gate_meaning(GateKind::Single, 1),
        ),
    });
    let length = phrase_value(app, Field::GateLength).unwrap_or_default();
    lines.push(match step.length() {
        Some(own) => part("Length", text(format!("{own:02X}")), length),
        None => part(
            "Length",
            text("··".to_owned()),
            format!("{length}, the track's"),
        ),
    });
    for (at, slot) in step.fx.iter().enumerate() {
        let label = format!("FX {}", at + 1);
        let Some(command) = slot.command else {
            lines.push(part(
                &label,
                text("\u{b7}\u{b7}\u{b7}".to_owned()),
                "empty".to_owned(),
            ));
            continue;
        };
        // A setting by its name, with its value in its units and its mark; an event by its
        // label and value, with what it does.
        let line = match command {
            FxCommand::Set(id) => {
                let on_track = app.track_here().is_some_and(|track| id.exists_on(track));
                let mut meaning: Vec<String> =
                    phrase_value(app, Field::FxValue(at)).into_iter().collect();
                if !on_track {
                    meaning.push("not on this track".to_owned());
                }
                meaning.extend(mark_word(*slot).map(str::to_owned));
                let name: String = id.name().chars().take(VALUE - 1).collect();
                part(
                    &label,
                    vec![Span::styled(name, Style::new().fg(theme::MAUVE))],
                    meaning.join("  "),
                )
            }
            event => part(
                &label,
                vec![Span::styled(
                    format!("{} {:02X}", event.name(), slot.coarse()),
                    Style::new().fg(theme::PEACH),
                )],
                phrase_value(app, Field::FxValue(at)).unwrap_or_else(|| event.meaning()),
            ),
        };
        lines.push(line);
    }
    lines
}

/// Draw it in an area, over the row under the cursor.
pub fn draw(frame: &mut Frame, area: Rect, app: &App, row: &str) {
    let Some(step) = app
        .phrase_here()
        .and_then(|phrase| phrase.steps().get(app.cursor.step))
    else {
        return;
    };
    let block = widgets::window_border()
        .border_style(Style::new().fg(theme::SURFACE1))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(format!("Row {row}"), theme::dim()),
            Span::raw(" "),
        ]));
    let mut lines = lines(app, step);
    lines.push(Line::raw(""));
    // As tall as the row needs, with a nudge or without.
    let area = Rect {
        height: area.height.min(lines.len() as u16 + 2),
        ..area
    };
    let inner = block.inner(area);
    block.render(area, frame.buffer_mut());
    frame.render_widget(Paragraph::new(lines), inner);
}
