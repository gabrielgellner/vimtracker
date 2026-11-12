//! The Nerd Font symbols the program draws, each named after its Nerd Font name and codepoint.
//! kitty and Ghostty both ship the symbols built in (decision 101), so they need no font set up.
//! Each one means something, as a switch's state does, and none is there for decoration.
//!
//! Every one is a single cell wide, as the terminal counts it, so nothing after it moves. kitty and
//! Ghostty may draw an icon over the space after it as well, which only makes it bigger. The
//! explorer's icons are followed by a no-break space, `ICON_SPACE`, which they don't draw over, so
//! each keeps to its cell and every row's icon is the same size. The switches are the other way
//! round: always followed by a plain space, so they're drawn over two cells (decision 157).

use std::time::Duration;

use vt_model::song::{Machine, SynthVoice, Track, TrackKind};

/// A switch that's on: `nf-fa-toggle_on`, U+F205. It's wide, so `widgets::toggle` follows it with
/// a plain space the terminal draws it over, as it's barely readable squeezed into one cell.
pub const SWITCH_ON: char = '\u{f205}';
/// A switch that's off: `nf-fa-toggle_off`, U+F204, followed by a plain space as `SWITCH_ON` is.
pub const SWITCH_OFF: char = '\u{f204}';

// Transport, on the status line (decision 104). These stand in for the words, rather than
// beside them.

/// Playing: `nf-md-play`, U+F040A.
pub const PLAY: char = '\u{f040a}';
/// Paused: `nf-md-pause`, U+F03E4.
pub const PAUSE: char = '\u{f03e4}';
/// Stopped: `nf-md-stop`, U+F04DB.
pub const STOP: char = '\u{f04db}';
/// Recording: `nf-md-record`, U+F044A.
pub const RECORD: char = '\u{f044a}';
/// Waiting for the launch grid, as a realign or a cued launch does: `nf-md-timer_sand`, U+F051F.
pub const WAITING: char = '\u{f051f}';

// Warnings, on the status line (decision 104).

/// Audio has dropped out: `nf-md-alert`, U+F0026.
pub const WARNING: char = '\u{f0026}';
/// Beside the CPU figure, the system is running the audio thread slowly to save power, so the
/// figure reads higher than the same work at full speed (decision 150): `nf-md-leaf`, U+F032A.
pub const LEAF: char = '\u{f032a}';

// Machines and kinds of track, in a track's heading and the machine picker (decision 104).

/// A sampler: `nf-md-waveform`, U+F147D.
pub const SAMPLER: char = '\u{f147d}';
/// A synth voice: `nf-md-piano`, U+F067D.
pub const SYNTH: char = '\u{f067d}';
/// A drum voice: `nf-fa-drum`, U+EE32.
pub const DRUM: char = '\u{ee32}';
/// A group, where its tracks are summed: `nf-md-call_merge`, U+F00F8.
pub const GROUP: char = '\u{f00f8}';
/// After a group's name, its tracks folded into it (decision 128), pushed together from both
/// sides: `nf-md-arrow_collapse_horizontal`, U+F084C.
pub const FOLDED: char = '\u{f084c}';
/// A return, where sends come back: `nf-md-arrow_u_left_top`, U+F17B3.
pub const RETURN: char = '\u{f17b3}';
/// The master, which is what's heard: `nf-md-speaker`, U+F04C3.
pub const MASTER: char = '\u{f04c3}';
/// A muted track: `nf-md-volume_mute`, U+F075F.
pub const MUTE: char = '\u{f075f}';
/// A soloed track: `nf-md-headphones`, U+F02CB.
pub const SOLO: char = '\u{f02cb}';
/// A fader a hand has taken over from the steps' automation, until `Space e` gives it back
/// (decision 174): `nf-md-gesture_tap`, U+F0741.
pub const TAKEN: char = '\u{f0741}';

/// A machine's icon: a sampler, a drum voice or any other synth voice.
pub fn machine(machine: Machine) -> char {
    match machine {
        Machine::Sampler => SAMPLER,
        Machine::Plaits(
            SynthVoice::Kick | SynthVoice::Snare | SynthVoice::Hat | SynthVoice::Drum,
        ) => DRUM,
        Machine::Plaits(_) => SYNTH,
    }
}

/// A track's icon: its machine's, or what kind of fixed track it is.
pub fn track(track: &Track) -> char {
    match track.kind {
        TrackKind::Sound => machine(track.machine),
        TrackKind::Group(_) => GROUP,
        TrackKind::ReturnA | TrackKind::ReturnB => RETURN,
        TrackKind::Master => MASTER,
    }
}

// Steps (decision 104).

/// A value's `Lock` mark in the phrase grid, which holds it until the next note:
/// `nf-md-lock`, U+F033E.
pub const LOCK: char = '\u{f033e}';
/// A live generator, in the phrase's title before the columns it keeps live, as it makes their
/// values: a wand, `nf-md-auto_fix`, U+F0068 (decision 182).
pub const LIVE: char = '\u{f0068}';

// Spinners, while work goes on in the background (decision 104).

/// The six turns of Fira Code's spinner, which the Nerd Font symbols carry from U+EE06 to U+EE0B
/// with no names of their own.
pub const SPINNER: [char; 6] = [
    '\u{ee06}', '\u{ee07}', '\u{ee08}', '\u{ee09}', '\u{ee0a}', '\u{ee0b}',
];
/// How long the spinner shows each turn.
pub const SPINNER_TURN: Duration = Duration::from_millis(100);
/// How long work goes on before the spinner shows, so quick work doesn't flash it.
pub const SPINNER_AFTER: Duration = Duration::from_millis(300);

/// The spinner's turn some time after the work started.
pub fn spinner(elapsed: Duration) -> char {
    let turn = elapsed.as_millis() / SPINNER_TURN.as_millis();
    SPINNER[(turn % SPINNER.len() as u128) as usize]
}

// Explorer (decision 103).

/// The space after an icon: a no-break space, U+00A0. kitty and Ghostty draw a symbol over a plain
/// space after it, two cells wide, but not over this, so each icon keeps to its own cell, as large
/// as every other, and stays clear of the name beside it.
pub const ICON_SPACE: char = '\u{a0}';
/// A folder, closed: `nf-md-folder`, U+F024B, as snacks' explorer draws one.
pub const FOLDER: char = '\u{f024b}';
/// A folder, open: `nf-md-folder_open`, U+F0770. It takes the place of an arrow.
pub const FOLDER_OPEN: char = '\u{f0770}';
/// A project's folder, which holds a `song.ron`: `nf-md-folder_music`, U+F1359.
pub const PROJECT: char = '\u{f1359}';
/// A kit, as a grid of pads: `nf-md-view_grid`, U+F0570.
pub const KIT: char = '\u{f0570}';
/// One of a kit's sounds, as one pad: `nf-md-square_rounded`, U+F14FB.
pub const KIT_SOUND: char = '\u{f14fb}';
/// One of a kit's sounds with variants or velocity layers, which opens to them, as pads stacked:
/// `nf-md-checkbox_multiple_blank`, U+F0136.
pub const KIT_SOUND_LAYERED: char = '\u{f0136}';
/// A sample: `nf-md-waveform`, U+F147D, as Ableton's browser shows one.
pub const SAMPLE: char = '\u{f147d}';
/// A sample that's a loop, by its folder or the tempo in its name: `nf-md-repeat`, U+F0456.
pub const LOOP: char = '\u{f0456}';
/// A file that isn't there, or a file or folder that couldn't be read: `nf-md-alert`, U+F0026,
/// in the error colour.
pub const MISSING: char = '\u{f0026}';

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spinner_turns_once_each_turn_and_goes_round() {
        assert_eq!(spinner(Duration::ZERO), SPINNER[0]);
        assert_eq!(spinner(SPINNER_TURN - Duration::from_millis(1)), SPINNER[0]);
        assert_eq!(spinner(SPINNER_TURN), SPINNER[1]);
        assert_eq!(spinner(SPINNER_TURN * 6), SPINNER[0]);
        assert_eq!(spinner(SPINNER_TURN * 11), SPINNER[5]);
    }
}
