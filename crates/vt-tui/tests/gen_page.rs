//! The track page's Gen page: a device for each live generator of the phrase under the cursor,
//! its settings turned, typed, listed, reset and rolled through the declared table, each turn
//! rewriting the column as one undo step, and `Space g a`, `dd`, `Enter` and `Empty`'s `K`.

use std::sync::Arc;
use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::Engine;
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::generator::Generator;
use vt_model::samples::SamplePool;
use vt_model::shape::{FillKind, GenParam, Where};
use vt_model::song::{Phrase, Song};
use vt_model::step::Step;
use vt_tui::app::App;
use vt_tui::input::keys::Key;
use vt_tui::input::mode::{Mode, View};
use vt_tui::track_page::{Control, Page};

mod common;
use common::*;

const E: &str = "--- -- --- --- ---";

fn app_with(steps: &[&str]) -> (App, Realtime) {
    let mut song = Song::new(1);
    let steps: Vec<Step> = steps.iter().map(|s| s.parse().unwrap()).collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(48_000, version));
    (App::new(song, None, samples, handle, 48_000), realtime)
}

/// Type keys. `<Esc>` and `<Enter>` are written out.
fn keys(app: &mut App, text: &str) {
    let now = Instant::now();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if let Some(tail) = rest.strip_prefix("<Esc>") {
            app.key(Key::new(KeyCode::Esc), now);
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix("<Enter>") {
            app.key(Key::new(KeyCode::Enter), now);
            rest = tail;
        } else {
            app.key(Key::char(c), now);
            rest = &rest[c.len_utf8()..];
        }
    }
}

fn generators(app: &App) -> Vec<Generator> {
    app.song().tracks[0].phrases[&0].generators().to_vec()
}

fn notes(app: &App) -> Vec<String> {
    app.song().tracks[0].phrases[&0]
        .steps()
        .iter()
        .map(|step| step.to_string()[..3].to_owned())
        .collect()
}

/// Eight empty rows with a tresillo of notes kept live on them, from a ramp, then the Gen page.
fn on_gen_page() -> (App, Realtime) {
    let (mut app, realtime) = app_with(&[E; 8]);
    keys(&mut app, "vGSjjjjjj===L");
    assert_eq!(generators(&app).len(), 1);
    keys(&mut app, " 4LLL");
    assert_eq!(app.track_page.page, Page::Gen);
    // Past the macros' band, first on the page, to the generator's title.
    assert_eq!(app.track_control(), Control::Macro(0));
    keys(&mut app, "}k");
    (app, realtime)
}

/// The setting the cursor is on, on the Gen page.
fn here(app: &App) -> Option<GenParam> {
    match app.track_control() {
        Control::Gen { param, .. } => Some(param),
        _ => None,
    }
}

#[test]
fn the_page_is_fourth_and_shows_the_phrase_under_the_cursor_with_a_device_a_generator() {
    let (app, _rt) = on_gen_page();
    let text = fx_page_text(&app).join("\n");
    assert!(text.contains(" Sound  Mod  FX  Gen "), "{text}");
    assert!(text.contains("Phrase 00"), "{text}");
    assert!(text.contains("Ramp  Euclid · Note 00–07"), "{text}");
    assert!(text.contains("Hits"), "{text}");
    assert!(text.contains("make a column of this phrase live"), "{text}");
    assert_eq!(here(&app), Some(GenParam::Fill), "it starts on the fill");
}

#[test]
fn a_turn_rewrites_the_column_as_one_undo_step() {
    let (mut app, _rt) = on_gen_page();
    assert_eq!(
        notes(&app),
        ["C-3", "---", "---", "F#3", "---", "---", "C-4", "---"]
    );
    keys(&mut app, "jj$");
    assert_eq!(here(&app), Some(GenParam::Hits));
    assert!(
        status_line(&app).contains("3 of the 8 rows"),
        "{}",
        status_line(&app)
    );
    let depth = app.history.undo_depth();
    keys(&mut app, "=");
    let shape = generators(&app)[0].shape;
    assert_eq!(shape.place, Where::Euclid { hits: 4, rotate: 0 });
    assert_eq!(notes(&app).iter().filter(|note| *note != "---").count(), 4);
    assert_eq!(app.history.undo_depth(), depth + 1);
    keys(&mut app, "u");
    assert_eq!(notes(&app).iter().filter(|note| *note != "---").count(), 3);
    // `x` puts it back, and on the rotation, `=` turns the pattern round.
    keys(&mut app, "==x");
    assert_eq!(
        generators(&app)[0].shape.place,
        Where::Euclid { hits: 3, rotate: 0 }
    );
    keys(&mut app, "j=");
    assert_eq!(here(&app), Some(GenParam::Rotate));
    assert_eq!(
        notes(&app),
        ["---", "C-3", "---", "---", "F#3", "---", "---", "C-4"]
    );
}

#[test]
fn k_lists_a_settings_choices_and_i_types_a_value() {
    let (mut app, _rt) = on_gen_page();
    keys(&mut app, "K");
    let names: Vec<String> = app
        .picker
        .as_ref()
        .unwrap()
        .items()
        .iter()
        .map(|item| item.label.clone())
        .collect();
    assert!(names.contains(&"Random".to_owned()), "{names:?}");
    assert!(
        !names.contains(&"Invert".to_owned()),
        "nothing was there to rework"
    );
    keys(&mut app, "alt<Enter>");
    assert_eq!(generators(&app)[0].shape.kind, FillKind::Alternate);
    // `i` on `To` types a note.
    keys(&mut app, "jji");
    assert_eq!(app.mode, Mode::Insert);
    keys(&mut app, "G-3<Enter>");
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(here(&app), Some(GenParam::To));
    assert_eq!(notes(&app)[3], "G-3");
    let text = fx_page_text(&app).join("\n");
    assert!(text.contains("Alternate  Euclid"), "{text}");
}

#[test]
fn r_rolls_a_random_fill_again() {
    let (mut app, _rt) = on_gen_page();
    keys(&mut app, "Krandom<Enter>");
    assert_eq!(generators(&app)[0].shape.kind, FillKind::Random);
    let seed = generators(&app)[0].shape.seed;
    keys(&mut app, "jr");
    assert_ne!(generators(&app)[0].shape.seed, seed);
}

/// `Vary` sits last in a drawing fill's title, `K` lists how long each variation lasts, and
/// `Space g a` keeps the pass last heard as its seed, `Vary` back to `Once`, as one undo step.
#[test]
fn space_g_a_keeps_the_variation_heard_of_a_generator_that_varies() {
    use vt_engine::link::Command;
    let (mut app, mut realtime) = on_gen_page();
    keys(&mut app, "Krandom<Enter>ll");
    assert_eq!(here(&app), Some(GenParam::Vary));
    let text = fx_page_text(&app).join("\n");
    assert!(text.contains("Random  Euclid  Once · Note 00–07"), "{text}");
    keys(&mut app, "K");
    let names: Vec<String> = app
        .picker
        .as_ref()
        .unwrap()
        .items()
        .iter()
        .map(|item| item.label.clone())
        .collect();
    assert_eq!(names[..2], ["Once", "Each pass"]);
    keys(&mut app, "each<Enter>");
    let varying = generators(&app)[0].shape;
    assert_eq!(varying.vary, 1);
    let written = notes(&app);
    // Nothing's been heard, so there's nothing to keep.
    keys(&mut app, " ga");
    assert_eq!(generators(&app)[0].shape, varying);
    assert!(app.status.contains("hasn't been heard"), "{}", app.status);

    // Two and a half times round its eight rows, once the callback has the song as it is.
    for _ in 0..4 {
        block(&mut realtime, &mut app);
    }
    app.handle.send(Command::PlayFrom { step: 0 });
    let pass = 8 * 6000;
    for _ in 0..(pass * 5 / 2) / 256 {
        block(&mut realtime, &mut app);
    }
    let depth = app.history.undo_depth();
    keys(&mut app, " ga");
    let kept = generators(&app)[0].shape;
    assert_eq!(kept.vary, 0, "Vary is Once");
    assert_eq!(kept.seed, varying.on_pass(2).seed, "the third pass's seed");
    assert_ne!(notes(&app), written, "the phrase holds the pass heard");
    assert_eq!(generators(&app).len(), 1, "still live");
    assert_eq!(app.history.undo_depth(), depth + 1);
    assert!(app.status.contains("pass 3"), "{}", app.status);
    keys(&mut app, "u");
    assert_eq!(generators(&app)[0].shape, varying);
}

#[test]
fn space_g_a_applies_it_and_dd_takes_it_away() {
    let (mut app, _rt) = on_gen_page();
    let written = notes(&app);
    keys(&mut app, " ga");
    assert!(generators(&app).is_empty());
    assert_eq!(notes(&app), written, "its values stay, as plain ones");
    assert!(app.status.contains("applied"), "{}", app.status);
    keys(&mut app, "u");
    assert_eq!(generators(&app).len(), 1);
    keys(&mut app, "dd");
    assert!(generators(&app).is_empty());
    assert!(
        notes(&app).iter().all(|note| note == "---"),
        "what was under it is back"
    );
    keys(&mut app, "u");
    assert_eq!(notes(&app), written);
}

#[test]
fn enter_opens_the_shape_panel_on_it_in_the_phrase_view() {
    let (mut app, _rt) = on_gen_page();
    keys(&mut app, "<Enter>");
    assert_eq!(app.view, View::Phrase);
    assert_eq!(app.mode, Mode::Shape);
    assert_eq!(app.shape.as_ref().unwrap().live, Some(0));
}

#[test]
fn empty_makes_another_column_live() {
    let (mut app, _rt) = on_gen_page();
    keys(&mut app, "}");
    assert!(matches!(app.track_control(), Control::NewGenerator(0)));
    keys(&mut app, "K");
    let names: Vec<String> = app
        .picker
        .as_ref()
        .unwrap()
        .items()
        .iter()
        .map(|item| item.label.clone())
        .collect();
    assert!(!names.contains(&"Note".to_owned()), "Note is live already");
    assert!(names.contains(&"Nudge".to_owned()), "{names:?}");
    // Notes of its own own their velocities too, so the nudges are the column to try.
    keys(&mut app, "nudge<Enter>");
    let live = generators(&app);
    assert_eq!(live.len(), 2, "{}", app.status);
    // Its device comes after Note's, and the cursor is on it.
    assert_eq!(
        app.track_page
            .device_cursor(app.track_here())
            .unwrap()
            .device,
        1
    );
    let text = fx_page_text(&app).join("\n");
    assert!(text.contains("Ramp  Notes · Nudge 00–07"), "{text}");
}

#[test]
fn a_phrase_with_none_has_only_the_macros_and_empty() {
    let (mut app, _rt) = app_with(&[E; 8]);
    keys(&mut app, " 4LLL");
    assert_eq!(app.track_control(), Control::Macro(0));
    assert_eq!(app.track_page.devices(app.track_here()).len(), 2);
    keys(&mut app, "}");
    assert!(matches!(app.track_control(), Control::NewGenerator(0)));
    keys(&mut app, "=");
    assert!(generators(&app).is_empty());
}

#[test]
fn each_device_has_a_preview_sent_again_only_as_its_column_changes() {
    use vt_tui::kitty::Graphics;
    let (mut app, _rt) = on_gen_page();
    let mut graphics = Graphics::new();
    let sync = |app: &mut App, graphics: &mut Graphics| {
        let mut out = Vec::new();
        graphics.sync(app, &mut out).unwrap();
        String::from_utf8(out)
            .unwrap()
            .matches(",c=49,r=5,")
            .count()
    };
    assert_eq!(sync(&mut app, &mut graphics), 1);
    let first = app.gen_images[0].unwrap();
    assert_eq!(sync(&mut app, &mut graphics), 0, "nothing has changed");
    // Moving the cursor off it shows it dim, which is sent once, and back costs nothing.
    keys(&mut app, "}");
    assert_eq!(sync(&mut app, &mut graphics), 1);
    keys(&mut app, "{");
    assert_eq!(sync(&mut app, &mut graphics), 0);
    assert_eq!(app.gen_images[0], Some(first));
    // A turn of `Hits` changes the column, so it's drawn again.
    keys(&mut app, "jj$=");
    assert_eq!(sync(&mut app, &mut graphics), 1);
    assert_ne!(app.gen_images[0], Some(first));
    // On screen, its box is beside the grid, and its brief value is the hits alone.
    let cells = screen_cells(&mut app).join("\n");
    assert!(
        cells.contains("▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒"),
        "{cells}"
    );
    assert!(cells.contains("Hits      4 "), "{cells}");
    assert_eq!(app.gen_panels_at.len(), 1);
}

#[test]
fn the_row_playing_rides_the_preview_as_a_dot() {
    use vt_engine::engine::Transport;
    use vt_tui::kitty::Graphics;
    let (mut app, _rt) = on_gen_page();
    let mut graphics = Graphics::new();
    let frame = |app: &mut App, graphics: &mut Graphics| {
        graphics.sync(app, &mut Vec::new()).unwrap();
        screen_cells(app);
        let mut placed = Vec::new();
        graphics.place(app, &mut placed).unwrap();
        String::from_utf8_lossy(&placed).into_owned()
    };
    assert_eq!(frame(&mut app, &mut graphics), "", "stopped, no dot");
    app.transport = Transport::Playing;
    app.jam[0].chain = Some(0);
    app.set_playhead(3);
    let placed = frame(&mut app, &mut graphics);
    assert_eq!(placed.matches("a=p,").count(), 1, "{placed:?}");
    assert_eq!(
        frame(&mut app, &mut graphics),
        "",
        "nothing moved, nothing placed"
    );
    app.set_playhead(4);
    let placed = frame(&mut app, &mut graphics);
    assert_eq!(placed.matches("a=p,").count(), 1, "moved by placing again");
    assert!(!placed.contains("a=t,"), "with no new image");
}
