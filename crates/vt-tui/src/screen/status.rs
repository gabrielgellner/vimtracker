//! The status line: mode, transport, tempo and messages.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vt_engine::engine::Transport;
use vt_model::accumulator::{Accumulation, MOST_DEGREES, Moves, amount, amount_text};
use vt_model::arpeggio::{Arpeggio, arp_text, octaves_text};
use vt_model::condition::Condition;
use vt_model::constants::TICKS_PER_STEP;
use vt_model::lfo::{Rate, note_rates};
use vt_model::macros::{GenColumn, MacroTarget};
use vt_model::selection::Field;
use vt_model::settings::SettingId;
use vt_model::song::Track;
use vt_model::step::{FxCommand, Note};
use vt_model::variation::{self, Varies};

use crate::app::App;
use crate::controls::{Control, view};
use crate::input::keys::Key;
use crate::input::mode::{Mode, View};
use crate::overlays::mapping::held_text;
use crate::pages::Page;
use crate::pages::phrase::draw::generator_cell_text;
use crate::services::Services;
use crate::services::banking::sample_shown;
use crate::services::generators::with_live;
use crate::widgets::glyphs;
use crate::widgets::theme;

/// Show a tempo without trailing zeros.
pub(crate) fn format_bpm(millibpm: u32) -> String {
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

/// The name of what the cursor is on, for the status line, as the page on screen tells it.
pub fn column_name(app: &App) -> String {
    let ctx = app.ctx();
    match app.modes.view {
        View::Song => app.song_view.column(&ctx),
        View::Chain => app.chain_view.column(&ctx),
        View::Phrase => app.phrase_view.column(&ctx),
        View::Track => app.track_page.column(&ctx),
        View::Mixer => app.mixer_page.column(&ctx),
        View::Project => app.project_page.column(&ctx),
        View::Audio => app.audio_page.column(&ctx),
        View::Sample => app.sample_page.column(&ctx),
    }
}

/// What the page on screen has the status line say while nothing else is said there.
fn page_hint(app: &App) -> Option<String> {
    let ctx = app.ctx();
    match app.modes.view {
        View::Song => app.song_view.hint(&ctx),
        View::Chain => app.chain_view.hint(&ctx),
        View::Phrase => app.phrase_view.hint(&ctx),
        View::Track => app.track_page.hint(&ctx),
        View::Mixer => app.mixer_page.hint(&ctx),
        View::Project => app.project_page.hint(&ctx),
        View::Audio => app.audio_page.hint(&ctx),
        View::Sample => app.sample_page.hint(&ctx),
    }
}

/// The value under the cursor in its setting's units, like `1.21 kHz`, where it's a setting the
/// track has: a lock in the grid, or a row of the track page, as its view says it.
pub fn value_shown(app: &App) -> Option<String> {
    let track = app.services.track_here()?;
    if app.modes.view == View::Phrase {
        return heard_shown(app, app.services.cursor.field)
            .or_else(|| phrase_value(&app.services, app.services.cursor.field));
    }
    if !matches!(app.modes.view, View::Track | View::Mixer) {
        return None;
    }
    let control = app.track_control();
    let view = |track: &Track| view(control, track, app.song(), app.services.live.scale.on);
    // A fader the steps drive reads where they have it, and says so, as one a hand has taken over
    // from them does.
    let heard = app.services.track_as_heard(app.services.cursor.track)?;
    // A setting of the registry a macro holds reads where the macro puts it, and says which and
    // how far. `Glide` says what off does on its voice instead, as it's set.
    let heard = heard.with_macros();
    let id = match control.value_on(&heard) {
        Some((id, _)) if control != Control::Glide => id,
        _ => {
            let text = view(track)?.text;
            return match control {
                // As its macros turn it, and which macro holds it.
                Control::Gen {
                    phrase,
                    generator,
                    param,
                } => {
                    let column = with_live(app.song(), track, phrase, generator, |found, _| {
                        found.column
                    })?;
                    let target = MacroTarget::Gen {
                        column: GenColumn::of(column),
                        param,
                    };
                    Some(match held_text(track, target) {
                        Some(held) => format!("{text}: {held}"),
                        None => text,
                    })
                }
                _ => (!text.is_empty()).then_some(text),
            };
        }
    };
    let note = app
        .services
        .fader_note(app.services.cursor.track, control)
        .unwrap_or("");
    let held = match control {
        Control::Macro(at) => format!(": holds {}", crate::overlays::mapping::holds(track, at)),
        _ => held_text(track, MacroTarget::Setting(id))
            .map_or_else(String::new, |held| format!(": {held}")),
    };
    let text = view(&heard)?.text;
    id.exists_on(track).then(|| format!("{text}{note}{held}"))
}

/// On a cell of a live generator a macro or `Vary` moved, what's written and what was last heard
/// there, as `C-2 written; D#2 heard on pass 3`.
fn heard_shown(app: &App, field: Field) -> Option<String> {
    let id = app.services.phrase_id()?;
    let phrase = app.services.phrase_here()?;
    let row = app.services.cursor.step;
    let (at, generator) = phrase
        .generators()
        .iter()
        .enumerate()
        .find(|(_, generator)| {
            crate::services::generators::on_column(field, generator.column)
                && generator
                    .rows()
                    .is_some_and(|rows| (rows.first..=rows.last).contains(&row))
        })?;
    let (heard, pass) = app
        .services
        .heard_cell(app.services.cursor.track, id, at, row)?;
    let written = phrase.steps().get(row)?;
    let shown =
        |step| generator_cell_text(step, &heard, generator.column, app.services.track_here());
    Some(format!(
        "{} written; {} heard on pass {}",
        shown(written),
        shown(&heard),
        pass + 1
    ))
}

/// A field of the row under the cursor in its setting's units, where it has them: the nudge, the
/// note heard where an accumulator or an `RND` moved the row playing, an FX value, or the gate's length. The
/// status line says it for the cursor's field and the row inspector for each of the row's.
pub(crate) fn phrase_value(services: &Services, field: Field) -> Option<String> {
    let track = services.track_here()?;
    let (id, value) = match field {
        Field::Nudge => return Some(nudge_shown(services.current_nudge())),
        // While an accumulator or an `RND` moves the row playing, the Note column says what it's
        // heard as, as the grid shows notes as written.
        Field::Note => {
            services.playhead_step()?;
            let heard = services.heard_note()?;
            return Some(format!("the row playing is heard as {}", Note::On(heard)));
        }
        Field::FxCommand(slot) | Field::FxValue(slot) | Field::FxFine(slot) => {
            let cell = services.current_slot(slot)?;
            if cell.command == Some(FxCommand::Cnd) {
                return Some(condition_shown(cell.coarse()));
            }
            if cell.command == Some(FxCommand::Arp) {
                return Some(arp_shown(
                    cell.coarse(),
                    track.arpeggio,
                    services.live.scale.on,
                ));
            }
            // What an `RND` varies is what's to its left.
            if cell.command == Some(FxCommand::Rnd) {
                let left = slot
                    .checked_sub(1)
                    .map(|left| services.current_slot(left).unwrap_or_default());
                return Some(variation::text(
                    cell.coarse(),
                    Varies::of(left),
                    services.live.scale.on,
                ));
            }
            if cell.command == Some(FxCommand::Acu) {
                return Some(amount_shown(
                    cell.coarse(),
                    track.accumulation,
                    services.live.scale.on,
                ));
            }
            if cell.command == Some(FxCommand::Set(SettingId::SAMPLE)) && track.is_sound() {
                return Some(sample_shown(services.song(), cell.coarse()));
            }
            if cell.command == Some(FxCommand::Set(SettingId::SLICE)) && track.is_sound() {
                return Some(services.slice_shown(cell));
            }
            let id = cell.command?.setting()?;
            (id, cell.value)
        }
        // With none of its own, the length is the track's `Gate`.
        Field::GateLength => {
            let length = services.current_gate().and_then(|gate| gate.length);
            let length = length.unwrap_or_else(|| track.gate_length());
            (SettingId::GATE_LENGTH, u16::from(length) << 8)
        }
        Field::Velocity | Field::Gate => return None,
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

/// How far a nudge moves its row, in 24ths of a sixteenth, which is the same time on every track.
fn nudge_shown(nudge: i8) -> String {
    match nudge {
        0 => "on the grid".to_owned(),
        late if late > 0 => format!("{late}/24 of a sixteenth late"),
        early => format!("{}/24 of a sixteenth early", early.unsigned_abs()),
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
/// red record beside play while recording, and the recorder's sample with its seconds while it
/// records. Fill changes what the conditions play, so it's named
/// while it's on, and a realign, a cued launch, a mute or solo armed for the grid and a loop are
/// named, with the glyph for waiting, until the launch grid comes round.
pub(crate) fn transport_spans(app: &App) -> Vec<Span<'static>> {
    let (glyph, style) = match app.services.reported.transport {
        Transport::Playing => (glyphs::PLAY, theme::active()),
        Transport::Paused => (glyphs::PAUSE, theme::dim()),
        Transport::Stopped => (glyphs::STOP, theme::dim()),
    };
    let mut spans = vec![Span::raw(" "), Span::styled(glyph.to_string(), style)];
    if app.services.reported.transport == Transport::Playing && app.recording() {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            glyphs::RECORD.to_string(),
            Style::new().fg(theme::RED),
        ));
    }
    // The recorder: red with its seconds while it writes, and waiting while it waits to start.
    match app.sampling_shown() {
        Some((number, Some(seconds))) => {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                glyphs::RECORD.to_string(),
                Style::new().fg(theme::RED),
            ));
            spans.push(Span::raw(format!(" {number:02X} {seconds:.1}s")));
        }
        Some((number, None)) => {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(glyphs::WAITING.to_string(), theme::dim()));
            spans.push(Span::raw(format!(" REC {number:02X}")));
        }
        None => {}
    }
    if app.services.live.fill {
        spans.push(Span::raw(" FILL"));
    }
    let realigning =
        app.services.reported.realigning || app.services.reported.realigning_tracks.contains(&true);
    let cued = app
        .services
        .reported
        .jam
        .iter()
        .any(|jam| jam.cued.is_some());
    let muting = app.services.reported.jam.iter().any(|jam| jam.mute_waiting);
    let soloing = app.services.reported.jam.iter().any(|jam| jam.solo_waiting);
    for (waiting, name) in [
        (realigning, "REALIGN"),
        (cued, "LAUNCH"),
        (muting, "MUTE"),
        (soloing, "SOLO"),
        (app.services.reported.loop_waiting, "LOOP"),
    ] {
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
pub(crate) fn right_spans(app: &App) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    if let Some((work, turn)) = app.background_work() {
        spans.push(Span::styled(format!("{turn} {work}  "), theme::dim()));
    }
    let dropouts = app.audio_page.audio.late + app.audio_page.audio.xruns;
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
    // Sections selected with `V` in the song view are Visual's too. The FX page zoomed in on a
    // slot says so, in the page's own colour, but the keyboard's mode is named over it.
    let special = app
        .sample_page
        .badge(app.modes.view, app.modes.mode)
        .or_else(|| app.song_view.anchor.map(|_| Mode::VisualField.label()))
        .or_else(|| {
            (app.track_page.fx_zoomed(app.modes.view) && app.modes.mode != Mode::Keys)
                .then_some("ZOOM")
        });
    let badge = match special {
        Some("SETTINGS") => Some(theme::ROSEWATER),
        Some(mode) if mode == Mode::VisualField.label() => Some(theme::MAUVE),
        _ => None,
    };
    let mode_style = Style::new()
        .fg(theme::BASE)
        .bg(badge.unwrap_or(match app.modes.mode {
            Mode::Normal
            | Mode::OperatorPending
            | Mode::Chain
            | Mode::Track
            | Mode::Mixer
            | Mode::Song
            | Mode::Audio
            | Mode::Project
            | Mode::Sample
            | Mode::SampleVisual => theme::BLUE,
            Mode::VisualField | Mode::VisualStep => theme::MAUVE,
            Mode::Insert => theme::GREEN,
            Mode::Command => theme::PEACH,
            Mode::Help | Mode::Guide | Mode::Picker | Mode::Shape | Mode::Mapping | Mode::Jump => {
                theme::TEAL
            }
            Mode::Replace | Mode::Restore => theme::RED,
            Mode::Keys => theme::YELLOW,
        }))
        .add_modifier(Modifier::BOLD);
    // An overlay's own sequences, the picker's `gg` and `zz`, aren't said here, as they never
    // were.
    let pending = if app.pending().is_empty()
        || matches!(
            app.modes.mode,
            Mode::Picker | Mode::Command | Mode::Jump | Mode::Mapping | Mode::Shape
        ) {
        String::new()
    } else {
        let keys: Vec<String> = app.pending().iter().map(Key::to_string).collect();
        format!("  {}", keys.join(" "))
    };
    let dirty = if app.services.dirty { " *" } else { "" };
    // After the changes' mark, as Vim's status line puts `[RO]` after `[+]`.
    let read_only = if app.services.live.read_only {
        " [RO]"
    } else {
        ""
    };
    let typed = if app.modes.entry.text.is_empty() {
        String::new()
    } else {
        format!("  typed {}", app.modes.entry.text)
    };
    // The cursor's track shows its own groove, marked so it isn't mistaken for the project's.
    let groove = match app
        .song()
        .tracks
        .get(app.services.cursor.track)
        .and_then(|t| t.groove)
    {
        Some(own) => format!("{own} (track)"),
        None => app.song().groove.to_string(),
    };
    let level = app
        .song()
        .tracks
        .get(app.services.cursor.track)
        .map_or(0, |t| t.level);
    // Only shown when on, since it changes what you hear and tracks otherwise run freely.
    let reset = app
        .services
        .section_here()
        .and_then(|s| s.reset)
        .map_or_else(String::new, |steps| format!("Reset {steps}  "));
    let sections = app.song().sections.len();
    let section = if sections > 1 {
        let looping = match app.services.reported.looping {
            Some(sections) if sections.first != sections.last => {
                format!(" loop {}-{}", sections.first + 1, sections.last + 1)
            }
            Some(_) => " loop".to_owned(),
            None => String::new(),
        };
        format!(
            "Sec {}/{sections}{looping}  ",
            app.services.cursor.section + 1
        )
    } else {
        String::new()
    };
    let reset = format!("{section}{reset}");
    // The grid only means something where notes are entered.
    let grid = if app.modes.view == View::Phrase {
        let jump = if app.services.live.auto_jump {
            " Jump"
        } else {
            ""
        };
        format!("  Grid {}{jump}", grid_name(app.song().skip))
    } else {
        String::new()
    };
    let scale = if app.modes.view == View::Phrase && app.services.live.scale.on {
        format!(
            "  Scale {} {}",
            app.services.live.scale.root_name(),
            app.services.live.scale.kind.short_name()
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
                if let Some(mode) = special {
                    mode
                } else if app.modes.mode == Mode::Keys {
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
        "  {} BPM  Groove {groove}  {reset}Oct {} Vel {:02X}{grid}{scale}  TVol {level:02X}{dirty}{read_only}{pending}{typed}",
        format_bpm(app.song().millibpm),
        app.services.live.octave,
        app.services.live.note_velocity,
    )));
    let message = if app.modes.mode == Mode::Command {
        format!(":{}", app.overlays.command.line)
    } else if let Some(jump) = &app.overlays.field_jump {
        format!("/{}", jump.query)
    } else if app.services.status.is_empty()
        && let Some(hint) = page_hint(app)
    {
        hint
    } else if app.services.status.is_empty()
        && let Some(total) = app.adjusted_total()
    {
        // What the changes kept on the selection add up to stays in sight as it moves.
        total
    } else {
        app.services.status.clone()
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
