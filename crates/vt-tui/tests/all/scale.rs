//! The scale: entering and moving notes through it, turning it off and on, and fitting a phrase
//! to it.

use std::time::Instant;

use std::sync::Arc;

use ratatui::crossterm::event::KeyCode;
use vt_model::action::{Action, edit};
use vt_model::generator::Span;
use vt_model::scale::{Scale, ScaleKind};
use vt_model::selection::StepRange;
use vt_model::shape::{Shape, ShapeColumn};
use vt_model::song::{Phrase, Song};
use vt_model::step::{Note, Step};
use vt_tui::app::App;
use vt_tui::input::keys::Key;
use vt_tui::input::mode::Mode;
use vt_tui::input::mode::View;

use crate::common::*;

#[test]
fn with_a_scale_the_degrees_go_up_from_the_root_and_on_along_the_row_above() {
    let (mut app, _realtime) = app();
    command(&mut app, "set root=d");
    command(&mut app, "set scale=dorian");
    assert!(app.services.live.scale.on, "setting one turns it on");
    entering_steps_down(&mut app);
    tap(&mut app, "iasdfghjqwertyu");
    assert_eq!(
        written_notes(&app),
        [62, 64, 65, 67, 69, 71, 72, 74, 76, 77, 79, 81, 83, 84].map(Note::On)
    );
}

#[test]
fn a_scale_of_other_than_seven_notes_keeps_counting_along_the_rows() {
    let (mut app, _realtime) = app();
    command(&mut app, "set scale=minor-pentatonic");
    entering_steps_down(&mut app);
    tap(&mut app, "iasdfghjqwertyu");
    assert_eq!(
        written_notes(&app),
        [60, 63, 65, 67, 70, 72, 75, 77, 79, 82, 84, 87, 89, 91].map(Note::On),
        "the row above isn't the home row an octave up, but the degrees after it"
    );
}

#[test]
fn with_a_scale_the_other_keys_are_silent_and_note_off_and_octave_still_work() {
    let (mut app, _realtime) = app();
    command(&mut app, "set scale=major");
    entering_steps_down(&mut app);
    let start = app.services.cursor.step;
    press(&mut app, "iip;");
    press(&mut app, ";");
    assert!(written_notes(&app).is_empty());
    assert_eq!(app.services.cursor.step, start, "and it doesn't move");
    press(&mut app, "1");
    assert_eq!(written_notes(&app), [Note::Off]);
    press(&mut app, "xa");
    assert_eq!(written_notes(&app)[1], Note::On(72));
}

#[test]
fn replacing_a_note_folds_too() {
    let (mut app, _realtime) = app();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    command(&mut app, "set root=d");
    press(&mut app, "ggrd");
    assert_eq!(
        written_notes(&app)[0],
        Note::On(66),
        "the third of D major is F#"
    );
}

#[test]
fn option_s_turns_the_scale_off_and_on_and_remembers_it() {
    let (mut app, _realtime) = app();
    entering_steps_down(&mut app);
    command(&mut app, "set root=d");
    command(&mut app, "set scale=dorian");
    press(&mut app, "i");
    alt(&mut app, "M-s");
    assert!(!app.services.live.scale.on);
    assert_eq!(app.modes.mode, Mode::Insert, "in Insert, it stays");
    press(&mut app, "s");
    assert_eq!(written_notes(&app), [Note::On(62)], "the piano again");
    app.key_released(Key::char('s'), Instant::now());
    alt(&mut app, "M-s");
    assert_eq!(app.services.live.scale.root_name(), "D");
    assert_eq!(app.services.live.scale.kind.name(), "Dorian");
    press(&mut app, "s");
    assert_eq!(written_notes(&app)[1], Note::On(64));
    special(&mut app, KeyCode::Esc);
    alt(&mut app, "M-s");
    assert!(!app.services.live.scale.on);
    command(&mut app, "set scale=on");
    assert!(app.services.live.scale.on);
    command(&mut app, "set scale=off");
    assert!(!app.services.live.scale.on);
}

#[test]
fn the_scale_is_not_undone_but_is_saved() {
    let (mut app, _realtime) = app();
    let depth = app.services.history.undo_depth();
    command(&mut app, "set root=d");
    command(&mut app, "set scale=dorian");
    assert_eq!(app.services.history.undo_depth(), depth);
    assert!(app.services.dirty);
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "u");
    assert!(app.services.live.scale.on, "undoing a note leaves it");
    assert_eq!(app.services.live.scale.root_name(), "D");

    let path = folder();
    command(&mut app, &format!("w {}", path.display()));
    let (mut other, _realtime) = self::app();
    command(&mut other, &format!("e {}", path.display()));
    assert_eq!(
        other.services.live.scale, app.services.live.scale,
        "{}",
        other.services.status
    );
}

#[test]
fn moving_a_note_goes_through_the_scale_except_chromatically_or_by_an_octave() {
    let (mut app, _realtime) = app();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "gg");
    command(&mut app, "set scale=major");
    let note = |app: &App| written_notes(app)[0];
    press(&mut app, "=");
    assert_eq!(note(&app), Note::On(62));
    press(&mut app, "2=");
    assert_eq!(note(&app), Note::On(65), "D, E, F");
    press(&mut app, ".");
    assert_eq!(note(&app), Note::On(69), "the count again");
    press(&mut app, "-");
    assert_eq!(note(&app), Note::On(67));
    alt(&mut app, "C-a");
    assert_eq!(note(&app), Note::On(68), "G#, out of key");
    press(&mut app, "=");
    assert_eq!(note(&app), Note::On(69), "and on to the next in");
    alt(&mut app, "C-x");
    press(&mut app, "+");
    assert_eq!(note(&app), Note::On(80), "an octave up, still out of key");
    press(&mut app, "v>");
    assert_eq!(note(&app), Note::On(81), "the operator as well");
}

#[test]
fn with_the_scale_off_notes_move_chromatically() {
    let (mut app, _realtime) = app();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "gg=");
    assert_eq!(written_notes(&app)[0], Note::On(61));
    alt(&mut app, "C-a");
    assert_eq!(written_notes(&app)[0], Note::On(62));
}

#[test]
fn scale_fit_moves_a_phrases_notes_and_can_be_undone() {
    let (mut app, _realtime) = app();
    entering_steps_down(&mut app);
    tap(&mut app, "iawe");
    special(&mut app, KeyCode::Esc);
    let before = written_notes(&app);
    assert_eq!(before, [60, 61, 63].map(Note::On));
    command(&mut app, "scale fit");
    assert_eq!(written_notes(&app), before, "the scale is off");
    assert!(
        app.services.status.contains("off"),
        "{}",
        app.services.status
    );
    command(&mut app, "set scale=major");
    assert_eq!(written_notes(&app), before, "setting a scale moves nothing");
    command(&mut app, "scale fit");
    assert_eq!(written_notes(&app), [60, 60, 62].map(Note::On));
    tap(&mut app, "u");
    assert_eq!(written_notes(&app), before);
}

/// In a section with a scale of its own, notes are entered, moved and shown in it, as the
/// section under the cursor has it, and the song's other sections stay in the song's (decision
/// 426).
#[test]
fn notes_are_entered_in_the_scale_of_the_section_under_the_cursor() {
    let status = |app: &App| {
        let backend = ratatui::backend::TestBackend::new(100, 2);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::screen::status::draw(frame, frame.area(), app))
            .unwrap();
        (0..100)
            .map(|x| terminal.backend().buffer()[(x, 0)].symbol().to_owned())
            .collect::<String>()
    };
    let (mut app, _realtime) = app();
    command(&mut app, "set scale=major");
    // The second section, playing the same chain, in D Dorian.
    press(&mut app, " 1gclll==++ 3");
    assert_eq!(app.services.cursor.section, 1);
    let d_dorian = Scale {
        on: true,
        root: 2,
        kind: ScaleKind::Dorian,
    };
    assert_eq!(app.song().sections[1].scale, Some(d_dorian));
    assert_eq!(app.services.live.scale, C_MAJOR, "the song's is as it was");
    assert!(status(&app).contains("Scale D Dor"), "{}", status(&app));
    entering_steps_down(&mut app);
    tap(&mut app, "iasd");
    special(&mut app, KeyCode::Esc);
    assert_eq!(written_notes(&app), [62, 64, 65].map(Note::On));
    // In the first section, the same phrase takes the song's C major.
    press(&mut app, " 1k 3");
    assert_eq!(app.services.cursor.section, 0);
    assert!(status(&app).contains("Scale C Maj"), "{}", status(&app));
    tap(&mut app, "Gia");
    special(&mut app, KeyCode::Esc);
    assert_eq!(written_notes(&app), [62, 64, 65, 60].map(Note::On));
}

#[test]
fn the_scale_list_sets_the_kind_and_turns_it_on() {
    let (mut app, _realtime) = app();
    command(&mut app, "scale");
    assert_eq!(app.modes.mode, Mode::Picker);
    press(&mut app, "dorian");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.services.live.scale.kind.name(), "Dorian");
    assert!(app.services.live.scale.on);
}

#[test]
fn the_screen_shows_the_scale_and_notes_outside_it() {
    let draw = |app: &App| {
        let backend = ratatui::backend::TestBackend::new(100, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                vt_tui::run::draw_view(frame, frame.area(), app, View::Phrase);
            })
            .unwrap();
        terminal.backend().buffer().clone()
    };
    let status = |app: &App| {
        let backend = ratatui::backend::TestBackend::new(100, 2);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::screen::status::draw(frame, frame.area(), app))
            .unwrap();
        (0..100)
            .map(|x| terminal.backend().buffer()[(x, 0)].symbol().to_owned())
            .collect::<String>()
    };
    let (mut app, _realtime) = app();
    entering_steps_down(&mut app);
    tap(&mut app, "iaw");
    special(&mut app, KeyCode::Esc);
    // The cursor isn't on these.
    tap(&mut app, "G");
    let peach = |app: &App| {
        let buffer = draw(app);
        (0..24)
            .flat_map(|y| (0..100).map(move |x| (x, y)))
            .filter(|at| buffer[*at].fg == vt_tui::widgets::theme::PEACH)
            .count()
    };
    assert_eq!(peach(&app), 0, "off, nothing is marked");
    assert!(!status(&app).contains("Scale"));
    command(&mut app, "set root=c");
    assert!(status(&app).contains("Scale C Maj"), "{}", status(&app));
    assert_eq!(peach(&app), 3, "C# on its three letters, and C isn't");
    alt(&mut app, "M-s");
    assert_eq!(peach(&app), 0);
}

const C_MAJOR: Scale = Scale {
    on: true,
    root: 0,
    kind: ScaleKind::Major,
};
const MAJOR_NOTES: [u8; 8] = [60, 62, 64, 65, 67, 69, 71, 72];
const MINOR_NOTES: [u8; 8] = [60, 62, 63, 65, 67, 68, 70, 72];

/// A song in C major whose first phrase has eight empty rows with a ramp up an octave from `C-3`
/// live on their notes.
fn live_ramp() -> Song {
    let mut song = Song::new(1);
    song.saved_scale = C_MAJOR;
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(vec![Step::default(); 8])));
    let generate = Action::Generate {
        track: 0,
        phrase: 0,
        column: ShapeColumn::Note {
            scale: C_MAJOR,
            writes: Some(0x64),
        },
        span: Span::rows(StepRange::between(0, 7)),
        shape: Shape {
            from: 60,
            to: 72,
            seed: 7,
            ..Shape::default()
        },
        list: Box::default(),
    };
    Arc::unwrap_or_clone(edit(&Arc::new(song), &generate).expect("the edit applies"))
}

/// The notes of a song's first phrase.
fn notes_of(song: &Song) -> Vec<Note> {
    song.tracks[0].phrases[&0]
        .steps()
        .iter()
        .map(|step| step.note)
        .collect()
}

#[test]
fn a_live_generators_notes_follow_a_scale_changed_by_command() {
    let (mut app, mut realtime) = app_for(live_ramp());
    assert_eq!(written_notes(&app), MAJOR_NOTES.map(Note::On));
    let depth = app.services.history.undo_depth();
    command(&mut app, "set scale=minor");
    // What the phrase holds, which the phrase view draws, is in the new scale, with no undo step.
    assert_eq!(written_notes(&app), MINOR_NOTES.map(Note::On));
    assert_eq!(app.services.history.undo_depth(), depth);
    let generator = &app.song().tracks[0].phrases[&0].generators()[0];
    assert_eq!(generator.scale(), Some(app.services.live.scale), "live");
    // And it's what the engine plays: the version it has holds the same rows.
    block(&mut realtime, &mut app);
    let playing = &realtime.engine().current().song;
    assert_eq!(notes_of(playing), MINOR_NOTES.map(Note::On));
    // What's saved has the scale and the notes that go with it.
    let project = folder();
    command(&mut app, &format!("w {}", project.display()));
    let saved = vt_model::project::load_song(&project).unwrap();
    let _ = std::fs::remove_dir_all(&project);
    assert_eq!(saved.saved_scale, app.services.live.scale);
    assert_eq!(notes_of(&saved), MINOR_NOTES.map(Note::On));
}

#[test]
fn undo_and_redo_after_a_scale_change_keep_the_new_scales_notes() {
    let (mut app, mut realtime) = app_for(live_ramp());
    // An edit beside the generator, to have something to undo: a longer phrase.
    command(&mut app, "len 16");
    let depth = app.services.history.undo_depth();
    command(&mut app, "set scale=minor");
    tap(&mut app, "u");
    assert_eq!(
        app.services.history.undo_depth(),
        depth - 1,
        "the edit, undone"
    );
    assert_eq!(written_notes(&app)[..8], MINOR_NOTES.map(Note::On));
    // A few blocks, as the queue to the engine holds one version at a time.
    for _ in 0..4 {
        block(&mut realtime, &mut app);
    }
    let playing = &realtime.engine().current().song;
    assert_eq!(notes_of(playing)[..8], MINOR_NOTES.map(Note::On));
    alt(&mut app, "C-r");
    assert_eq!(written_notes(&app)[..8], MINOR_NOTES.map(Note::On));
    // Back in major, each version met again follows.
    command(&mut app, "set scale=major");
    tap(&mut app, "u");
    assert_eq!(written_notes(&app)[..8], MAJOR_NOTES.map(Note::On));
}

#[test]
fn a_generator_applied_keeps_its_notes_through_a_scale_change() {
    let applied = Action::ApplyGenerator {
        track: 0,
        phrase: 0,
        generator: 0,
    };
    let song = Arc::unwrap_or_clone(edit(&Arc::new(live_ramp()), &applied).unwrap());
    let (mut app, _realtime) = app_for(song);
    let before = Arc::clone(app.song());
    command(&mut app, "set scale=minor");
    assert_eq!(written_notes(&app), MAJOR_NOTES.map(Note::On));
    assert!(Arc::ptr_eq(&before, app.song()), "no version was made");
}

#[test]
fn a_song_whose_generators_were_left_in_another_scale_takes_its_own_as_it_opens() {
    let mut song = live_ramp();
    song.saved_scale = Scale {
        kind: ScaleKind::Minor,
        ..C_MAJOR
    };
    let (mut app, mut realtime) = app_for(song);
    assert_eq!(written_notes(&app), MINOR_NOTES.map(Note::On));
    block(&mut realtime, &mut app);
    let playing = &realtime.engine().current().song;
    assert_eq!(notes_of(playing), MINOR_NOTES.map(Note::On));
}
