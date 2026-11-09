//! The chain view: one track's phrases in order. The row past the end adds a phrase.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::input::mode::Mode;
use crate::theme;
use crate::tree::ChainColumn;

/// Draw the view.
pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let Some(chain) = app.chain_id() else {
        frame.render_widget(Paragraph::new("This track has no chain."), area);
        return;
    };
    let song = app.song();
    let shared = if app.chain_uses() > 1 { "*" } else { "" };
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let mut lines = vec![
        Line::styled(
            format!(
                "Track {:02}  Chain {chain:02X}{shared}",
                app.cursor.track + 1
            ),
            bold,
        ),
        Line::styled("Slot    Phrase  Trans  Steps", bold),
    ];
    let playing = app
        .playing_place(app.cursor.track, chain)
        .and_then(|playhead| {
            song.tracks[app.cursor.track].chain_position(
                chain,
                playhead,
                app.dealer(app.cursor.track),
            )
        })
        .map(|position| position.slot);
    let entries = app.chain_entries();
    let room = usize::from(area.height.saturating_sub(2)).max(1);
    let first = (app.cursor.slot + 1).saturating_sub(room);
    let typing = app.mode == Mode::Insert;
    let cursor = theme::cursor(app.mode);
    for slot in (first..=entries.len()).take(room) {
        let entry = entries.get(slot);
        let marker = if playing == Some(slot) { ">" } else { " " };
        let phrase = entry.map_or_else(
            || "--".to_owned(),
            |entry| {
                let mark = if app.phrase_uses(entry.phrase).len() > 1 {
                    "*"
                } else {
                    ""
                };
                format!("{:02X}{mark}", entry.phrase)
            },
        );
        let transpose = entry.map_or_else(String::new, |entry| format!("{:+03}", entry.transpose));
        let here = slot == app.cursor.slot;
        let shown = |column: ChainColumn, text: String| {
            if here && typing && app.chain_column == column {
                format!("{}_", app.entry.text)
            } else {
                text
            }
        };
        let style = |column: ChainColumn| {
            if here && app.chain_column == column {
                cursor
            } else {
                Style::new()
            }
        };
        let steps = entry
            .and_then(|entry| song.tracks[app.cursor.track].phrases.get(&entry.phrase))
            .map_or_else(String::new, |phrase| phrase.len().to_string());
        lines.push(Line::from(vec![
            Span::styled(format!("{slot:02X}"), theme::dim()),
            Span::styled(format!(" {marker}    "), theme::active()),
            Span::styled(
                format!("{:<3}", shown(ChainColumn::Phrase, phrase)),
                style(ChainColumn::Phrase),
            ),
            Span::raw("     "),
            Span::styled(
                format!("{:<3}", shown(ChainColumn::Transpose, transpose)),
                style(ChainColumn::Transpose),
            ),
            Span::raw(format!("    {steps}")),
        ]));
    }
    frame.render_widget(Paragraph::new(lines), area);
}
