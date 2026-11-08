//! The track page: what a track holds apart from its steps, grouped by theme.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use vt_engine::track_state::LiveValues;
use vt_model::action::TrackControl;
use vt_model::constants::TRACK_ENVELOPES;
use vt_model::insert::Insert;
use vt_model::lfo::depth_text;
use vt_model::settings::{Kind, SettingId};
use vt_model::song::Track;

use crate::app::App;
use crate::input::mode::Mode;
use crate::kitty;
use crate::theme;
use crate::track_page::{
    Control, ENVELOPES, EnvelopeField, FX_COLUMNS, FX_ROWS, LFOS, LfoField, Page, SLOT_ROWS,
};
use crate::views::braille::envelope_lines;
use crate::views::status::format_swing;

/// The size of an envelope's diagram, in characters.
pub(crate) const DIAGRAM_WIDTH: usize = 28;
pub(crate) const DIAGRAM_HEIGHT: usize = 4;
/// The size of an LFO's icon, in characters, and the room for its shape's name before it.
pub(crate) const LFO_ICON_WIDTH: usize = 4;
const LFO_SHAPE_NAME: usize = 7;
/// The size of the filter's curve, in characters. With its box, it's as wide as a value and its bar.
pub(crate) const FILTER_CURVE_WIDTH: usize = 24;
pub(crate) const FILTER_CURVE_HEIGHT: usize = 3;

/// Room for a slot's number on the FX page, and for its effect's name.
const FX_INDENT: usize = 3;
const FX_EFFECT_WIDTH: usize = 10;
/// Room for a setting's name on the FX page, and for its value and bar, so its columns line up.
const FX_NAME_WIDTHS: [usize; FX_COLUMNS] = [10, 10, 10, 6, 5];
const FX_VALUE_WIDTH: usize = 12;
/// Cells in a bar on the FX page, where there are more columns to fit.
const FX_BAR_CELLS: usize = 5;
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
    level_bar_of(value, BAR_CELLS)
}

fn level_bar_of(value: u8, cells: usize) -> String {
    let eighths = (usize::from(value) * cells * 8 / 255).max(usize::from(value > 0));
    let mut bar = "█".repeat(eighths / 8);
    if eighths % 8 > 0 {
        bar.push(EIGHTHS[eighths % 8 - 1]);
    }
    format!("{bar:<cells$}")
}

/// A mark for a value either side of a centre, like pan, filling all of the bar's cells.
pub fn centre_bar(value: u8) -> String {
    format!(
        "{:<BAR_CELLS$}",
        format!("{}│", " ".repeat(bar_cell(value)))
    )
}

/// The cell of a bar a value's mark goes in.
fn bar_cell(value: u8) -> usize {
    usize::from(value) * BAR_CELLS / 256
}

/// A value's bar when something is moving it. The fill or bright mark follows what's playing, with
/// a dim mark for what's set. Over a filled cell, that mark is cut out of the fill, and the moving
/// edge takes its place while passing through its cell, as a cell can only show one of them.
pub fn bar_spans(set: u8, live: u8, centred: bool) -> Vec<Span<'static>> {
    let bright = Style::new().fg(theme::MAUVE);
    if set == live {
        let bar = if centred {
            centre_bar(set)
        } else {
            level_bar(set)
        };
        return vec![Span::styled(bar, bright)];
    }
    let bar = if centred {
        centre_bar(live)
    } else {
        level_bar(live)
    };
    let mark = bar_cell(set);
    bar.chars()
        .enumerate()
        .map(|(cell, shown)| match shown {
            '█' if cell == mark => {
                Span::styled("│", Style::new().fg(theme::BASE).bg(theme::MAUVE))
            }
            ' ' if cell == mark => Span::styled("│", theme::faint()),
            _ => Span::styled(shown.to_string(), bright),
        })
        .collect()
}

/// A setting's bar on the FX page: what's playing, with a dim mark for what's set if that differs.
fn fx_bar_spans(set: u8, live: u8) -> Vec<Span<'static>> {
    let bright = Style::new().fg(theme::MAUVE);
    let mark = usize::from(set) * FX_BAR_CELLS / 256;
    level_bar_of(live, FX_BAR_CELLS)
        .chars()
        .enumerate()
        .map(|(cell, shown)| match shown {
            '█' if set != live && cell == mark => {
                Span::styled("│", Style::new().fg(theme::BASE).bg(theme::MAUVE))
            }
            ' ' if set != live && cell == mark => Span::styled("│", theme::faint()),
            _ => Span::styled(shown.to_string(), bright),
        })
        .collect()
}

/// What a control with a bar is playing with right now.
fn live_value(control: Control, live: &LiveValues) -> Option<u8> {
    match control {
        Control::Level => Some(live.level),
        Control::Base(TrackControl::Pan) => Some(live.pan),
        Control::Setting(setting) => live.machine.get(setting).copied(),
        Control::Base(TrackControl::Cutoff) => Some(live.cutoff),
        Control::Base(TrackControl::Resonance) => Some(live.resonance),
        Control::Base(TrackControl::SendA) => Some(live.send_a),
        Control::Base(TrackControl::SendB) => Some(live.send_b),
        _ => None,
    }
}

/// The choice a machine's setting that's a list is on, such as 6-op's patch, shown on its row.
fn choice_name(control: Control, track: &Track) -> Option<String> {
    let id = control.setting_on(track)?;
    let Kind::Stepped { choices, .. } = id.entry().kind else {
        return None;
    };
    choices
        .get(usize::from(track.setting(id)))
        .map(|choice| (*choice).to_owned())
}

/// The value a control shows a bar for, and how. True is a mark either side of a centre.
fn bar_value(control: Control, track: &Track) -> Option<(u8, bool)> {
    let level = |value| Some((value, false));
    match control {
        Control::Level => level(track.level),
        Control::Base(TrackControl::Pan) => Some((track.pan, true)),
        // Those with a middle that's a setting of its own show where they are around it.
        Control::Setting(_) => {
            let id = control.setting_on(track)?;
            match id.entry().kind {
                Kind::Continuous { middle: false, .. } => level(track.setting(id)),
                Kind::Continuous { middle: true, .. } => Some((track.setting(id), true)),
                Kind::Stepped { .. } => None,
            }
        }
        Control::Base(TrackControl::Cutoff) => level(track.cutoff),
        Control::Base(TrackControl::Resonance) => level(track.resonance),
        Control::Base(TrackControl::SendA) => level(track.send_a),
        Control::Base(TrackControl::SendB) => level(track.send_b),
        Control::Base(TrackControl::Drive) => level(track.drive),
        Control::Base(TrackControl::LowCut) => level(track.low_cut),
        Control::Base(TrackControl::HighCut) => level(track.high_cut),
        _ => None,
    }
}

/// A setting's value, with a jump's label over its first letter if it has one.
pub(crate) fn value_spans(
    app: &App,
    control: Control,
    text: String,
    style: Style,
) -> Vec<Span<'static>> {
    let label = app.field_jump.as_ref().and_then(|jump| jump.label(control));
    match label {
        Some(label) => vec![
            Span::styled(label.to_string(), theme::jump_label()),
            Span::styled(text.chars().skip(1).collect::<String>(), style),
        ],
        None => vec![Span::styled(text, style)],
    }
}

/// A setting's name in its column, with any labels from a jump over the letters just after what's
/// been typed, as flash.nvim draws them, since that's where the eye is while searching.
pub(crate) fn name_spans(app: &App, name: &str, labels: &str, room: usize) -> Vec<Span<'static>> {
    let letters: Vec<char> = name.chars().collect();
    let typed = app.field_jump.as_ref().and_then(|jump| {
        let at = name.to_lowercase().find(&jump.query)?;
        Some(at + jump.query.len())
    });
    let Some(typed) = typed.filter(|_| !labels.is_empty()) else {
        return vec![Span::styled(format!("{name:<room$}"), theme::dim())];
    };
    let count = labels.chars().count();
    let before = typed.min(room.saturating_sub(count));
    let after: String = letters.iter().skip(before + count).collect();
    let used = before + count + after.chars().count();
    vec![
        Span::styled(
            letters[..before.min(letters.len())]
                .iter()
                .collect::<String>(),
            theme::dim(),
        ),
        Span::styled(labels.to_owned(), theme::jump_label()),
        Span::styled(after, theme::dim()),
        Span::raw(" ".repeat(room.saturating_sub(used))),
    ]
}

/// A row of the Sound page: a control on the left, and on the right perhaps one, or a drawing.
fn sound_row(app: &App, track: &Track, controls: &[Control]) -> Line<'static> {
    let current = app.track_control();
    let mut spans = Vec::new();
    let mut used = 0;
    for control in controls {
        // A sound track's output is always on the right, even on a row with nothing on the left. A
        // return or the master has only output, so it's all on the left.
        if control.column() > 0 && track.is_sound() {
            spans.push(Span::raw(" ".repeat(SOUND_COLUMN.saturating_sub(used))));
        }
        let label = app
            .field_jump
            .as_ref()
            .and_then(|jump| jump.label(*control));
        spans.extend(name_spans(
            app,
            control.label_on(track),
            &label.map_or_else(String::new, String::from),
            SOUND_LABEL,
        ));
        used = SOUND_LABEL;
        let here = *control == current;
        let on_fine = here && app.track_fine;
        let typing = here && app.mode == Mode::Insert;
        let mut text = if typing && !on_fine {
            format!("{}_", app.entry.text)
        } else {
            value(*control, track, app.song().swing)
        };
        if *control == Control::Sample {
            text = text.chars().take(FILE_WIDTH).collect();
        }
        let style = if here && !on_fine {
            theme::cursor(app.mode)
        } else if *control == Control::Swing && track.swing.is_none() {
            theme::dim()
        } else {
            Style::new()
        };
        used += text.chars().count();
        spans.extend(value_spans(app, *control, text, style));
        // A setting that sweeps shows its fine part, set back, and any other with a bar keeps the
        // room so the bars line up.
        if let Some(id) = control.fine_on(track) {
            let fine = if typing && on_fine {
                format!("{:<2}", format!("{}_", app.entry.text))
            } else {
                format!("{:02X}", track.fine(id))
            };
            let style = if on_fine {
                theme::cursor(app.mode)
            } else {
                theme::dim()
            };
            used += 2;
            spans.push(Span::styled(fine, style));
        } else if bar_value(*control, track).is_some() {
            used += 2;
            spans.push(Span::raw("  "));
        }
        if let Some(name) = choice_name(*control, track) {
            // A patch number says nothing as a bar, so its name takes the bar's place.
            used += 1 + name.chars().count();
            spans.push(Span::raw(" "));
            spans.push(Span::styled(name, Style::new().fg(theme::MAUVE)));
        } else if let Some((value, centred)) = bar_value(*control, track) {
            let live = app
                .live
                .and_then(|live| live_value(*control, &live))
                .unwrap_or(value);
            used += 1 + BAR_START.chars().count() + BAR_CELLS + BAR_END.chars().count();
            spans.push(Span::raw(" "));
            spans.push(Span::styled(BAR_START, theme::faint()));
            spans.extend(bar_spans(value, live, centred));
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
        Control::Setting(_) => control
            .setting_on(track)
            .map_or_else(String::new, |id| hex(track.setting(id))),
        Control::Base(TrackControl::Cutoff) => hex(track.cutoff),
        Control::Base(TrackControl::Resonance) => hex(track.resonance),
        Control::Base(TrackControl::Pan) => hex(track.pan),
        Control::Base(TrackControl::SendA) => hex(track.send_a),
        Control::Base(TrackControl::SendB) => hex(track.send_b),
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
                    .map_or_else(|| "---".to_owned(), |d| d.name_on(track)),
                EnvelopeField::Attack => ms(envelope.attack_ms),
                EnvelopeField::Decay => ms(envelope.decay_ms),
                EnvelopeField::Sustain => format!("{}%", envelope.sustain),
                EnvelopeField::Release => ms(envelope.release_ms),
                // The first envelope on `Vol` shapes each note, rather than moving a value.
                EnvelopeField::Depth
                    if slot == 0 && envelope.destination == Some(SettingId::VOL) =>
                {
                    "--".to_owned()
                }
                EnvelopeField::Depth => depth_text(envelope.depth),
                EnvelopeField::Reset => if envelope.reset { "on" } else { "off" }.to_owned(),
            }
        }
        Control::Insert(slot) => track.inserts[slot].name().to_owned(),
        Control::InsertParam(slot, param) => {
            let insert = track.inserts[slot];
            match insert.params().get(param) {
                Some(found) if !found.choices.is_empty() => found
                    .choices
                    .get(usize::from(insert.value(param)))
                    .copied()
                    .unwrap_or("")
                    .to_owned(),
                Some(_) => hex(insert.value(param)),
                None => String::new(),
            }
        }
        Control::Lfo(slot, field) => {
            let lfo = &track.lfos[slot];
            match field {
                LfoField::Destination => lfo
                    .destination
                    .map_or_else(|| "---".to_owned(), |d| d.name_on(track)),
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
        Control::Lfo(_, LfoField::Shape) => LFO_SHAPE_NAME + LFO_ICON_WIDTH + 2,
        Control::Base(TrackControl::SendB) | Control::Lfo(_, LfoField::Retrig) => 8,
        Control::Base(TrackControl::LowCut | TrackControl::HighCut) => 9,
        Control::Envelope(
            _,
            EnvelopeField::Attack
            | EnvelopeField::Decay
            | EnvelopeField::Sustain
            | EnvelopeField::Release,
        ) => 9,
        // Room for a slot's setting, like `2 Ping-pong`, or one that's gone, like `Material ---`.
        Control::Lfo(_, LfoField::Destination)
        | Control::Envelope(_, EnvelopeField::Destination) => 13,
        _ => 7,
    }
}

/// The column names over some rows, with a jump's labels for those rows on each name.
fn heading<const N: usize>(app: &App, rows: &[[Control; N]]) -> Line<'static> {
    let mut spans = vec![Span::raw(" ".repeat(NAME_WIDTH))];
    for column in 0..N {
        let control = rows[0][column];
        let labels: String = rows
            .iter()
            .filter_map(|row| app.field_jump.as_ref()?.label(row[column]))
            .collect();
        spans.extend(name_spans(app, control.label(), &labels, width(control)));
    }
    Line::from(spans)
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
        let mut gap = room.saturating_sub(text.chars().count());
        let length = text.chars().count();
        spans.extend(value_spans(app, *control, text, style));
        // A drawing of an LFO's shape follows its name, with the column the same width without one.
        if let (Control::Lfo(slot, LfoField::Shape), Some(ids)) = (*control, app.lfo_images) {
            let before = LFO_SHAPE_NAME.saturating_sub(length);
            spans.push(Span::raw(" ".repeat(before)));
            let icon = kitty::placeholder_lines(LFO_ICON_WIDTH, 1).concat();
            spans.push(Span::styled(
                icon,
                Style::new().fg(kitty::id_color(ids[slot])),
            ));
            gap = room.saturating_sub(length + before + LFO_ICON_WIDTH);
        }
        spans.push(Span::raw(" ".repeat(gap)));
    }
    Line::from(spans)
}

/// Which envelope the cursor is on, if any.
pub(crate) fn envelope_being_edited(app: &App) -> Option<usize> {
    match app.track_control() {
        Control::Envelope(slot, _) => Some(slot),
        _ => None,
    }
}

/// Whether the cursor is on something the filter's curve shows.
pub(crate) fn filter_being_edited(app: &App) -> bool {
    matches!(
        app.track_control(),
        Control::FilterType
            | Control::Base(
                TrackControl::Cutoff
                    | TrackControl::Resonance
                    | TrackControl::Drive
                    | TrackControl::LowCut
                    | TrackControl::HighCut
            )
    )
}

/// Which LFO the cursor is on, if any.
pub(crate) fn lfo_being_edited(app: &App) -> Option<usize> {
    match app.track_control() {
        Control::Lfo(slot, _) => Some(slot),
        _ => None,
    }
}

/// Each envelope's shape, side by side, with the one being edited picked out.
fn diagrams(app: &App, track: &Track) -> Vec<Line<'static>> {
    let current = envelope_being_edited(app);
    let shapes = track.envelopes.map(|envelope| match app.envelope_images {
        Some(_) => kitty::placeholder_lines(DIAGRAM_WIDTH, DIAGRAM_HEIGHT),
        None => envelope_lines(&envelope, DIAGRAM_WIDTH, DIAGRAM_HEIGHT),
    });
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
                let name = track.envelopes[slot].destination.map_or_else(
                    || "---".to_owned(),
                    |destination| destination.name_on(track),
                );
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
                .map(|slot| {
                    // A placeholder's colour says which image it's part of.
                    let shape = match app.envelope_images {
                        Some(ids) => Style::new().fg(kitty::id_color(ids[slot])),
                        None => style(slot),
                    };
                    Span::styled(format!("{}  ", shapes[slot][row]), shape)
                })
                .collect::<Vec<_>>(),
        )
    }));
    lines
}

/// One row of a slot on the FX page, with its settings in columns, read from the effect's list. The
/// effect is only drawn in its first row.
fn fx_row(app: &App, track: &Track, controls: &[Control], first: bool) -> Line<'static> {
    let current = app.track_control();
    let mut spans = Vec::new();
    for control in controls {
        let selected = *control == current;
        let text = if selected && app.mode == Mode::Insert {
            format!("{}_", app.entry.text)
        } else {
            value(*control, track, app.song().swing)
        };
        let style = if selected {
            theme::cursor(app.mode)
        } else {
            Style::new()
        };
        let label = app
            .field_jump
            .as_ref()
            .and_then(|jump| jump.label(*control));
        let labels = label.map_or_else(String::new, String::from);
        match *control {
            Control::Insert(slot) if first => {
                spans.push(Span::styled(
                    format!("{:<FX_INDENT$}", slot + 1),
                    theme::dim(),
                ));
                let room = FX_EFFECT_WIDTH.saturating_sub(text.chars().count());
                spans.extend(value_spans(app, *control, text, style));
                spans.push(Span::raw(" ".repeat(room)));
            }
            Control::Insert(_) => {
                spans.push(Span::raw(" ".repeat(FX_INDENT + FX_EFFECT_WIDTH)));
            }
            Control::InsertParam(slot, param) if control.exists_on(track) => {
                let name = control.name_on(track);
                let room = FX_NAME_WIDTHS[param / 2];
                spans.extend(name_spans(app, name, &labels, room));
                let insert = track.inserts[slot];
                let used = if insert.params()[param].choices.is_empty() {
                    let set = insert.value(param);
                    let live = app.live.map_or(set, |live| live.inserts[slot][param]);
                    let used = text.chars().count() + 1 + FX_BAR_CELLS + 2;
                    spans.extend(value_spans(app, *control, text, style));
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(BAR_START, theme::faint()));
                    spans.extend(fx_bar_spans(set, live));
                    spans.push(Span::styled(BAR_END, theme::faint()));
                    used
                } else {
                    let used = text.chars().count();
                    spans.extend(value_spans(app, *control, text, style));
                    used
                };
                spans.push(Span::raw(" ".repeat(FX_VALUE_WIDTH.saturating_sub(used))));
            }
            _ => {}
        }
    }
    Line::from(spans)
}

/// The filter's curve in a box, under its rows.
fn filter_curve(id: u32) -> Vec<Line<'static>> {
    let rule = "─".repeat(FILTER_CURVE_WIDTH);
    let edge = theme::faint();
    let mut lines = vec![Line::styled(format!("┌{rule}┐"), edge)];
    for row in kitty::placeholder_lines(FILTER_CURVE_WIDTH, FILTER_CURVE_HEIGHT) {
        lines.push(Line::from(vec![
            Span::styled("│", edge),
            Span::styled(row, Style::new().fg(kitty::id_color(id))),
            Span::styled("│", edge),
        ]));
    }
    lines.push(Line::styled(format!("└{rule}┘"), edge));
    lines
}

/// Draw the view.
pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    // While the keyboard plays the sound, the page sits in a box of that mode's colour.
    let area = if app.mode == Mode::Keys {
        let block = Block::new()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(theme::YELLOW))
            .title(" Keys ");
        let inner = block.inner(area);
        frame.render_widget(block, area);
        inner
    } else {
        area
    };
    let Some(track) = app.song().tracks.get(app.cursor.track) else {
        frame.render_widget(Paragraph::new("This track doesn't exist."), area);
        return;
    };
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let tabs = Page::ALL.map(|page| {
        let text = format!(" {} ", page.name());
        if page == app.track_page {
            Span::styled(text, bold)
        } else {
            Span::styled(text, theme::dim())
        }
    });
    let name = if track.is_sound() {
        format!("Track {:02} {} ", app.cursor.track + 1, track.name)
    } else {
        format!("{} ", track.kind.title())
    };
    let mut title = vec![Span::styled(name, bold)];
    title.extend(tabs);
    let mut lines = vec![Line::from(title), Line::raw("")];
    match app.track_page {
        Page::Sound => {
            for controls in app.page_rows() {
                // The filter stands apart from the machine above it.
                if controls.first() == Some(&Control::FilterType) {
                    lines.push(Line::raw(""));
                }
                lines.push(sound_row(app, track, controls));
            }
            if track.is_sound() {
                lines.extend(app.filter_image.map_or_else(Vec::new, filter_curve));
            }
        }
        Page::Fx => {
            for (slot, rows) in FX_ROWS.chunks(SLOT_ROWS).enumerate() {
                lines.push(fx_row(app, track, rows[0], true));
                // An empty slot has nothing for its second row, and the slots stay apart.
                if track.inserts[slot] != Insert::Empty {
                    lines.push(fx_row(app, track, rows[1], false));
                }
                lines.push(Line::raw(""));
            }
        }
        Page::Mod => {
            lines.push(heading(app, &ENVELOPES));
            for (slot, controls) in ENVELOPES.iter().enumerate() {
                lines.push(row(app, track, &format!("Env {}", slot + 1), controls));
            }
            lines.push(Line::raw(""));
            lines.push(heading(app, &LFOS));
            for (slot, controls) in LFOS.iter().enumerate() {
                lines.push(row(app, track, &format!("LFO {}", slot + 1), controls));
            }
            lines.push(Line::raw(""));
            lines.extend(diagrams(app, track));
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}
