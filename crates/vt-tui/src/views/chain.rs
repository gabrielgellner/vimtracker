//! The chain view: one track's phrases in order. The row past the end adds a phrase.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::glyphs;
use crate::input::mode::Mode;
use crate::theme;
use crate::tree::ChainColumn;
use crate::views::phrase::track_name;
use crate::views::widgets::group;

/// Draw the view.
pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let Some(chain) = app.chain_id() else {
        frame.render_widget(Paragraph::new("This track has no chain."), area);
        return;
    };
    let song = app.song();
    let bold = Style::new().add_modifier(Modifier::BOLD);
    // The track as the phrase view names it, with its machine or kind as a glyph before it, and a
    // track in a group carries its mark, in the group's colour, as on the mixer.
    let mut title = vec![];
    if let Some(track) = app.track_here() {
        title.push(Span::styled(
            format!("{} ", glyphs::track(track)),
            theme::dim(),
        ));
    }
    title.push(Span::styled(
        format!("Track {}", track_name(app, app.cursor.track)),
        bold,
    ));
    if let Some(mark) = group::mark(song, app.cursor.track) {
        title.extend([Span::raw(" "), mark.patch_style(bold)]);
    }
    title.push(Span::styled(format!("  Chain {chain:02X}"), bold));
    // A chain that plays in more than one place is linked, as a phrase is.
    if app.chain_uses() > 1 {
        title.push(Span::styled(format!(" {}", glyphs::LINK), theme::dim()));
    }
    let mut lines = vec![
        Line::from(title),
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
        // A phrase that plays in more than one place is linked, and the space after the glyph is
        // one it isn't drawn over.
        let linked = entry.is_some_and(|entry| app.phrase_uses(entry.phrase).len() > 1);
        let phrase = entry.map_or_else(
            || "--".to_owned(),
            |entry| {
                let mark = if linked {
                    glyphs::LINK.to_string()
                } else {
                    String::new()
                };
                format!("{:02X}{mark}", entry.phrase)
            },
        );
        let after = if linked { glyphs::ICON_SPACE } else { ' ' };
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
            Span::raw(format!("{after}    ")),
            Span::styled(
                format!("{:<3}", shown(ChainColumn::Transpose, transpose)),
                style(ChainColumn::Transpose),
            ),
            Span::raw(format!("    {steps}")),
        ]));
    }
    frame.render_widget(Paragraph::new(lines), area);
}
