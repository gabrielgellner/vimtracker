//! `dd` on a setting, on the track page and the mixer: it sets the setting to nothing, its lowest,
//! off, its centre or its first choice, as one undo step, while `x` still puts it back to its
//! default (decision 379).

use vt_model::action::TrackControl;
use vt_model::filter::FilterType;
use vt_model::insert::Insert;
use vt_model::modulation::ModField;
use vt_model::shape::{GenParam, Where};
use vt_tui::app::App;
use vt_tui::controls::Control;
use vt_tui::screen::status::column_name;

use crate::common::*;

/// Put the track page's cursor on a setting of the page on screen.
fn go_to(app: &mut App, control: Control) {
    let track = app.services.track_here().cloned();
    for row in 0..200 {
        for field in 0..16 {
            app.track_page.move_to(track.as_ref(), row, field);
            if app.track_control() == control {
                return;
            }
        }
    }
    panic!("{control:?} isn't on this page");
}

#[test]
fn dd_takes_a_one_sided_setting_to_its_lowest_as_one_undo_step_and_x_still_resets() {
    let (mut app, _rt) = app();
    press(&mut app, " 6");
    go_to(&mut app, Control::Mod(0, ModField::Sustain));
    let sustain = |app: &App| app.song().tracks[0].mods[0].adsr.sustain;
    let before = sustain(&app);
    assert_ne!(before, 0);
    let depth = app.services.history.undo_depth();
    press(&mut app, "dd");
    assert_eq!(sustain(&app), 0);
    assert_eq!(
        app.services.history.undo_depth(),
        depth + 1,
        "one undo step"
    );
    assert!(
        app.services.status.starts_with("Sustain to "),
        "{}",
        app.services.status
    );
    // Again, there's nothing to change, and no undo step.
    press(&mut app, "dd");
    assert_eq!(app.services.history.undo_depth(), depth + 1);
    press(&mut app, "u");
    assert_eq!(sustain(&app), before, "undone");
    // `x` is what it was: back to the default, not to nothing.
    press(&mut app, "ddx");
    assert_eq!(sustain(&app), before);
}

#[test]
fn dd_switches_a_switch_off() {
    let (mut app, _rt) = app();
    press(&mut app, " 5");
    go_to(&mut app, Control::Safe);
    press(&mut app, "=");
    assert!(app.song().tracks[0].safe);
    press(&mut app, "dd");
    assert!(!app.song().tracks[0].safe);
    press(&mut app, "u");
    assert!(app.song().tracks[0].safe);
}

#[test]
fn dd_puts_a_centred_setting_at_its_centre_not_its_end() {
    let (mut app, _rt) = app();
    press(&mut app, " 5");
    go_to(&mut app, Control::Base(TrackControl::Pan));
    press(&mut app, "___");
    let pan = |app: &App| app.song().tracks[0].pan;
    assert!(pan(&app) < 0x80);
    press(&mut app, "dd");
    assert_eq!(pan(&app), 0x80);
    press(&mut app, "u");
    assert!(pan(&app) < 0x80);
}

#[test]
fn dd_puts_a_list_on_its_first_choice() {
    let (mut app, _rt) = app();
    press(&mut app, " 5");
    go_to(&mut app, Control::FilterType);
    press(&mut app, "==");
    assert_ne!(app.song().tracks[0].filter, FilterType::ALL[0]);
    press(&mut app, "dd");
    assert_eq!(app.song().tracks[0].filter, FilterType::ALL[0]);
}

#[test]
fn d_alone_does_nothing_and_esc_drops_it() {
    let (mut app, _rt) = app();
    press(&mut app, " 6");
    go_to(&mut app, Control::Mod(0, ModField::Sustain));
    let before = app.song().tracks[0].mods[0].adsr.sustain;
    let depth = app.services.history.undo_depth();
    press(&mut app, "d");
    assert_eq!(app.services.history.undo_depth(), depth);
    keys(&mut app, "<Esc>d");
    assert_eq!(app.song().tracks[0].mods[0].adsr.sustain, before);
    assert_eq!(app.services.history.undo_depth(), depth);
    keys(&mut app, "<Esc>");
}

#[test]
fn on_the_fx_page_dd_on_a_setting_clears_it_and_on_the_name_takes_the_effect_out() {
    let (mut app, _rt) = on_fx_page();
    press(&mut app, "=");
    let effect = app.song().tracks[0].inserts[0];
    assert_ne!(effect, Insert::Empty);
    go_to(&mut app, Control::InsertParam(0, 0));
    press(&mut app, "+++");
    assert_ne!(app.song().tracks[0].inserts[0].value(0), 0);
    press(&mut app, "dd");
    assert_eq!(app.song().tracks[0].inserts[0].value(0), 0);
    assert_ne!(
        app.song().tracks[0].inserts[0],
        Insert::Empty,
        "still there"
    );
    go_to(&mut app, Control::Insert(0));
    press(&mut app, "dd");
    assert_eq!(app.song().tracks[0].inserts[0], Insert::Empty, "taken out");
}

#[test]
fn on_the_gen_page_dd_on_a_setting_clears_it_and_on_fill_takes_the_generator_away() {
    let (mut app, _rt) = app_with(&["--- -- --- --- ---"; 8]);
    keys(&mut app, "vGSjjjjjj===L");
    keys(&mut app, " 8");
    let generators = |app: &App| app.song().tracks[0].phrases[&0].generators().to_vec();
    assert_eq!(here(&app), Some(GenParam::Fill));
    keys(&mut app, "jj$");
    assert_eq!(here(&app), Some(GenParam::Hits));
    keys(&mut app, "dd");
    let Where::Euclid { hits, .. } = generators(&app)[0].shape.place else {
        panic!("{:?}", generators(&app)[0].shape.place);
    };
    assert!(hits <= 1, "{hits}");
    assert_eq!(generators(&app).len(), 1, "still live");
    assert!(
        app.services.status.starts_with("Hits to "),
        "{}",
        app.services.status
    );
    keys(&mut app, "u");
    assert_eq!(
        generators(&app)[0].shape.place,
        Where::Euclid { hits: 3, rotate: 0 }
    );
    keys(&mut app, "0kk");
    assert_eq!(here(&app), Some(GenParam::Fill));
    keys(&mut app, "dd");
    assert!(generators(&app).is_empty());
}

#[test]
fn on_the_mixer_dd_takes_a_fader_down_and_pan_to_the_middle() {
    let (mut app, _rt) = app();
    press(&mut app, " 4gg");
    while column_name(&app) != "Level" {
        press(&mut app, "j");
    }
    press(&mut app, "dd");
    assert_eq!(app.song().tracks[0].level, 0);
    assert!(
        app.services.status.starts_with("Level to 00"),
        "{}",
        app.services.status
    );
    press(&mut app, "gg");
    while column_name(&app) != "Pan" {
        press(&mut app, "j");
    }
    press(&mut app, "-dd");
    assert_eq!(app.song().tracks[0].pan, 0x80);
}
