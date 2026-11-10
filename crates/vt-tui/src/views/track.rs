//! The track page: what a track holds apart from its steps, grouped by theme.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Row};
use vt_engine::engine::Transport;
use vt_engine::track_state::LiveValues;
use vt_model::action::TrackControl;
use vt_model::constants::TRACK_ENVELOPES;
use vt_model::insert::Insert;
use vt_model::lfo::depth_text;
use vt_model::settings::{Kind, SettingId, gate_percent};
use vt_model::song::{Song, SynthVoice, Track, TrackKind, reset_name};
use vt_model::speed::Speed;

use crate::app::App;
use crate::banking::sample_text;
use crate::glyphs;
use crate::input::mode::Mode;
use crate::kitty;
use crate::theme;
use crate::track_page::{
    ACCUMULATOR, ARPEGGIO, AccumulatorField, ArpField, Control, ENVELOPES, EnvelopeField,
    FX_COLUMNS, FX_ROWS, LFOS, LfoField, Page, SLOT_ROWS,
};
use crate::views::phrase::level_glyph;
use crate::views::widgets;
use crate::views::widgets::bar::{BAR_ENDS, ValueBar};
use crate::views::widgets::group;
use crate::views::widgets::image;
use crate::views::widgets::meter::{hanging_cell, hanging_eighths};
use crate::views::widgets::setting::{self, jump_label, name_line, value_spans};
use crate::views::widgets::table::{Cells, Columns, blank_row, take_top};

/// The size of an envelope's diagram, in characters.
pub(crate) const DIAGRAM_WIDTH: usize = 28;
pub(crate) const DIAGRAM_HEIGHT: usize = 4;
/// The room between envelopes' diagrams.
const DIAGRAM_GAP: usize = 2;
/// The room for an LFO's shape's name, the longest being `Square`, and for its icon after it: the
/// icon, which is one character, and a space more.
const LFO_SHAPE_NAME: u16 = 6;
const LFO_ICON_ROOM: u16 = 2;
/// The size of the filter's curve, in characters. With its box, it's as wide as a value and its bar.
pub(crate) const FILTER_CURVE_WIDTH: usize = 24;
pub(crate) const FILTER_CURVE_HEIGHT: usize = 3;
/// The size of a sample's waveform, in characters. It sits beside the filter's curve, under the
/// output's column, and is as big, so the two boxes line up.
pub(crate) const WAVEFORM_WIDTH: usize = FILTER_CURVE_WIDTH;
pub(crate) const WAVEFORM_HEIGHT: usize = FILTER_CURVE_HEIGHT;

/// The size of a compressor's curve on the FX page, in characters, about square, and the rows a
/// slot's drawing takes, which are its two rows of settings.
pub(crate) const DYNAMICS_CURVE_WIDTH: usize = 4;
pub(crate) const SLOT_DRAWING_HEIGHT: usize = 2;
/// The size of an EQ's curve on the FX page, in characters: all the room left before the meters.
pub(crate) const EQ_CURVE_WIDTH: usize = 10;
/// How much a gain reduction meter covers, in dB, from the top of its two rows to the bottom.
const REDUCTION_METER_DB: f32 = 24.0;

/// Room for a slot's number on the FX page, and for its effect's name.
const FX_INDENT: u16 = 3;
const FX_EFFECT_WIDTH: u16 = 10;
/// Room for a setting's name on the FX page, for its value and a gap, and for its bar and a gap,
/// so its columns line up. A choice takes the value's and the bar's room.
const FX_NAME_WIDTHS: [u16; FX_COLUMNS] = [10, 10, 10, 6, 5];
const FX_VALUE_WIDTH: u16 = 3;
const FX_BAR_WIDTH: u16 = FX_BAR_CELLS as u16 + BAR_ENDS + 2;
/// A slot's drawing comes after all its columns of settings, as a compressor's `Clean` type has
/// five, and starts a space after them.
const FX_DRAWING_GAP: u16 = 1;
/// The table column a slot's drawing starts in: after its number and effect, three for each
/// column of settings, and the gap.
const FX_DRAWING_COLUMN: usize = 2 + 3 * FX_COLUMNS + 1;
/// Cells in a bar on the FX page, where there are more columns to fit.
const FX_BAR_CELLS: usize = 5;
/// Room for a section's name.
const NAME_WIDTH: u16 = 10;
/// Cells in a value's bar, each split into eighths. An odd number, so a centre has a cell.
const BAR_CELLS: usize = 9;
/// The Sound page's columns, in each half: a setting's name, its value, its fine part and a gap,
/// and its bar. A word takes all of those after the name, and the room after the bar, as a long
/// file name needs; a choice's name takes the bar's room and what's after it. A gap parts the
/// halves.
const SOUND_LABEL: u16 = 10;
const SOUND_VALUE: u16 = 2;
const SOUND_FINE: u16 = 3;
const SOUND_BAR: u16 = BAR_CELLS as u16 + BAR_ENDS;
const SOUND_AFTER_BAR: u16 = FILE_WIDTH - SOUND_VALUE - SOUND_FINE - SOUND_BAR;
const SOUND_GAP: u16 = 2;
const SOUND_HALF: [u16; 6] = [
    SOUND_LABEL,
    SOUND_VALUE,
    SOUND_FINE,
    SOUND_BAR,
    SOUND_AFTER_BAR,
    SOUND_GAP,
];
/// Where the right half starts, the output's column, which the waveform sits under.
const SOUND_COLUMN: u16 = SOUND_LABEL + FILE_WIDTH + SOUND_GAP;
/// The longest file name shown on the Sound page.
const FILE_WIDTH: u16 = 24;

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
        Control::Base(TrackControl::Pitch) => Some((track.pitch, true)),
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

/// Whether a setting is left at its default, which matters less, so its value is dim: a track
/// following the project's groove shows what it plays, dimmed as it isn't its own.
fn at_default(control: Control, track: &Track) -> bool {
    control == Control::Groove && track.groove.is_none()
        || control == Control::GateLength && track.gate_length.is_none()
        || control == Control::Speed && track.speed == Speed::Sixteenth
        || control == Control::Reset && track.reset.is_none()
        || control == Control::Glide && track.glide == 0
        || control == Control::FilterMode && !track.filter.has_modes()
}

/// The Sound page's columns: its two halves side by side.
fn sound_columns() -> Columns {
    Columns::new([SOUND_HALF, SOUND_HALF].concat())
}

/// A row of the Sound page: a control on the left, and on the right perhaps one.
fn sound_row(app: &App, track: &Track, controls: &[Control], columns: &Columns) -> Row<'static> {
    let mut cells = columns.row();
    sound_cells(app, track, controls, &mut cells);
    cells.done()
}

/// A Sound page row's controls, put in their halves.
fn sound_cells(app: &App, track: &Track, controls: &[Control], cells: &mut Cells<'_>) {
    let current = app.track_control();
    for control in controls {
        // A sound track's output is always on the right, even on a row with nothing on the left,
        // and so is a group's, with its tracks on the left. A return or the master has only
        // output, so it's all on the left.
        let half = usize::from(control.column() > 0 && (track.is_sound() || track.is_group()));
        cells.to(half * SOUND_HALF.len());
        cells.cell(name_line(
            app,
            control.label_on(track),
            &jump_label(app, *control),
            SOUND_LABEL,
        ));
        let here = *control == current;
        let on_fine = here && app.track_fine;
        let on_value = here && !on_fine;
        let text = setting::text(app, on_value, || value(*control, track, app.song()));
        let mut style = setting::style(app, on_value, at_default(*control, track));
        // The group a track is in is in its colour, as on the mixer.
        if *control == Control::Group
            && !on_value
            && let Some(tint) = group::style(app.song(), app.cursor.track)
        {
            style = tint;
        }
        let value = Line::from(value_spans(app, *control, text, style));
        // A setting that sweeps shows its fine part, set back.
        let fine = control.fine_on(track).map(|id| {
            let fine = setting::text(app, on_fine, || format!("{:02X}", track.fine(id)));
            Line::styled(fine, setting::style(app, on_fine, true))
        });
        let name = if control.setting_on(track) == Some(SettingId::SLICE) {
            Some(app.slice_choice(track))
        } else {
            choice_name(*control, track)
        };
        let bar = bar_value(*control, track);
        if name.is_none() && bar.is_none() && fine.is_none() {
            // A word, with all the room after the name.
            cells.span(value, 4);
            continue;
        }
        cells.cell(value).cell(fine.unwrap_or_default());
        if let Some(name) = name {
            // A patch number says nothing as a bar, so its name takes the bar's place.
            cells.span(Line::styled(name, Style::new().fg(theme::MAUVE)), 2);
        } else if let Some((value, centred)) = bar {
            let live = app
                .live
                .and_then(|live| live_value(*control, &live))
                .unwrap_or(value);
            cells.cell(
                ValueBar::new(value, BAR_CELLS)
                    .playing(live)
                    .centred(centred),
            );
        }
    }
}

/// A group's Sound page: the tracks it sums down the left, where a sound track's machine is, each
/// with its own mute or solo, and its output on the right, where a sound track's is, so its level
/// stays in the same place as its tracks'. A list longer than the page ends with how many more.
fn group_rows(app: &App, track: &Track, columns: &Columns, room: usize) -> Vec<Row<'static>> {
    let song = app.song();
    let members: Vec<usize> = song.group_members(app.cursor.track).collect();
    let controls = app.page_rows();
    let lines = members.len().max(1).min(room).max(controls.len());
    let cut = members.len() > lines;
    let mut rows = Vec::new();
    for line in 0..lines {
        let mut cells = columns.row();
        let label = if line == 0 { "Tracks" } else { "" };
        cells.cell(Line::styled(label, theme::dim()));
        let member = members.get(line).copied();
        let text = match member {
            _ if cut && line == lines - 1 => {
                Line::styled(format!("+{} more", members.len() - line), theme::faint())
            }
            None if line == 0 => Line::styled("None", theme::dim()),
            None => Line::default(),
            Some(member) => member_line(app, member),
        };
        cells.span(text, 4);
        if let Some(controls) = controls.get(line) {
            sound_cells(app, track, controls, &mut cells);
        }
        rows.push(cells.done());
    }
    rows
}

/// One of a group's tracks, by its number and name, with its own mute or solo after it, dim while
/// it's muted.
fn member_line(app: &App, member: usize) -> Line<'static> {
    let song = app.song();
    let name = format!("{:02} {}", member + 1, song.tracks[member].name);
    let live = app.levels.get(member).copied().unwrap_or_default();
    let style = if live.mute {
        theme::dim()
    } else {
        Style::new()
    };
    let mut spans = vec![Span::styled(name.trim_end().to_owned(), style)];
    let glyph = if live.mute {
        Some(glyphs::MUTE)
    } else if live.solo {
        Some(glyphs::SOLO)
    } else {
        None
    };
    if let Some(glyph) = glyph {
        spans.push(Span::styled(format!(" {glyph}"), theme::active()));
    }
    Line::from(spans)
}

/// A setting's value as text.
pub(crate) fn value(control: Control, track: &Track, song: &Song) -> String {
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
        Control::Base(TrackControl::Pitch) => hex(track.pitch),
        Control::Group => track
            .group
            .map_or_else(|| "Off".to_owned(), |group| format!("G{group}")),
        Control::Choke => track
            .choke
            .map_or_else(|| "Off".to_owned(), |group| group.to_string()),
        Control::GateLength => format!("{}%", gate_percent(track.gate_length())),
        Control::Speed => track.speed.name().to_owned(),
        Control::Reset => reset_name(track.reset),
        // Off, `Acid` slides by its own `Slide`, as a 303 does.
        Control::Glide if track.glide == 0 && track.voice() == Some(SynthVoice::Acid) => {
            "Slide".to_owned()
        }
        Control::Glide => SettingId::GLIDE
            .entry()
            .kind
            .shown(u16::from(track.glide) << 8),
        Control::Sample => sample_text(song, track.sample()),
        Control::Mute | Control::Solo => String::new(),
        Control::FilterType => track.filter.name().to_owned(),
        // The MS-20's have none, so it doesn't apply.
        Control::FilterMode if !track.filter.has_modes() => "--".to_owned(),
        Control::FilterMode => track.filter.plays(track.filter_mode).name().to_owned(),
        Control::Level => hex(track.level),
        Control::Groove => track.groove.unwrap_or(song.groove).name().to_owned(),
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
        Control::Accumulator(field) => {
            let accumulation = track.accumulation;
            match field {
                AccumulatorField::Up => accumulation.up.to_string(),
                AccumulatorField::Down => accumulation.down.to_string(),
                AccumulatorField::Moves => accumulation.moves.name().to_owned(),
                AccumulatorField::AtLimit => accumulation.at_limit.name().to_owned(),
                AccumulatorField::Adds => accumulation.adds.name().to_owned(),
                AccumulatorField::Reset => accumulation.reset.name(),
            }
        }
        Control::Arpeggio(field) => {
            let arpeggio = track.arpeggio;
            match field {
                ArpField::Rate => arpeggio.rate.name().to_owned(),
                ArpField::Mode => arpeggio.mode.name().to_owned(),
                ArpField::Octaves => arpeggio.octaves.to_string(),
                ArpField::Retrig => if arpeggio.retrig { "on" } else { "off" }.to_owned(),
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

/// How wide a setting's column is on the Mod page, with a space between columns, so columns line
/// up and don't move as values change.
fn width(control: Control) -> u16 {
    match control {
        Control::Lfo(_, LfoField::Rate) => 8,
        Control::Lfo(_, LfoField::Shape) => LFO_SHAPE_NAME + 1 + LFO_ICON_ROOM,
        Control::Base(TrackControl::SendB) | Control::Lfo(_, LfoField::Retrig) => 7,
        // Room for `16 loops`.
        Control::Accumulator(AccumulatorField::Reset) => 8,
        // Room for `Up-down-inc`.
        Control::Arpeggio(ArpField::Mode) => 11,
        Control::Base(TrackControl::LowCut | TrackControl::HighCut) => 8,
        Control::Envelope(
            _,
            EnvelopeField::Attack
            | EnvelopeField::Decay
            | EnvelopeField::Sustain
            | EnvelopeField::Release,
        ) => 8,
        // Room for a slot's setting, like `2 Ping-pong`, or one that's gone, like `Material ---`.
        Control::Lfo(_, LfoField::Destination)
        | Control::Envelope(_, EnvelopeField::Destination) => 12,
        _ => 6,
    }
}

/// The Mod page's columns for some rows: their name, and then each setting's. An LFO's shape takes
/// two, its name and then its icon.
fn mod_columns(controls: &[Control]) -> Columns {
    let mut widths = vec![NAME_WIDTH - 1];
    for control in controls {
        match control {
            Control::Lfo(_, LfoField::Shape) => widths.extend([LFO_SHAPE_NAME, LFO_ICON_ROOM]),
            _ => widths.push(width(*control)),
        }
    }
    Columns::new(widths).spaced(1)
}

/// The column names over some rows, with a jump's labels for those rows on each name.
fn heading<R: AsRef<[Control]>>(app: &App, rows: &[R], columns: &Columns) -> Row<'static> {
    let mut cells = columns.row();
    cells.skip(1);
    for (column, control) in rows[0].as_ref().iter().copied().enumerate() {
        let labels: String = rows
            .iter()
            .filter_map(|row| app.field_jump.as_ref()?.label(row.as_ref()[column]))
            .collect();
        let name = name_line(app, control.label(), &labels, width(control));
        match control {
            Control::Lfo(_, LfoField::Shape) => cells.span(name, 2),
            _ => cells.cell(name),
        };
    }
    cells.done()
}

/// A row of settings, with the cursor on one of them.
fn row(
    app: &App,
    track: &Track,
    name: &str,
    controls: &[Control],
    columns: &Columns,
) -> Row<'static> {
    let mut cells = columns.row();
    cells.cell(name.to_owned());
    let current = app.track_control();
    for control in controls {
        let here = *control == current;
        let text = setting::text(app, here, || value(*control, track, app.song()));
        let style = setting::style(app, here, at_default(*control, track));
        cells.cell(value_spans(app, *control, text, style));
        // An LFO's shape has its icon after its name, where it has one, in the active colour while
        // the cursor is on that LFO.
        if let Control::Lfo(slot, LfoField::Shape) = *control {
            let icon = glyphs::lfo_shape(track.lfos[slot].shape).map(|icon| {
                let style = if lfo_being_edited(app) == Some(slot) {
                    theme::active()
                } else {
                    theme::dim()
                };
                Line::styled(icon.to_string(), style)
            });
            cells.cell(icon.unwrap_or_default());
        }
    }
    cells.done()
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
            | Control::FilterMode
            | Control::Base(
                TrackControl::Cutoff
                    | TrackControl::Resonance
                    | TrackControl::Drive
                    | TrackControl::LowCut
                    | TrackControl::HighCut
            )
    )
}

/// Which FX slot the cursor is on, if any.
pub(crate) fn slot_being_edited(app: &App) -> Option<usize> {
    match app.track_control() {
        Control::Insert(slot) | Control::InsertParam(slot, _) => Some(slot),
        _ => None,
    }
}

/// Which LFO the cursor is on, if any.
pub(crate) fn lfo_being_edited(app: &App) -> Option<usize> {
    match app.track_control() {
        Control::Lfo(slot, _) => Some(slot),
        _ => None,
    }
}

/// Each envelope's name and shape, side by side, with the one being edited picked out. The shapes
/// are images, and they're left blank until they've been sent. Each envelope's is a cell of a grid,
/// its name over its shape.
fn draw_diagrams(frame: &mut Frame, area: Rect, app: &App, track: &Track) {
    let columns = Columns::new([(DIAGRAM_WIDTH + DIAGRAM_GAP) as u16; TRACK_ENVELOPES]);
    let current = envelope_being_edited(app);
    for slot in 0..TRACK_ENVELOPES {
        let style = if current == Some(slot) {
            theme::active()
        } else {
            theme::dim()
        };
        let name = track.envelopes[slot].destination.map_or_else(
            || "---".to_owned(),
            |destination| destination.name_on(track),
        );
        let shape = match app.envelope_images {
            Some(ids) => image::rows(ids[slot], DIAGRAM_WIDTH, DIAGRAM_HEIGHT),
            None => vec![Span::raw(" ".repeat(DIAGRAM_WIDTH)); DIAGRAM_HEIGHT],
        };
        let mut lines = vec![Line::styled(format!("Env {}  {name}", slot + 1), style)];
        lines.extend(shape.into_iter().map(Line::from));
        frame.render_widget(Paragraph::new(lines), columns.area_of(area, slot, 1));
    }
}

/// Whether a slot is switched out as it's heard: while the song plays, a step's lock of its bypass
/// counts, so the slot fades and comes back with the lock, and otherwise it's the slot's own.
fn heard_bypassed(app: &App, track: &Track, slot: usize) -> bool {
    match app.live {
        Some(live) if app.transport == Transport::Playing => live.bypassed[slot],
        _ => track.bypassed[slot],
    }
}

/// Part of a bypassed slot's row, faint but for the cursor.
fn faded(line: Line<'static>, cursor: Style) -> Line<'static> {
    Line::from(
        line.spans
            .into_iter()
            .map(|span| {
                // A drawing's placeholders keep their colour, which says which image they are.
                if span.style == cursor || span.content.starts_with(kitty::PLACEHOLDER) {
                    span
                } else {
                    span.style(theme::faint())
                }
            })
            .collect::<Vec<_>>(),
    )
}

/// The FX page's columns: a slot's number and its effect, then for each column of settings a name,
/// a value and a bar, then the gap before a slot's drawing and the drawing.
fn fx_columns() -> Columns {
    let mut widths = vec![FX_INDENT, FX_EFFECT_WIDTH];
    for name in FX_NAME_WIDTHS {
        widths.extend([name, FX_VALUE_WIDTH, FX_BAR_WIDTH]);
    }
    widths.extend([FX_DRAWING_GAP, EQ_CURVE_WIDTH as u16]);
    Columns::new(widths)
}

/// One row of a slot on the FX page, with its settings in columns, read from the effect's list. The
/// effect is only drawn in its first row.
fn fx_row(
    app: &App,
    track: &Track,
    columns: &Columns,
    controls: &[Control],
    slot: usize,
    first: bool,
) -> Row<'static> {
    let mut cells = columns.row();
    let current = app.track_control();
    for control in controls {
        let selected = *control == current;
        let text = setting::text(app, selected, || value(*control, track, app.song()));
        let style = setting::style(app, selected, false);
        match *control {
            Control::Insert(slot) if first => {
                cells
                    .cell(Line::styled((slot + 1).to_string(), theme::dim()))
                    .cell(value_spans(app, *control, text, style));
            }
            Control::Insert(_) => {
                cells.skip(2);
            }
            Control::InsertParam(slot, param) if control.exists_on(track) => {
                let column = param / 2;
                let labels = jump_label(app, *control);
                let name = name_line(app, control.name_on(track), &labels, FX_NAME_WIDTHS[column]);
                cells.cell(name);
                let insert = track.inserts[slot];
                let value = value_spans(app, *control, text, style);
                if insert.params()[param].choices.is_empty() {
                    let set = insert.value(param);
                    let live = app.live.map_or(set, |live| live.inserts[slot][param]);
                    cells
                        .cell(value)
                        .cell(ValueBar::new(set, FX_BAR_CELLS).playing(live));
                } else {
                    cells.span(value, 2);
                }
            }
            _ => {}
        }
    }
    if heard_bypassed(app, track, slot) {
        let cursor = theme::cursor(app.mode);
        cells.map(|line| faded(line, cursor));
    }
    cells.done()
}

/// One row of a gain reduction meter, which hangs down from the top of its two rows as it turns
/// down more, as the main meter's does. An empty one shows a faint mark, so it's seen to be there.
fn reduction_cell(db: f32, row: usize, color: Color) -> Span<'static> {
    let filled = hanging_eighths(db / REDUCTION_METER_DB, row, SLOT_DRAWING_HEIGHT);
    if filled == 0 && row == 0 {
        return Span::styled("▔", theme::faint());
    }
    hanging_cell(filled, color, theme::BASE, 1)
}

/// What follows a slot's rows on the FX page: a compressor's curve and how far it's turning down,
/// a limiter's meter alone, or an EQ's curve, a line for each row. A bypassed slot's is faint but
/// for its images.
fn slot_drawing(app: &App, track: &Track, slot: usize) -> Vec<Line<'static>> {
    let style = if slot_being_edited(app) == Some(slot) {
        theme::active()
    } else {
        theme::dim()
    };
    let color = style.fg.unwrap_or(theme::TEXT);
    let bypassed = heard_bypassed(app, track, slot);
    let reduction = app
        .live
        .filter(|_| !bypassed)
        .map_or(0.0, |live| live.reduction[slot]);
    let drawing = |width: usize, id: u32| image::rows(id, width, SLOT_DRAWING_HEIGHT).into_iter();
    let rows = 0..SLOT_DRAWING_HEIGHT;
    let lines: Vec<Line<'static>> = match (track.inserts[slot], app.slot_images[slot]) {
        (Insert::Compressor(_), Some(id)) => drawing(DYNAMICS_CURVE_WIDTH, id)
            .zip(rows)
            .map(|(curve, row)| {
                Line::from(vec![
                    curve,
                    Span::raw(" "),
                    reduction_cell(reduction, row, color),
                ])
            })
            .collect(),
        (Insert::Limit(_), _) => rows
            .map(|row| Line::from(reduction_cell(reduction, row, color)))
            .collect(),
        (Insert::Eq(_), Some(id)) => drawing(EQ_CURVE_WIDTH, id).map(Line::from).collect(),
        _ => Vec::new(),
    };
    if !bypassed {
        return lines;
    }
    let cursor = theme::cursor(app.mode);
    lines.into_iter().map(|line| faded(line, cursor)).collect()
}

/// The filter's curve in a box, under its rows, and beside it under the output's column, the
/// waveform of the sample the track plays, if it has one, each in a cell of a grid.
fn draw_curves(frame: &mut Frame, area: Rect, filter: u32, waveform: Option<u32>) {
    let columns = Columns::new([SOUND_COLUMN, WAVEFORM_WIDTH as u16 + 2]);
    let filter = image::boxed(filter, FILTER_CURVE_WIDTH, FILTER_CURVE_HEIGHT);
    frame.render_widget(Paragraph::new(filter), columns.area_of(area, 0, 1));
    if let Some(waveform) = waveform {
        let waveform = image::boxed(waveform, WAVEFORM_WIDTH, WAVEFORM_HEIGHT);
        frame.render_widget(Paragraph::new(waveform), columns.area_of(area, 1, 1));
    }
}

/// Draw the view.
pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    // While the keyboard plays the sound, the page sits in a box of that mode's colour.
    let area = if app.mode == Mode::Keys {
        let block = widgets::window_border()
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
        let name = format!("Track {:02} {}", app.cursor.track + 1, track.name);
        format!("{} ", name.trim_end())
    } else {
        format!("{} ", track.kind.title())
    };
    // The track's machine or kind as a glyph before it, a group's in its colour, and whether it's
    // muted or soloed after.
    let icon = match track.kind {
        TrackKind::Group(_) => group::style(app.song(), app.cursor.track).unwrap_or_default(),
        _ => theme::dim(),
    };
    let mut title = vec![
        Span::styled(format!("{} ", glyphs::track(track)), icon),
        Span::styled(name, bold),
    ];
    // A track in a group carries its mark, in the group's colour, as on the mixer.
    if let Some(mark) = group::mark(app.song(), app.cursor.track) {
        title.extend([mark.patch_style(bold), Span::raw(" ")]);
    }
    if let Some((glyph, style)) = level_glyph(app, app.cursor.track) {
        title.push(Span::styled(format!("{glyph} "), style));
    }
    title.extend(tabs);
    let mut rest = area;
    frame.render_widget(Paragraph::new(Line::from(title)), take_top(&mut rest, 2));
    match app.track_page {
        Page::Sound if track.is_group() => {
            let columns = sound_columns();
            let rows = group_rows(app, track, &columns, usize::from(rest.height));
            columns.stack(frame, &mut rest, rows);
        }
        Page::Sound => {
            let columns = sound_columns();
            let mut rows = Vec::new();
            for controls in app.page_rows() {
                // The filter stands apart from the machine above it.
                if controls.first() == Some(&Control::FilterType) {
                    rows.push(blank_row());
                }
                rows.push(sound_row(app, track, controls, &columns));
            }
            columns.stack(frame, &mut rest, rows);
            if track.is_sound()
                && let Some(filter) = app.filter_image
            {
                let waveform = app
                    .waveform_image
                    .filter(|_| track.machine == vt_model::song::Machine::Sampler);
                let rows = FILTER_CURVE_HEIGHT as u16 + 2;
                draw_curves(frame, take_top(&mut rest, rows), filter, waveform);
            }
        }
        Page::Fx => {
            let columns = fx_columns();
            let mut rows = Vec::new();
            // Where each slot's rows start, for its drawing.
            let mut starts = Vec::new();
            for (slot, slot_rows) in FX_ROWS.chunks(SLOT_ROWS).enumerate() {
                starts.push(rows.len() as u16);
                rows.push(fx_row(app, track, &columns, slot_rows[0], slot, true));
                // An empty slot has nothing for its second row, and the slots stay apart.
                if track.inserts[slot] != Insert::Empty {
                    rows.push(fx_row(app, track, &columns, slot_rows[1], slot, false));
                }
                rows.push(blank_row());
            }
            let area = rest;
            columns.stack(frame, &mut rest, rows);
            // Each slot's drawing is a cell of the same grid, beside its two rows of settings.
            for (slot, start) in starts.into_iter().enumerate() {
                let mut below = area;
                take_top(&mut below, start);
                let cell = take_top(&mut below, SLOT_DRAWING_HEIGHT as u16);
                let cell = columns.area_of(cell, FX_DRAWING_COLUMN, 1);
                frame.render_widget(Paragraph::new(slot_drawing(app, track, slot)), cell);
            }
        }
        Page::Mod => {
            let envelopes = mod_columns(&ENVELOPES[0]);
            let mut rows = vec![heading(app, &ENVELOPES, &envelopes)];
            for (slot, controls) in ENVELOPES.iter().enumerate() {
                let name = format!("Env {}", slot + 1);
                rows.push(row(app, track, &name, controls, &envelopes));
            }
            rows.push(blank_row());
            envelopes.stack(frame, &mut rest, rows);
            let lfos = mod_columns(&LFOS[0]);
            let mut rows = vec![heading(app, &LFOS, &lfos)];
            for (slot, controls) in LFOS.iter().enumerate() {
                rows.push(row(
                    app,
                    track,
                    &format!("LFO {}", slot + 1),
                    controls,
                    &lfos,
                ));
            }
            rows.push(blank_row());
            lfos.stack(frame, &mut rest, rows);
            // A track with no note to move has no accumulator or arpeggio.
            if track.is_sound() {
                for (name, controls) in [("Acc", &ACCUMULATOR[..]), ("Arp", &ARPEGGIO[..])] {
                    let columns = mod_columns(controls);
                    let rows = vec![
                        heading(app, &[controls], &columns),
                        row(app, track, name, controls, &columns),
                        blank_row(),
                    ];
                    columns.stack(frame, &mut rest, rows);
                }
            }
            let rows = DIAGRAM_HEIGHT as u16 + 1;
            draw_diagrams(frame, take_top(&mut rest, rows), app, track);
        }
    }
}
