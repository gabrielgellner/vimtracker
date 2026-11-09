//! The phrase view: one track's steps, the cursor and the playhead.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vt_model::constants::{BEAT_STEPS, FX_SLOTS};
use vt_model::selection::Scope;
use vt_model::song::{Phrase, Track};
use vt_model::step::{FxCommand, Mark, Note, Step};

use crate::app::{App, Field, TrackSpan};
use crate::input::mode::Mode;
use crate::theme;

/// The width of a read-only note column for another track: its playhead marker and a note.
const NEIGHBOUR_WIDTH: usize = 4;
/// Room for the distance from the cursor, the row number, the playhead marker and a gap.
const GUTTER: usize = 9;

/// The rows that aren't steps: the heading, and the box's top and bottom. The box always has its
/// room, so nothing moves when Insert starts.
pub const ROWS_AROUND_STEPS: u16 = 3;

/// Room for a slot's command on screen, a label of up to 5 characters like `2FDBK`, and a gap.
const COMMAND_WIDTH: usize = 6;

/// A slot on screen: its command, coarse and fine value, its mark, and the gap before the next.
const SLOT_WIDTH: usize = COMMAND_WIDTH + 2 + FINE_WIDTH + MARK_WIDTH + 1;

/// The fine part of a value, after its coarse digits.
const FINE_WIDTH: usize = 2;

/// The gap and mark after a slot's value.
const MARK_WIDTH: usize = 2;
/// How an empty gate cell shows, which plays as one pulse of `SGL`.
const GATE_EMPTY: &str = "----";
/// How a gate cell shows with no length of its own, which plays at the track's `Gate`.
const LENGTH_EMPTY: &str = "--";
/// How a row on the grid shows its nudge: as nothing, so only a nudged row's catches the eye.
const NUDGE_EMPTY: &str = "  ";
/// Where the first FX slot starts: after the note, velocity, nudge and gate cell with its length.
const FX_START: usize = 18;

/// A step's text on screen, naming a slot's settings from what's on this track.
fn step_text(step: &Step, track: Option<&Track>) -> String {
    let velocity = step
        .velocity
        .map_or("--".to_owned(), |v| format!("{v:02X}"));
    let gate = step.gate.map_or_else(
        || GATE_EMPTY.to_owned(),
        |gate| format!("{}{}", gate.kind.name(), gate.pulses),
    );
    let length = step
        .length()
        .map_or_else(|| LENGTH_EMPTY.to_owned(), |length| format!("{length:02X}"));
    // A sign and the 24ths in one hex digit, as the most is `B`.
    let nudge = match step.nudge {
        0 => NUDGE_EMPTY.to_owned(),
        late if late > 0 => format!("+{late:X}"),
        early => format!("-{:X}", early.unsigned_abs()),
    };
    let mut text = format!("{} {velocity} {nudge} {gate} {length}", step.note);
    for slot in &step.fx {
        let name = match (slot.command, track) {
            (Some(command), Some(_)) => command.name(),
            (Some(command), None) => command.name().to_owned(),
            (None, _) => "---".to_owned(),
        };
        let name: String = name.chars().take(COMMAND_WIDTH - 1).collect();
        let mark = if slot.is_value() {
            slot.mark.glyph()
        } else {
            ' '
        };
        // Only a continuous setting's value has a fine part. An event's or a list's is whole, and
        // an empty cell has none.
        let fine = if slot.command.is_some_and(FxCommand::has_fine) {
            format!("{:02X}", slot.fine())
        } else {
            "  ".to_owned()
        };
        text.push_str(&format!(
            " {name:<COMMAND_WIDTH$}{:02X}{fine} {mark}",
            slot.coarse()
        ));
    }
    text
}

/// The step as it will be once the value being typed is in: a command chosen in Insert waits for
/// its value before it's written, but shows in its cell meanwhile.
fn pending_step(app: &App, step: &Step, typing: bool) -> Option<Step> {
    let (Field::FxValue(slot), Some(command)) = (app.cursor.field, app.entry.command) else {
        return None;
    };
    if !typing {
        return None;
    }
    let mut step = step.clone();
    step.fx.get_mut(slot)?.command = Some(command);
    Some(step)
}

/// Split text into runs of the same style, given a style for each character.
fn spans_of(text: &str, styles: &[Style]) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut start = 0;
    for end in 1..=text.len() {
        if end == text.len() || styles[end] != styles[start] {
            spans.push(Span::styled(text[start..end].to_owned(), styles[start]));
            start = end;
        }
    }
    spans
}

/// The character range of a field within a step's text.
fn field_range(field: Field) -> (usize, usize) {
    let fx_start = |slot: usize| FX_START + slot * SLOT_WIDTH;
    match field {
        Field::Note => (0, 3),
        Field::Velocity => (4, 6),
        Field::Nudge => (7, 9),
        Field::Gate => (10, 14),
        Field::GateLength => (15, 17),
        Field::FxCommand(slot) => (fx_start(slot), fx_start(slot) + COMMAND_WIDTH),
        Field::FxValue(slot) => (
            fx_start(slot) + COMMAND_WIDTH,
            fx_start(slot) + COMMAND_WIDTH + 2,
        ),
        Field::FxFine(slot) => (
            fx_start(slot) + COMMAND_WIDTH + 2,
            fx_start(slot) + COMMAND_WIDTH + 2 + FINE_WIDTH,
        ),
    }
}

/// The character range a selection covers on one step.
fn selected_columns(step: &Step, scope: Scope, length: usize) -> (usize, usize) {
    if scope.is_whole() {
        return (0, length);
    }
    let slots = step.fx.len();
    let last = scope.last.min(2 + slots);
    let (start, _) = field_range(Field::of_group(scope.first.min(last)));
    let end = match Field::of_group(last) {
        Field::FxCommand(slot) => field_range(Field::FxValue(slot)).1 + FINE_WIDTH + MARK_WIDTH,
        Field::Velocity => field_range(Field::Nudge).1,
        Field::Gate => field_range(Field::GateLength).1,
        field => field_range(field).1,
    };
    (start.min(length), end.min(length))
}

/// Which step of this phrase the playhead is on, if it is on this phrase.
pub(crate) fn playhead_step(app: &App) -> Option<usize> {
    let owner = app.track_here()?;
    let chain = (*app.section_here()?.chains.get(app.cursor.track)?)?;
    let playhead = app.playing_place(app.cursor.track, chain)?;
    let position = owner.chain_position(chain, playhead, app.dealer(app.cursor.track))?;
    let start = owner.phrases.get(&position.phrase)?.start();
    (position.slot == app.cursor.slot && Some(position.phrase) == app.phrase_id())
        .then_some(start + position.step)
}

/// The note the cursor's track's row playing was heard at, before its chain's transpose, where an
/// accumulator moved it.
pub(crate) fn heard_note(app: &App) -> Option<u8> {
    app.places.get(app.cursor.track)?.accumulated
}

/// Where each FX column with a lane of its own is reading, if this phrase is the one playing.
fn lane_playheads(app: &App, phrase: &Phrase) -> [Option<usize>; FX_SLOTS] {
    let mut playheads = [None; FX_SLOTS];
    let Some(jam) = playhead_step(app).and(app.jam.get(app.cursor.track)) else {
        return playheads;
    };
    let count = app.places[app.cursor.track].count - jam.started;
    for (slot, playhead) in playheads.iter_mut().enumerate() {
        *playhead = phrase
            .lane(slot)
            .map(|lane| lane.step_at(count + jam.lane_offsets[slot]));
    }
    playheads
}

/// The characters an FX slot takes in a step's text, with its mark.
fn slot_columns(slot: usize) -> (usize, usize) {
    let (start, _) = field_range(Field::FxCommand(slot));
    (
        start,
        field_range(Field::FxValue(slot)).1 + FINE_WIDTH + MARK_WIDTH,
    )
}

/// Which tracks fit in the overview beside the grid, in order, with the focused one among them and
/// those near it in view. Returns and the master have their notes elsewhere, so they're left out
/// unless one is the track being edited.
fn neighbours(app: &App, width: usize) -> Vec<usize> {
    let tracks: Vec<usize> = app
        .song()
        .tracks
        .iter()
        .enumerate()
        .filter(|(index, track)| track.is_sound() || *index == app.cursor.track)
        .map(|(index, _)| index)
        .collect();
    let shown = (width / NEIGHBOUR_WIDTH).min(tracks.len());
    if shown == 0 {
        return Vec::new();
    }
    let centre = tracks.partition_point(|track| *track < app.cursor.track);
    let start = centre.saturating_sub(shown / 2).min(tracks.len() - shown);
    tracks[start..start + shown].to_vec()
}

/// The headings over the grid's two parts, the note and its FX, at the columns they start at.
fn section_headings(width: usize) -> String {
    let mut text = format!("{:<FX_START$}FX", "Note");
    text.truncate(width);
    text
}

/// What another track's column shows: the phrase it's on and, if it's moving, where in that phrase.
struct Column<'a> {
    steps: &'a [Step],
    playing: Option<usize>,
}

/// The phrase a track is on now, or the one that would play from the cursor while stopped.
fn column(app: &App, track: usize) -> Option<Column<'_>> {
    let owner = app.song().tracks.get(track)?;
    let chain = (*app.section_here()?.chains.get(track)?)?;
    let moving = app.playing_place(track, chain);
    let place = moving.unwrap_or_else(|| app.step_on(track, app.cursor.track, app.section_step()));
    let position = owner.chain_position(chain, place, app.dealer(track))?;
    Some(Column {
        steps: owner.phrases.get(&position.phrase)?.loop_steps(),
        playing: moving.map(|_| position.step),
    })
}

/// The other tracks' columns on one row. Row `00` is each track's own loop start, and past its
/// end the column is blank, so it shows the track's real length.
fn column_spans(
    columns: &[Option<Column>],
    tracks: &[usize],
    row: isize,
    selected: Option<(TrackSpan, isize, isize)>,
) -> Vec<Span<'static>> {
    columns
        .iter()
        .zip(tracks)
        .map(|(column, track)| {
            let in_selection = selected.is_some_and(|(span, first, last)| {
                span.contains(*track) && (first..=last).contains(&row)
            });
            let found = column.as_ref().and_then(|column| {
                let step = column.steps.get(usize::try_from(row).ok()?)?;
                Some((step, column.playing == usize::try_from(row).ok()))
            });
            match found {
                Some((step, true)) => Span::styled(format!(">{}", step.note), theme::active()),
                Some((step, false)) if in_selection => {
                    Span::styled(format!(" {}", step.note), theme::selected())
                }
                Some((step, false)) => Span::styled(format!(" {}", step.note), theme::dim()),
                None => Span::raw(" ".repeat(NEIGHBOUR_WIDTH)),
            }
        })
        .collect()
}

/// How far a row is from the cursor's, as the count to type before `j` or `k`, so it is decimal.
/// The cursor's own row is blank.
fn relative_number(index: usize, cursor: usize) -> String {
    match index.abs_diff(cursor) {
        0 => "   ".to_owned(),
        // Long phrases can reach three digits, which then use up the gap.
        distance => format!("{:<3}", format!("{distance:>2}")),
    }
}

/// A row's number. Row `00` is the loop's first step, and rows above it are negative.
fn row_number(row: isize) -> String {
    let sign = if row < 0 { '-' } else { ' ' };
    format!("{sign}{:02X}", row.unsigned_abs())
}

fn track_label(app: &App, track: usize) -> String {
    let live = app.levels.get(track).copied().unwrap_or_default();
    let flag = if live.mute {
        'm'
    } else if live.solo {
        's'
    } else {
        ' '
    };
    let name = app.song().tracks.get(track).map_or("", |t| t.name.as_str());
    if name.is_empty() {
        format!("{:02}{flag} ", track + 1)
    } else {
        format!("{name}{flag} ")
    }
}

/// Draw the view.
pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let Some(phrase) = app.phrase_here() else {
        frame.render_widget(Paragraph::new("This track has no phrase here."), area);
        return;
    };
    let height = usize::from(area.height.saturating_sub(ROWS_AROUND_STEPS)).max(1);
    let top = app.top;
    let selection = app.mode.is_visual().then(|| app.current_selection());
    let selection = selection
        .or(app.replacing_selection.map(|(selection, _)| selection))
        .or(app.shape.as_ref().map(|panel| panel.selection));
    let across = selection.map(|s| {
        let start = app.extent().start as isize;
        (
            s.tracks,
            s.range.first as isize - start,
            s.range.last as isize - start,
        )
    });
    let playhead = playhead_step(app);
    let lane_playheads = lane_playheads(app, phrase);
    // When each row of the loop starts, in steps, so a beat row is one that starts on a beat even
    // when rows before it last several pulses.
    let starts: Vec<usize> = phrase
        .loop_steps()
        .iter()
        .scan(0, |time, step| {
            let start = *time;
            *time += step.pulses();
            Some(start)
        })
        .collect();
    let body_width = phrase
        .steps()
        .iter()
        .map(|step| step_text(step, app.track_here()).len())
        .max()
        .unwrap_or(0);
    let room = usize::from(area.width).saturating_sub(GUTTER + body_width + 2);
    let others = neighbours(app, room);
    let extent = phrase.extent();
    let columns: Vec<_> = others.iter().map(|track| column(app, *track)).collect();
    let stored = phrase.steps().len();
    // A short phrase still has rows for the longer tracks beside it.
    let rows = columns
        .iter()
        .flatten()
        .map(|column| extent.start + column.steps.len())
        .fold(stored, usize::max);
    let id = app.phrase_id().unwrap_or(0);
    let shared = if app.phrase_uses(id).len() > 1 {
        "*"
    } else {
        ""
    };
    let mut heading = format!(
        "Track {}Phrase {id:02X}{shared} {}/{}",
        track_label(app, app.cursor.track),
        app.cursor.slot + 1,
        app.chain_entries().len(),
    );
    // An order other than forward is named, as a lane's length is.
    if !phrase.order().is_forward() {
        heading.push_str(&format!(" {}", phrase.order()));
    }
    for slot in 0..FX_SLOTS {
        if let Some(lane) = phrase.lane(slot) {
            heading.push_str(&format!(" FX{}:{}", slot + 1, lane.length));
        }
    }
    heading.truncate(GUTTER + body_width);
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let mut title = vec![Span::styled(
        format!("{heading:<width$}  ", width = GUTTER + body_width),
        bold,
    )];
    for track in &others {
        // Over the note, past the column's marker. The track being edited is marked out, so the
        // overview shows every track in its place.
        let label = &track_label(app, *track)[..3];
        if *track == app.cursor.track {
            title.push(Span::raw(" "));
            title.push(Span::styled(
                label.to_owned(),
                bold.fg(theme::BASE).bg(theme::GREEN),
            ));
        } else {
            title.push(Span::styled(format!(" {label}"), bold));
        }
    }
    let cursor_style = theme::cursor(app.mode);
    let boxed = app.mode == Mode::Insert;
    let edge = Style::new().fg(theme::GREEN);
    let rule = |left: &str, right: &str| {
        if boxed {
            let inside = "─".repeat(body_width);
            let indent = " ".repeat(GUTTER - 1);
            Line::styled(format!("{indent}{left}{inside}{right}"), edge)
        } else {
            Line::raw("")
        }
    };
    // The top edge names the grid's two parts, set into the box in Insert.
    let headings = section_headings(body_width);
    let top_rule = if boxed {
        let inside: String = headings
            .chars()
            .chain(std::iter::repeat('─'))
            .take(body_width)
            .map(|c| if c == ' ' { '─' } else { c })
            .collect();
        Line::styled(format!("{}┌{inside}┐", " ".repeat(GUTTER - 1)), edge)
    } else {
        Line::styled(format!("{}{headings}", " ".repeat(GUTTER)), theme::dim())
    };
    let side = if boxed { "│" } else { " " };
    let mut lines = vec![Line::from(title), top_rule];
    for index in (top..rows).take(height) {
        let row = extent.row(index);
        let Some(step) = phrase.steps().get(index) else {
            // The box closes on the first row past the phrase, so rows don't move in Insert.
            let (left, fill, right) = if boxed && index == stored {
                ("└", "─", "┘")
            } else {
                (" ", " ", " ")
            };
            let number = if row.rem_euclid(BEAT_STEPS as isize) == 0 {
                theme::dim()
            } else {
                theme::faint()
            };
            let mut spans = vec![
                Span::raw("   "),
                Span::styled(row_number(row), number),
                Span::raw("  "),
                Span::styled(format!("{left}{}{right}", fill.repeat(body_width)), edge),
                Span::raw(" "),
            ];
            spans.extend(column_spans(&columns, &others, row, across));
            lines.push(Line::from(spans));
            continue;
        };
        let typing = index == app.cursor.step && matches!(app.mode, Mode::Insert | Mode::Replace);
        let pending = pending_step(app, step, typing);
        let step = pending.as_ref().unwrap_or(step);
        let mut text = step_text(step, app.track_here());
        // A label being typed shows in its command's place, and the first digit of a value in its.
        if typing
            && let Field::FxCommand(_) = app.cursor.field
            && !app.entry.text.is_empty()
        {
            let (start, end) = field_range(app.cursor.field);
            if end <= text.len() {
                let typed = format!("{}_", app.entry.text);
                text.replace_range(start..end, &format!("{typed:<width$}", width = end - start));
            }
        }
        // The first digit of a value being typed shows in its place.
        if typing
            && matches!(
                app.cursor.field,
                Field::FxValue(_)
                    | Field::FxFine(_)
                    | Field::Velocity
                    | Field::Nudge
                    | Field::GateLength
            )
            && app.entry.text.len() == 1
        {
            let (start, end) = field_range(app.cursor.field);
            if end <= text.len() {
                text.replace_range(start..end, &format!("{}_", app.entry.text));
            }
        }
        // The row playing shows the note heard where an accumulator moved it, as the note column
        // otherwise shows notes as written.
        if playhead == Some(index)
            && matches!(step.note, Note::On(_))
            && let Some(heard) = heard_note(app)
        {
            let (start, end) = field_range(Field::Note);
            if end <= text.len() {
                text.replace_range(start..end, &Note::On(heard).to_string());
            }
        }
        // A lane's playhead goes in the gap before its column.
        for (slot, at) in lane_playheads.iter().enumerate() {
            let before = slot_columns(slot).0 - 1;
            if *at == Some(index) && text.get(before..=before) == Some(" ") {
                text.replace_range(before..=before, ">");
            }
        }
        let marker = if playhead == Some(index) { ">" } else { " " };
        let plays = extent.plays(index);
        let on_beat = match usize::try_from(row).ok().and_then(|row| starts.get(row)) {
            Some(start) if plays => start.is_multiple_of(BEAT_STEPS),
            _ => row.rem_euclid(BEAT_STEPS as isize) == 0,
        };
        let number = match (plays, on_beat) {
            (true, true) => Style::new().fg(theme::YELLOW),
            (true, false) => Style::new().fg(theme::SUBTEXT1),
            (false, true) => theme::dim(),
            (false, false) => theme::faint(),
        };
        let mut spans = vec![
            Span::styled(relative_number(index, app.cursor.step), theme::faint()),
            Span::styled(row_number(row), number),
            Span::styled(format!(" {marker}"), theme::active()),
            Span::styled(side, edge),
        ];
        // A row its `CHA` or `CND` skipped this time is faint under the playhead, so the logic
        // can be followed.
        let skipped = app
            .places
            .get(app.cursor.track)
            .is_some_and(|place| place.skipped);
        let body = if playhead == Some(index) && skipped {
            theme::faint()
        } else if playhead == Some(index) {
            theme::active()
        } else if plays {
            Style::new()
        } else {
            theme::faint()
        };
        let mut styles = vec![body; text.len()];
        // What's empty is faint, as a code editor dims whitespace, so what's written stands out.
        let mut empty = Vec::new();
        if step.note == Note::Empty {
            empty.push(field_range(Field::Note));
        }
        if step.velocity.is_none() {
            empty.push(field_range(Field::Velocity));
        }
        if step.gate.is_none() {
            empty.push(field_range(Field::Gate));
        }
        if step.length().is_none() {
            empty.push(field_range(Field::GateLength));
        }
        for (slot, fx) in step.fx.iter().enumerate() {
            if fx.command.is_none() {
                empty.push(slot_columns(slot));
            }
        }
        for (start, end) in empty {
            for style in &mut styles[start..end.min(text.len())] {
                if style.fg.is_none() {
                    *style = style.patch(theme::faint());
                }
            }
        }
        // A row's own length and its nudge are dim, so its note, gate and pulses stand out.
        let mut dimmed = Vec::new();
        if step.length().is_some() {
            dimmed.push(field_range(Field::GateLength));
        }
        if step.nudge != 0 {
            dimmed.push(field_range(Field::Nudge));
        }
        for (start, end) in dimmed {
            for style in &mut styles[start..end] {
                if style.fg.is_none() {
                    *style = style.patch(theme::dim());
                }
            }
        }
        if let Note::On(midi) = step.note
            && plays
            && app.scale.on
            && !app.scale.contains(midi)
        {
            let (start, end) = field_range(Field::Note);
            styles[start..end].fill(body.patch(theme::out_of_scale()));
        }
        // A column with a lane plays its own rows: they're lit, and the rest of it is dim.
        for (slot, lane_playhead) in lane_playheads.iter().enumerate() {
            let Some(lane) = phrase.lane(slot) else {
                continue;
            };
            let (start, end) = slot_columns(slot);
            let end = end.min(text.len());
            let style = if *lane_playhead == Some(index) {
                theme::active()
            } else if lane.contains(index) {
                Style::new()
            } else {
                theme::faint()
            };
            if start < end {
                styles[start - 1..end].fill(style);
            }
        }
        // A lock for a voice or an effect this track doesn't have now stays, grey with no colour
        // and brighter than an empty cell, so it isn't typed over. It plays again when its owner is
        // back.
        if let Some(track) = app.track_here() {
            for (slot, fx) in step.fx.iter().enumerate() {
                if fx.command.is_some_and(|command| !command.exists_on(track)) {
                    let (start, end) = slot_columns(slot);
                    let end = end.min(text.len());
                    if start < end {
                        styles[start..end].fill(theme::dim());
                    }
                }
            }
        }
        // A setting's value is in the value colour, its coarse digits bright and bold and its fine
        // ones set back. An event's is in its own colour, as it isn't a setting. Rows already
        // coloured keep theirs.
        for (slot, fx) in step.fx.iter().enumerate() {
            if fx.command.is_none() {
                continue;
            }
            let hue = if fx.is_value() {
                theme::MAUVE
            } else {
                theme::PEACH
            };
            let (start, end) = field_range(Field::FxValue(slot));
            let coarse = Style::new().fg(hue).add_modifier(Modifier::BOLD);
            let fine = Style::new().fg(theme::fine(hue));
            for (range, look) in [(start..end, coarse), (end..end + FINE_WIDTH, fine)] {
                for style in &mut styles[range] {
                    if style.fg.is_none() {
                        *style = style.patch(look);
                    }
                }
            }
        }
        // The FX sit on a band a shade darker than the screen, so the two parts read apart.
        // A selection's or the cursor's colour goes over it.
        for style in styles.iter_mut().skip(FX_START) {
            if style.bg.is_none() {
                *style = style.bg(theme::FX_BAND);
            }
        }
        for (slot, fx) in step.fx.iter().enumerate() {
            if fx.is_value() && fx.mark == Mark::Lock {
                let at = field_range(Field::FxValue(slot)).1 + FINE_WIDTH + MARK_WIDTH - 1;
                if styles[at].fg.is_none() {
                    styles[at] = styles[at].patch(theme::dim());
                }
            }
        }
        let selected = selection.filter(|s| (s.range.first..=s.range.last).contains(&index));
        if let Some(selected) = selected {
            let (start, end) = selected_columns(step, selected.scope, text.len());
            styles[start..end].fill(theme::selected());
        }
        // The cursor stays visible inside a selection, so the moving end can be seen.
        if index == app.cursor.step {
            let (start, end) = field_range(app.cursor.field);
            let end = end.min(text.len());
            let start = start.min(end);
            let (first, last) = selected.map_or((start, end), |selected| {
                let (from, to) = selected_columns(step, selected.scope, text.len());
                (start.clamp(from, to), end.clamp(from, to))
            });
            styles[first..last].fill(cursor_style);
        }
        spans.extend(spans_of(&text, &styles));
        let padding = body_width.saturating_sub(text.len());
        spans.push(Span::raw(" ".repeat(padding)));
        spans.push(Span::styled(side, edge));
        spans.push(Span::raw(" "));
        spans.extend(column_spans(&columns, &others, row, across));
        lines.push(Line::from(spans));
    }
    if !(top..top + height).contains(&stored) || stored >= rows {
        lines.push(rule("└", "┘"));
    }
    frame.render_widget(Paragraph::new(lines), area);
}
