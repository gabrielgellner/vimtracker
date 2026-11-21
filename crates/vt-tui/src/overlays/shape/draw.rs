//! The shape panel, over the right of the screen so the column it fills stays in view, and the bars
//! of that column's values beside it.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vt_model::lfo::Shape as Wave;
use vt_model::notation::Field;
use vt_model::scale::Scale;
use vt_model::shape::{Context, FillKind, GenParam, ShapeColumn, Where, World, written_from};
use vt_model::speed::Speed;
use vt_model::step::Note;

use crate::input::mode::Mode;
use crate::input::typed::TypedValue;
use crate::overlays::shape::ShapePanel;
use crate::pages::Ctx;
use crate::widgets::popup;
use crate::widgets::theme;
use crate::widgets::tokens::TokenCells;
use crate::widgets::value_bars::{Beside, Side};

/// Room for eight notes in a `List`'s values; a longer list shows as many as fit and how many
/// more.
const WIDTH: u16 = 42;
/// The border, the heading and the keys, beyond the rows.
const AROUND_ROWS: u16 = 4;
/// Clear of the CPU figure at the top of the screen.
const FROM_TOP: u16 = 2;
/// The room a row's label takes unless one is longer.
const LABEL: usize = 8;

/// A row's value in spans, `room` characters at most for a `List`'s values, with a note outside
/// the scale coloured as the grid colours it.
fn spans(panel: &ShapePanel, context: &Context, row: GenParam, room: usize) -> Vec<Span<'static>> {
    let value = || vec![Span::raw(row.shown(&panel.shape, context))];
    let notes: Vec<u8> = match row {
        GenParam::Values => return list_spans(panel, room, None),
        _ if !matches!(panel.column, ShapeColumn::Note { .. }) => return value(),
        GenParam::From => vec![panel.shape.from],
        GenParam::To => vec![panel.shape.to],
        _ => return value(),
    };
    notes
        .into_iter()
        .map(|note| Span::styled(panel.column.show(note), note_style(panel, note)))
        .collect()
}

/// How a value of the column is drawn: a note outside the scale as the grid colours it.
fn note_style(panel: &ShapePanel, value: u8) -> Style {
    match panel.column {
        ShapeColumn::Note { scale, .. } if scale.on && !scale.contains(value) => {
            theme::out_of_scale()
        }
        _ => Style::new(),
    }
}

/// A `List`'s values in `room` characters: as many as fit from the first, then how many more
/// there are, as `+248`, so a list as long as a phrase shows its start and its length. On the
/// cursor's row, `lit`, they're lit together, as a value on its row is.
fn list_spans(panel: &ShapePanel, room: usize, lit: Option<Style>) -> Vec<Span<'static>> {
    let list = &panel.list;
    let mut spans = Vec::new();
    let mut used = 0;
    for (index, value) in list.iter().enumerate() {
        let shown = panel.column.show(*value);
        let gap = usize::from(index > 0);
        let left = list.len() - index;
        // Room for this one, and for what says how many are left after it, unless it's the last.
        let after = if left > 1 {
            format!(" +{}", left - 1).len()
        } else {
            0
        };
        if used + gap + shown.len() + after > room {
            let more = format!("{}+{left}", if index > 0 { " " } else { "" });
            spans.push(Span::styled(more, lit.unwrap_or_else(theme::dim)));
            break;
        }
        if gap > 0 {
            spans.push(Span::styled(" ", lit.unwrap_or_default()));
        }
        used += gap + shown.len();
        spans.push(Span::styled(
            shown,
            lit.unwrap_or_else(|| note_style(panel, *value)),
        ));
    }
    spans
}

/// What's being typed, in `room` characters: its end, where the next character goes, kept in
/// sight as a long list is typed, with `…` before it where the start is cut off.
fn typed_text(typing: &TypedValue, room: usize) -> String {
    let shown = typing.shown();
    let count = shown.chars().count();
    if count <= room || room < 2 {
        return shown;
    }
    let tail: String = shown.chars().skip(count - (room - 1)).collect();
    format!("…{tail}")
}

/// The room each of `Weights`' digits takes, so the note under it fits, `C#` and a space.
const WEIGHT_CELL: usize = 3;

/// `Weights` in two lines, a field of tokens, each digit over the note it's for: `F  0  8` over
/// `C  D  E`. The cursor in them is on its digit, and its note is in full; on the row but not in
/// them, the digits are lit together; and while they're typed, the text so far and a `_` after it.
fn weight_lines(panel: &ShapePanel, current: bool, label: usize) -> [Line<'static>; 2] {
    let notes = panel.weight_notes();
    let typed = current.then_some(panel.typing.as_ref()).flatten();
    let tokens = panel.tokens.as_ref().filter(|_| current);
    let made;
    let field = match (tokens, panel.field(GenParam::Weights)) {
        (Some(tokens), _) => &tokens.field,
        (None, Some(field)) => {
            made = match typed {
                Some(typed) => Field::new(field.grammar(), typed, panel.column),
                None => field,
            };
            &made
        }
        (None, None) => return [Line::default(), Line::default()],
    };
    let typing_at = match typed {
        Some(typed) => Some(typed.len()),
        None => field.inserting().map(|insert| insert.to),
    };
    let [top, under] = TokenCells {
        field,
        cursor: panel.weight.filter(|_| current),
        typing_at,
        current,
        cell: WEIGHT_CELL,
        under: &notes,
    }
    .lines();
    let mut line = vec![Span::raw(format!("{:<label$}", GenParam::Weights.name()))];
    line.extend(top);
    let mut below = vec![Span::raw(" ".repeat(label))];
    below.extend(under);
    [Line::from(line), Line::from(below)]
}

/// How wide the panel is: room for eight notes in a `List`'s values, or for each of `Weights`'
/// digits over its note.
fn width(panel: &ShapePanel, rows: &[GenParam], label: usize) -> u16 {
    if !rows.contains(&GenParam::Weights) {
        return WIDTH;
    }
    // The last note needs only its own room, and the border is either side.
    let weights = (panel.weight_notes().len() * WEIGHT_CELL).saturating_sub(1);
    WIDTH.max((label + weights + 2) as u16)
}

/// A `Tonnetz`' first chords as the panel names them, `C Cm G# Fm`, and across tracks that each
/// plays a voice of them.
fn progression(panel: &ShapePanel) -> String {
    let scale = match panel.column {
        ShapeColumn::Note { scale, .. } => scale,
        _ => Scale::default(),
    };
    let shape = panel.shape_of(0);
    let across = panel.parts.len() > 1;
    let count = if across { 5 } else { 8 };
    let names: Vec<String> = shape
        .tonnetz
        .chords(shape.from, shape.seed, scale, count)
        .into_iter()
        .map(|chord| chord.name())
        .collect();
    let names = names.join(" ");
    if across {
        format!("A voice a track: {names}")
    } else {
        names
    }
}

/// Where the panel goes in an area: its top right corner, clear of the CPU figure, or its top
/// left, sized for its rows.
fn window(area: Rect, panel: &ShapePanel, side: Side) -> Rect {
    let rows = panel.rows();
    let below = Rect {
        y: area.y + FROM_TOP.min(area.height),
        height: area.height.saturating_sub(FROM_TOP),
        ..area
    };
    // `Weights` takes a second line, for its notes.
    let lines_more = u16::from(rows.contains(&GenParam::Weights));
    let width = width(panel, &rows, labels(&rows));
    let height = rows.len() as u16 + lines_more + AROUND_ROWS;
    match side {
        Side::Right => popup::top_right(below, width, height),
        Side::Left => popup::top_left(below, width, height),
    }
}

/// Room for the longest label, as `Transpose` is longer than the rest.
fn labels(rows: &[GenParam]) -> usize {
    rows.iter()
        .map(|row| row.name().len() + 1)
        .max()
        .unwrap_or(0)
        .max(LABEL)
}

/// Draw the panel in its usual place, the area's top right.
pub fn draw(frame: &mut Frame, area: Rect, ctx: &Ctx, panel: &ShapePanel) {
    draw_in(frame, window(area, panel, Side::Right), ctx, panel);
}

/// Draw the panel with the bars of the column it shapes beside the column, when the phrase grid is
/// what's under it and the column's values are amounts. The bars go right of the column and the
/// panel top right, or where they'd meet, the bars left of it and the panel top left, so neither
/// covers the other or the column. Where no way round fits, the bars are drawn over the panel.
pub(crate) fn draw_with_bars(
    frame: &mut Frame,
    area: Rect,
    ctx: &Ctx,
    panel: &ShapePanel,
    beside: Option<&Beside>,
) {
    let bars = beside.and_then(|beside| Some((beside, panel.value_bars(ctx)?)));
    let Some((beside, bars)) = bars else {
        draw(frame, area, ctx, panel);
        return;
    };
    let clear = |a: Rect, b: Rect| a.intersection(b).is_empty();
    let column = Rect::new(
        beside.column.0,
        area.y,
        beside.column.1 - beside.column.0,
        area.height,
    );
    let ways = [
        (Side::Right, Side::Right),
        (Side::Left, Side::Left),
        (Side::Left, Side::Right),
        (Side::Right, Side::Left),
    ];
    let found = ways.into_iter().find_map(|(bars_side, panel_side)| {
        let at = beside.place(&bars, bars_side)?;
        let window = window(area, panel, panel_side);
        (clear(at, window) && clear(column, window)).then_some((at, window))
    });
    match found {
        Some((at, window)) => {
            beside.draw(frame.buffer_mut(), &bars, at);
            draw_in(frame, window, ctx, panel);
        }
        None => {
            draw(frame, area, ctx, panel);
            let at = beside
                .place(&bars, Side::Right)
                .or_else(|| beside.place(&bars, Side::Left));
            if let Some(at) = at {
                beside.draw(frame.buffer_mut(), &bars, at);
            }
        }
    }
}

/// Draw the panel in its window.
fn draw_in(frame: &mut Frame, window: Rect, ctx: &Ctx, panel: &ShapePanel) {
    let rows = panel.rows();
    let context = panel.context();
    let labels = labels(&rows);
    // What a row's value has beside its label, inside the border.
    let room = usize::from(window.width).saturating_sub(labels + 2);
    let name = match panel.column {
        ShapeColumn::Note { .. } => "Note".to_owned(),
        ShapeColumn::Velocity => "Velocity".to_owned(),
        ShapeColumn::Nudge { .. } => "Nudge".to_owned(),
        ShapeColumn::Length => "Length".to_owned(),
        ShapeColumn::Fx { command, .. } => ctx
            .song()
            .tracks
            .get(panel.parts[0].track)
            .map_or_else(|| command.name(), |track| command.name_on(track)),
    };
    let (mut filled, mut rows_shaped) = (0, 0);
    let source = panel.source_steps();
    for (nth, part) in panel.parts.iter().enumerate() {
        let steps = panel.base_steps(part);
        let shape = panel.shape_of(nth);
        let written = written_from(steps, part.range, part.column, &shape, &panel.list, source);
        // A `Follow`, `Grids`, `Divisions`, `Ring` and a `Variation` write every row, rests and
        // all, so their count is of the notes, and `Multiply`'s of the rhythm it leaves. A
        // `Tonnetz` writes the rows its `Where` chooses, as other fills do.
        filled += if matches!(
            shape.kind,
            FillKind::Follow
                | FillKind::Grids
                | FillKind::Divisions
                | FillKind::Ring
                | FillKind::Variation
        ) {
            written
                .iter()
                .filter(|(_, step)| matches!(step.note, Note::On(_)))
                .count()
        } else if shape.kind == FillKind::Multiply {
            written
                .iter()
                .filter(|(_, step)| match part.column {
                    ShapeColumn::Fx { slot, command } => step
                        .fx
                        .get(slot)
                        .is_some_and(|fx| fx.command == Some(command)),
                    _ => matches!(step.note, Note::On(_)),
                })
                .count()
        } else {
            written.len()
        };
        rows_shaped += part.range.len();
    }
    let tracks = match panel.parts.len() {
        1 => String::new(),
        count => format!(" on {count} tracks"),
    };
    let live = if panel.live.is_some() { ", live" } else { "" };
    let mut lines = vec![Line::styled(
        format!("{name}{tracks}{live}: {filled} of {rows_shaped} rows"),
        theme::dim(),
    )];
    for row in rows {
        let current = row == panel.row;
        if row == GenParam::Weights {
            lines.extend(weight_lines(panel, current, labels));
            continue;
        }
        let typing = current.then_some(panel.typing.as_ref()).flatten();
        let mut line = vec![Span::raw(format!("{:<labels$}", row.name()))];
        if current {
            let style = theme::cursor(if panel.typing.is_some() {
                Mode::Insert
            } else {
                Mode::Normal
            });
            match typing {
                Some(typing) => line.push(Span::styled(typed_text(typing, room), style)),
                None if row == GenParam::Values => {
                    line.extend(list_spans(panel, room, Some(style)));
                }
                None => line.push(Span::styled(row.shown(&panel.shape, &context), style)),
            }
        } else {
            line.extend(spans(panel, &context, row, room));
        }
        lines.push(Line::from(line));
    }
    let too_few = panel.too_few_notes();
    let pairs = panel.parts.len() > 1 && matches!(panel.shape.place, Where::Complementary { .. });
    let rhythm = match panel.shape.place {
        Where::Rhythm { rhythm, .. } if panel.shape.kind.has_where() => Some(rhythm),
        _ => None,
    };
    let speed = panel.speed();
    let cells = panel.shape.evolves();
    let chords = if panel.shape.kind == FillKind::Tonnetz {
        progression(panel)
    } else {
        String::new()
    };
    let hint = match panel.shape.kind {
        _ if panel.weight.is_some() => "h l move  = - change  0-F types  Esc out",
        // Each pair of tracks shares one coin.
        _ if pairs && panel.shape.kind.has_where() => "Each row goes to one track or the other",
        // A named rhythm's parts go to the tracks in turn, and a 12/8 one's pulses are a bar's
        // triplet eighths.
        _ if rhythm.is_some() && panel.parts.len() > 1 => "Each track plays the next part",
        _ if rhythm.is_some_and(World::ternary) && speed != Some(Speed::EighthTriplet) => {
            "Its 12 rows make a bar on a 1/8T track"
        }
        _ if cells && panel.parts.len() > 1 => "Each track a cell, a row a generation",
        _ if cells => "Each row a cell, stepped on by Vary",
        FillKind::Random | FillKind::Walk | FillKind::Pink if ctx.mode() == Mode::Shape => {
            "~ swap  r reroll"
        }
        FillKind::Wave if panel.shape.wave == Wave::Smooth && ctx.mode() == Mode::Shape => {
            "~ swap  r reroll"
        }
        FillKind::Shuffle if ctx.mode() == Mode::Shape => "r deals again",
        FillKind::Acid if panel.shape.acid.keep => "r rolls the accents, slides and octaves",
        FillKind::Acid => "r rolls a new riff  ~ swap the ends",
        FillKind::Follow if panel.shape.follow.mutates() && ctx.mode() == Mode::Shape => {
            "r other changes  L keeps it in step"
        }
        FillKind::Follow => "L keeps it in step with its source",
        FillKind::List => "~ reverse the list",
        FillKind::Snap => match panel.column {
            ShapeColumn::Note { scale, .. } if scale.on => "To the nearest note in the scale",
            _ => "The scale is off, so nothing moves",
        },
        FillKind::Invert | FillKind::Shuffle | FillKind::Reverse => "From the values there",
        FillKind::Multiply => "16 rows a number, row 1 its lowest bit",
        FillKind::Grids if panel.shape.grids.part.is_none() => "Each track plays its own part",
        FillKind::Grids if panel.shape.grids.chaos > 0 => "r rolls the chaos",
        FillKind::Grids => "A density adds the strongest hits first",
        FillKind::Divisions if panel.shape.divisions.channel.is_none() => {
            "Each track its own channel: A, B, A+B"
        }
        FillKind::Divisions if panel.shape.divisions.crowded() => {
            "16ths and triplets need 32nds to fit"
        }
        FillKind::Divisions if ctx.mode() == Mode::Shape => "r rolls the dice  a chance only adds",
        FillKind::Divisions => "A chance only adds strikes",
        FillKind::Ring if panel.shape.ring.kept.is_some() => "Its loop kept  r starts a new one",
        FillKind::Ring if ctx.mode() == Mode::Shape => "r a new ring  L runs it on each pass",
        FillKind::Ring => "Bit 0 plays, the byte is the pitch",
        FillKind::Tonnetz => chords.as_str(),
        FillKind::Variation => match &too_few {
            Some(why) => why.as_str(),
            None if ctx.mode() == Mode::Shape => "r another  Order 3 copies closely",
            None => "Learned from the notes there",
        },
        FillKind::InvertBits | FillKind::MaskBits | FillKind::RotateBits => match panel.column {
            ShapeColumn::Note { scale, .. } if scale.on => "Bits of each degree, kept in the scale",
            _ => "The values there, as bits",
        },
        _ => "~ swap the ends",
    };
    lines.push(Line::styled(hint, theme::dim()));
    let border = popup::border()
        // What it does: generate on an empty column, or reshape what's there.
        .title(format!(" {} ", panel.verb()))
        .title_bottom(if panel.live.is_some() {
            " Enter apply  L keep live  Esc cancel "
        } else {
            " Enter keep  L keep live  Esc cancel "
        })
        .title_style(Style::new().add_modifier(Modifier::BOLD));
    let inside = popup::draw(frame, window, border);
    frame.render_widget(Paragraph::new(lines), inside);
    token_menu(frame, inside, panel, labels);
}

/// The list `K` or `Tab` opened in a field of tokens, under the field, its names under the token
/// the cursor is on (decision 458).
fn token_menu(frame: &mut Frame, inside: Rect, panel: &ShapePanel, labels: usize) {
    let (Some(tokens), Some(at)) = (&panel.tokens, panel.weight) else {
        return;
    };
    let Some(menu) = &tokens.menu else {
        return;
    };
    let Some(place) = panel.rows().iter().position(|row| *row == panel.row) else {
        return;
    };
    // Under the line the field's tokens are over, after the panel's heading.
    let row = inside.y + place as u16 + 2;
    let column = inside.x + (labels + at * WEIGHT_CELL) as u16;
    // The panel keeps the list and lends it to be drawn, so it scrolls from the top each time.
    let mut picker = menu.picker.clone();
    crate::overlays::picker::draw::draw_menu_under(frame, row, column, &mut picker);
}

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;

    use super::*;
    use crate::widgets::pin::{app_on, assert_drawn, cut, keys, screen};

    /// A row of the screen as text.
    fn row(buffer: &Buffer, y: u16) -> String {
        (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect()
    }

    /// Where the panel's top left corner is: the `╭` before its title.
    fn panel_at(buffer: &Buffer, title: &str) -> (u16, u16) {
        for y in 0..buffer.area.height {
            let text = row(buffer, y);
            if let Some(at) = text.find(title) {
                let left = text[..at].chars().count() - 1;
                return (left as u16, y);
            }
        }
        panic!("no panel titled {title}");
    }

    #[test]
    fn shaping_velocities_shows_each_rows_bar_now_over_the_one_before() {
        let mut app = app_on(&["C-3 40 --- --- ---"; 16]);
        keys(&mut app, "lvjjjjjSKramp<Enter>");
        let screen = screen(&mut app, 163, 42);
        // Right of the velocity, over the rest of the row, the panel staying top right.
        assert_drawn(
            &cut(&screen, Rect::new(15, 0, 24, 9)),
            &[
                " VEL 00 ─ 7F            ",
                "abbbccccccccaaaaaaaaaaaa",
                " 00─7F avg 40 6 changed ",
                "acccccccccccccccccccccca",
                " ███████████▏           ",
                "addddddddddddaaaaaaaaaaa",
                " ████▍██████▏           ",
                "aeeeefdddddddaaaaaaaaaaa",
                " ████████▉██▏           ",
                "aeeeeeeeefdddaaaaaaaaaaa",
                " █████████████▏         ",
                "aeeeeeeeeeeeeeeaaaaaaaaa",
                " █████████████████▋     ",
                "aeeeeeeeeeeeeeeeeeeaaaaa",
                " ██████████████████████ ",
                "aeeeeeeeeeeeeeeeeeeeeeea",
                "     ···· ·· ···   ··   ",
                "ggggghhhhghhgiiiiiiiiiii",
                "a fg TEXT bg MANTLE",
                "b fg TEXT bg MANTLE +BOLD",
                "c fg OVERLAY2 bg MANTLE",
                "d fg #75638e bg MANTLE",
                "e fg MAUVE bg MANTLE",
                "f fg MAUVE bg #75638e",
                "g fg TEXT bg BASE",
                "h fg OVERLAY0 bg BASE",
                "i fg OVERLAY0 bg MANTLE",
            ],
        );
        assert_eq!(panel_at(&screen, " Reshape "), (108, 2));
    }

    #[test]
    fn the_bars_scroll_with_the_grid_and_only_rows_in_the_span_have_one() {
        let mut app = app_on(&["C-3 40 --- --- ---"; 64]);
        keys(&mut app, "lGvkkkkSKramp<Enter>");
        let screen = screen(&mut app, 163, 42);
        assert_drawn(
            &cut(&screen, Rect::new(0, 0, 35, 2)),
            &[
                "\u{f147d} Track 01  Chain 00  00           ",
                "aabbbbbbbbbbbbbbbbbbbbccddddddddddd",
                "        ╭Note ───────────── FX 1 ──",
                "ddddddddeaaaaaeeeeeeeeeeeeeaaaaaaee",
                "a fg OVERLAY2 bg BASE",
                "b fg TEXT bg BASE +BOLD",
                "c fg ROSEWATER bg BASE +BOLD",
                "d fg TEXT bg BASE",
                "e fg SURFACE1 bg BASE",
            ],
        );
        assert_drawn(
            &cut(&screen, Rect::new(0, 35, 35, 5)),
            &[
                " 3  38  │C-3 40     ···· ·· ···   ·",
                "aaabbbccdeeeeeeeeeeeaaaaeaaefffffff",
                " 2  39  │C-3 40 VEL 00 ─ 7F        ",
                "aaagggccdeeeeeehiiijjjjjjjjhhhhhhhh",
                " 1  3A  │C-3 40 00─7F avg 40 4 chan",
                "aaagggccdeeeeeehjjjjjjjjjjjjjjjjjjj",
                "    3B  │C-3 00 ███████████▏       ",
                "aaagggccdkkkkllhmmmmmmmmmmmmhhhhhhh",
                "        ╰──────────────────────────",
                "ddddddddddddddddddddddddddddddddddd",
                "a fg OVERLAY0 bg BASE",
                "b fg YELLOW bg BASE",
                "c fg GREEN bg BASE",
                "d fg SURFACE1 bg BASE",
                "e fg TEXT bg BASE",
                "f fg OVERLAY0 bg MANTLE",
                "g fg SUBTEXT1 bg BASE",
                "h fg TEXT bg MANTLE",
                "i fg TEXT bg MANTLE +BOLD",
                "j fg OVERLAY2 bg MANTLE",
                "k fg TEXT bg BASE ul SURFACE1 +UNDERLINED",
                "l fg BASE bg ROSEWATER ul SURFACE1 +UNDERLINED",
                "m fg #75638e bg MANTLE",
            ],
        );
    }

    #[test]
    fn a_note_column_is_a_contour_a_mark_a_row_over_the_scales_roots() {
        let mut app = app_on(&[
            "C-3 40 --- --- ---",
            "E-3 40 --- --- ---",
            "G-3 40 --- --- ---",
            "C-4 40 --- --- ---",
            "--- -- --- --- ---",
            "A-2 40 --- --- ---",
        ]);
        keys(&mut app, "vGSKreverse<Enter>");
        let screen = screen(&mut app, 163, 42);
        assert_drawn(
            &cut(&screen, Rect::new(12, 0, 24, 9)),
            &[
                " NOTE A-2 ─ C-4         ",
                "abbbbccccccccccaaaaaaaaa",
                " A-2─C-4 ~E-3 4 changed ",
                "acccccccccccccccccccccca",
                "    ●  ●           ┊    ",
                "aaaadaaeaaaaaaaaaaafaaaa",
                "       ┊   ●       ●    ",
                "aaaaaaafaaaeaaaaaaadaaaa",
                "       ┊      ●    ┊    ",
                "aaaaaaafaaaaaadaaaafaaaa",
                "       ┊   ●       ●    ",
                "aaaaaaafaaadaaaaaaaeaaaa",
                "       ┊           ┊    ",
                "aaaaaaafaaaaaaaaaaafaaaa",
                "    ●  ●           ┊    ",
                "aaaaeaadaaaaaaaaaaafaaaa",
                "────────────────────────",
                "gggggggggggggggggggggggg",
                "a fg TEXT bg MANTLE",
                "b fg TEXT bg MANTLE +BOLD",
                "c fg OVERLAY2 bg MANTLE",
                "d fg MAUVE bg MANTLE",
                "e fg #75638e bg MANTLE",
                "f fg OVERLAY0 bg MANTLE",
                "g fg SURFACE1 bg BASE",
            ],
        );
    }

    #[test]
    fn an_fx_columns_bars_are_on_its_commands_own_range() {
        let mut app = app_on(&["C-3 40 res:40 --- ---"; 8]);
        keys(&mut app, "llllllvjjjSKramp<Enter>");
        let screen = screen(&mut app, 163, 42);
        for y in 0..10 {
            println!("{}", row(&screen, y));
        }
        assert_drawn(
            &cut(&screen, Rect::new(38, 0, 24, 7)),
            &[
                " RES 00 ─ FF            ",
                "abbbccccccccaaaaaaaaaaaa",
                " 00─FF avg 80 4 changed ",
                "acccccccccccccccccccccca",
                " █████▌                 ",
                "addddddaaaaaaaaaaaaaaaaa",
                " ███████▍               ",
                "aeeeeeeeeaaaaaaaaaaaaaaa",
                " ██████████████▋        ",
                "aeeeeeeeeeeeeeeeaaaaaaaa",
                " ██████████████████████ ",
                "aeeeeeeeeeeeeeeeeeeeeeea",
                " \u{f033e} ···   ··     ···   ··",
                "acaffffffffffffaffffffff",
                "a fg TEXT bg MANTLE",
                "b fg TEXT bg MANTLE +BOLD",
                "c fg OVERLAY2 bg MANTLE",
                "d fg #75638e bg MANTLE",
                "e fg MAUVE bg MANTLE",
                "f fg OVERLAY0 bg MANTLE",
            ],
        );
    }

    #[test]
    fn a_column_whose_values_arent_amounts_has_no_bars() {
        let mut app = app_on(&["C-3 40 arp:37 --- ---"; 8]);
        keys(&mut app, "llllllvjjjSKramp<Enter>");
        assert!(app.overlays.shape.is_some());
        let screen = screen(&mut app, 163, 42);
        assert!(row(&screen, 0).contains("Chain 00"), "the title as it was");
        assert!(row(&screen, 2).contains("ARP"));
    }

    #[test]
    fn near_the_right_edge_the_panel_goes_left_so_the_bars_and_the_column_stay_clear() {
        let mut app = app_on(&["C-3 40 --- --- cut:40"; 16]);
        keys(&mut app, "$vjjjjjSKramp<Enter>");
        let screen = screen(&mut app, 110, 20);
        for y in 0..20 {
            println!("{}", row(&screen, y));
        }
        assert_eq!(panel_at(&screen, " Reshape "), (0, 2));
        assert_drawn(
            &cut(&screen, Rect::new(64, 0, 24, 9)),
            &[
                " CUT 00 ─ FF            ",
                "abbbccccccccaaaaaaaaaaaa",
                " 00─FF avg 80 6 changed ",
                "acccccccccccccccccccccca",
                " █████▌                 ",
                "addddddaaaaaaaaaaaaaaaaa",
                " ████▍▌                 ",
                "aeeeefdaaaaaaaaaaaaaaaaa",
                " ████████▊              ",
                "aeeeeeeeeeaaaaaaaaaaaaaa",
                " █████████████▎         ",
                "aeeeeeeeeeeeeeeaaaaaaaaa",
                " █████████████████▋     ",
                "aeeeeeeeeeeeeeeeeeeaaaaa",
                " ██████████████████████ ",
                "aeeeeeeeeeeeeeeeeeeeeeea",
                " \u{f033e}│  C-3                ",
                "acghiiiihhhhhhhhhhhhhhhh",
                "a fg TEXT bg MANTLE",
                "b fg TEXT bg MANTLE +BOLD",
                "c fg OVERLAY2 bg MANTLE",
                "d fg #75638e bg MANTLE",
                "e fg MAUVE bg MANTLE",
                "f fg MAUVE bg #75638e",
                "g fg SURFACE1 bg BASE",
                "h fg TEXT bg BASE",
                "i fg OVERLAY2 bg BASE",
            ],
        );
    }

    #[test]
    fn with_the_macro_panel_and_the_description_open_the_bars_and_the_panel_still_clear() {
        let mut app = app_on(&["C-3 40 --- --- ---"; 64]);
        keys(&mut app, " tlvGSKramp<Enter> d");
        let screen = screen(&mut app, 163, 42);
        for y in 0..42 {
            println!("{}", row(&screen, y));
        }
        assert_eq!(panel_at(&screen, " Reshape "), (108, 2));
    }

    /// The panel's row on screen whose label is this.
    fn labelled(buffer: &Buffer, label: &str) -> String {
        let (left, top) = panel_at(buffer, " Reshape ");
        (top..buffer.area.height)
            .map(|y| row(buffer, y))
            .map(|text| text.chars().skip(usize::from(left) + 1).collect::<String>())
            .find(|text| text.starts_with(label))
            .unwrap_or_else(|| panic!("no {label} row"))
    }

    #[test]
    fn a_list_longer_than_the_panel_shows_its_first_values_and_how_many_more() {
        let mut app = app_on(&["C-3 40 --- --- ---"; 64]);
        keys(&mut app, "lvGSKlist<Enter>j");
        keys(
            &mut app,
            &format!("i{}<Enter>", "1020304050607071".repeat(32)),
        );
        assert_eq!(app.overlays.shape.as_ref().unwrap().list.len(), 256);
        let shown = screen(&mut app, 163, 42);
        let values = labelled(&shown, "Values");
        assert!(
            values.starts_with("Values  10 20 30 40 50 60 70 71 10 +247"),
            "{values}"
        );
        // While it's typed, the end of what's typed is in sight.
        keys(&mut app, &format!("i{}", "40".repeat(20)));
        let shown = screen(&mut app, 163, 42);
        let typed = labelled(&shown, "Values");
        assert!(typed.starts_with("Values  \u{2026}"), "{typed}");
        assert!(typed.contains("40 40 40_"), "{typed}");
    }
}
