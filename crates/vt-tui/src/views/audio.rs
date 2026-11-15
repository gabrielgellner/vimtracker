//! The audio page, the project page's Audio sub-page: this machine's output playing, its rate and
//! buffer, the input the recorder hears, how it's keeping up, whether vimtracker drives the
//! Roto-Control, and whether the mouse wheel turns the cursor's setting. It's drawn from the
//! page's own row and what it reads of the program through a [`Ctx`].

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Row};

use crate::app::App;
use crate::audio_page::{AudioPage, AudioRow, buffer_ms};
use crate::page::Ctx;
use crate::theme;
use crate::views::widgets::grid::CellWidths;
use crate::views::widgets::param::{Face, ParamCell, ParamState, Tone};
use crate::views::widgets::setting::{self, Focus};
use crate::views::widgets::table::{Columns, blank_row, take_top};
use crate::views::{project, system};

/// Room for a row's name.
const NAME_WIDTH: u16 = 12;
/// The room a row's cell has: its name, then its value, which has the rest of the row. None has
/// a bar.
const CELL: CellWidths = CellWidths {
    name: NAME_WIDTH,
    value: 0,
    bar_cells: 0,
    gap: 0,
};

/// A row's value as the page shows it.
fn value(ctx: &Ctx, row: AudioRow) -> String {
    let audio = ctx.audio();
    let wheel = ctx.wheel();
    match row {
        AudioRow::Device if audio.device.is_empty() => "None".to_owned(),
        AudioRow::Device => audio.device.clone(),
        AudioRow::Rate => format!("{} Hz", ctx.rate()),
        AudioRow::Buffer => {
            let own = if audio.choice.buffer.is_none() {
                ", the device's own"
            } else {
                ""
            };
            match audio.buffer.or(audio.choice.buffer) {
                Some(frames) => format!(
                    "{frames} frames, {:.1} ms{own}",
                    buffer_ms(frames, ctx.rate())
                ),
                None => "The device's own".to_owned(),
            }
        }
        AudioRow::Input => match (&audio.input, &audio.choice.input) {
            (Some(name), _) => format!("{name}, heard only by the recorder"),
            (None, Some(_)) => "Chosen, but it couldn't be opened".to_owned(),
            (None, None) => "Off".to_owned(),
        },
        AudioRow::Roto => ctx.roto().connection.describe(),
        AudioRow::Wheel if wheel.on => {
            "Scrolling turns the setting under the cursor. Shift-drag selects text".to_owned()
        }
        AudioRow::Wheel => "Off: the terminal keeps its mouse".to_owned(),
        AudioRow::WheelDirection if wheel.reversed => {
            "Against the fingers: up or right lowers".to_owned()
        }
        AudioRow::WheelDirection => {
            "With the fingers, as a bar fills: up or right raises".to_owned()
        }
    }
}

/// Whether an off/on row, the Roto's or the wheel's, is on.
fn switched_on(ctx: &Ctx, row: AudioRow) -> bool {
    match row {
        AudioRow::Wheel => ctx.wheel().on,
        _ => ctx.roto().setting.on,
    }
}

/// A row as a setting's cell: its name, and its value, a toggle on the Roto's and the wheel's
/// rows, the cursor's where it's on it.
fn cell<'a>(ctx: &Ctx<'a>, row: AudioRow, here: bool) -> ParamCell<'a> {
    let face = match row {
        AudioRow::Roto | AudioRow::Wheel => Face::Toggle {
            on: switched_on(ctx, row),
        },
        _ => Face::Word,
    };
    ParamCell {
        name: row.name().into(),
        face,
        text: value(ctx, row),
        place: None,
        state: ParamState::new(Tone::Plain),
        look: ctx.look(Focus::at(here, false)),
    }
}

/// An off/on row's value, the Roto's or the wheel's: its switch in its own colour, or the
/// cursor's, as its cell draws it, then what it says, dim while it's off.
fn switch_value(on: bool, switch: Line<'static>, says: String) -> Line<'static> {
    let style = if on { Style::new() } else { theme::dim() };
    let mut line = switch;
    line.push_span(Span::raw(" "));
    line.push_span(Span::styled(says, style));
    line
}

/// Draw the app's audio page.
pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    draw_page(frame, area, &app.audio_page, &app.ctx());
}

/// Draw the audio page, as its `Page::draw`.
pub(crate) fn draw_page(frame: &mut Frame, area: Rect, page: &AudioPage, ctx: &Ctx) {
    let mut rest = area;
    frame.render_widget(Paragraph::new(project::title(ctx)), take_top(&mut rest, 2));
    // A row's name and then its value, which has the rest of the room.
    let columns = Columns::new([NAME_WIDTH, 0]).open_ended();
    let named = |name: Line<'static>, value: Line<'static>| -> Row<'static> {
        let mut cells = columns.row();
        cells.cell(name).cell(value);
        cells.done()
    };
    let whole = |line: Line<'static>| {
        let mut cells = columns.row();
        cells.span(line, 2);
        cells.done()
    };
    let mut rows = Vec::new();
    for (index, row) in AudioRow::ALL.into_iter().enumerate() {
        let here = index == page.row;
        let cell = cell(ctx, row, here);
        let parts = cell.parts(CELL);
        let line = match row {
            AudioRow::Roto | AudioRow::Wheel => {
                switch_value(switched_on(ctx, row), parts.value, cell.text)
            }
            // Dim while the wheel's off, as it does nothing then.
            AudioRow::WheelDirection if !ctx.wheel().on && !here => {
                Line::styled(cell.text, theme::dim())
            }
            _ => Line::styled(cell.text, setting::style(&cell.look, false)),
        };
        rows.push(named(parts.name, line));
    }
    rows.push(blank_row());
    let audio = ctx.audio();
    if audio.running {
        // The device's own delay is in this too, which on Bluetooth is most of it.
        let latency = audio.latency_us as f64 / 1000.0;
        rows.push(named(
            Line::styled("Latency", theme::dim()),
            Line::raw(format!("{latency:.1} ms, out of the device")),
        ));
        let trouble = audio.late > 0 || audio.xruns > 0;
        let figures = Span::styled(
            format!(
                "CPU {}%  late {}  xruns {}",
                ctx.cpu_percent(),
                audio.late,
                audio.xruns
            ),
            if trouble {
                theme::warning()
            } else {
                Style::new()
            },
        );
        // The status line says what the leaf means.
        let line: Line = system::leaf(ctx.slowed())
            .into_iter()
            .chain([figures])
            .collect();
        rows.push(named(Line::styled("Keeping up", theme::dim()), line));
    } else {
        rows.push(whole(Line::styled(
            "Stopped. Choose a device to play again.",
            theme::warning(),
        )));
    }
    rows.push(blank_row());
    rows.push(whole(Line::styled(
        "A smaller buffer answers keys sooner, but drops out sooner when the CPU is busy.",
        theme::faint(),
    )));
    columns.stack(frame, &mut rest, rows);
}
