//! What the Roto-Control's files share: ticking the app until the fake Roto, or the app, is as
//! wanted, and what the Roto was told to show.

use std::time::{Duration, Instant};

use vt_roto::fake::{FakePorts, FakeRoto};
use vt_roto::sysex::ToRoto;
use vt_tui::app::App;

/// Ticks the app until the fake Roto has what's wanted, for up to five seconds.
pub fn tick_until(app: &mut App, ports: &FakePorts, done: impl Fn(&FakeRoto) -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        app.tick();
        if ports.with(|fake| done(fake)) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    false
}

/// Ticks the app until it's as wanted, for up to five seconds.
pub fn tick_until_app(app: &mut App, done: impl Fn(&App) -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        app.tick();
        if done(app) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    false
}

/// Ticks the app for a while, for what's sent to settle.
pub fn settle(app: &mut App, for_ms: u64) {
    let until = Instant::now() + Duration::from_millis(for_ms);
    while Instant::now() < until {
        app.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Every value the Roto was told to show on a knob, in order.
pub fn values_shown(fake: &FakeRoto, knob: u8) -> Vec<String> {
    fake.received
        .iter()
        .filter_map(|message| match message {
            ToRoto::ShowControlValue { index, value, .. } if *index == knob => {
                Some(value.as_str().to_owned())
            }
            _ => None,
        })
        .collect()
}
