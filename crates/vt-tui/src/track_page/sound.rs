//! The Sound page: a sound track's is a rack of four devices, its machine, its filter, its output
//! and how it plays its notes, each with a grid of its own (the track pages study,
//! `docs/designs/track-pages.md`, §2.1). A group's, a return's and the master's is the same Output
//! with holes where they lack a setting (§2.3). What feeds a group or a return is drawn under it,
//! but has nothing for the cursor, so it isn't one of the page's devices.

use vt_model::action::TrackControl;
use vt_model::grid::GRID_ROWS;
use vt_model::song::{Machine, Track, TrackKind};

use super::controls::base;
use super::{Control, DEVICE_COLUMNS, DeviceSpec, DeviceWidth};

/// The Sound page's devices, top to bottom: on a sound track its machine, its filter, its output
/// and how it plays, each a full-width device, the four filling the rows inside the keyboard's
/// box, as the FX page's slots do. A group, a return and the master have only their output.
pub(super) fn devices(track: &Track) -> Vec<DeviceSpec> {
    let output: &[&[Control]] = match track.kind {
        TrackKind::Sound => &OUTPUT,
        TrackKind::Group(_) => &GROUP_OUTPUT,
        TrackKind::ReturnA | TrackKind::ReturnB => &RETURN_OUTPUT,
        TrackKind::Master => &MASTER_OUTPUT,
    };
    if !track.is_sound() {
        return vec![device("Output", [], output)];
    }
    vec![
        machine(track.machine),
        device(
            "Filter",
            [Some(Control::FilterType), Some(Control::FilterMode)],
            &FILTER,
        ),
        device("Output", [], output),
        device("Play", [], &PLAY),
    ]
}

/// The filter's settings, in columns: the filter itself, then the utility filter's two cuts.
const FILTER: [&[Control]; 2] = [
    &[
        base(TrackControl::Cutoff),
        base(TrackControl::Resonance),
        base(TrackControl::Drive),
    ],
    &[base(TrackControl::LowCut), base(TrackControl::HighCut)],
];

/// The output's, in the order the signal meets them down each column, as decision 136 has `Trim`
/// before `Level`: its gain, fader and place, then what it sends and the group it goes through.
const OUTPUT: [&[Control]; 2] = [
    &[Control::Trim, Control::Level, base(TrackControl::Pan)],
    &[
        base(TrackControl::SendA),
        base(TrackControl::SendB),
        Control::Group,
    ],
];

/// A group's output: a sound track's, with a hole where `Group` is, as a group can't be in one.
const GROUP_OUTPUT: [&[Control]; 2] = [
    &[Control::Trim, Control::Level, base(TrackControl::Pan)],
    &[base(TrackControl::SendA), base(TrackControl::SendB)],
];

/// A return's: its gain, fader and place, with holes where the sends and `Group` are, as a return
/// goes only to the master.
const RETURN_OUTPUT: [&[Control]; 1] = [&[Control::Trim, Control::Level, base(TrackControl::Pan)]];

/// The master's: its gain and fader, with a hole where `Pan` is, then the song's limiter. Down the
/// columns it reads as the master plays: `Trim`, its slots, `Level`, then the limiter.
const MASTER_OUTPUT: [&[Control]; 2] = [&[Control::Trim, Control::Level], &[Control::Limiter]];

/// How it plays its notes: their pitch and how they join and end, then the steps' timing, then
/// what cuts them off.
const PLAY: [&[Control]; 3] = [
    &[
        base(TrackControl::Pitch),
        Control::Glide,
        Control::GateLength,
    ],
    &[Control::Speed, Control::Groove, Control::Reset],
    &[Control::Choke],
];

/// The machine's device: the machine in its title, and its type after it, the sample a sampler
/// plays or a synth's setting that gives it its character, then its settings in its own grid.
fn machine(machine: Machine) -> DeviceSpec {
    let kind = match machine {
        Machine::Sampler => Some(Control::Sample),
        Machine::Plaits(_) => machine.type_param().map(Control::Setting),
    };
    DeviceSpec::new(
        "Machine",
        [Some(Control::Machine), kind, None, None],
        machine.grid(),
        Control::Setting,
    )
}

/// A device of the track's own settings, with these title controls over its first columns and
/// these columns of settings, each from the top.
fn device<const TITLE: usize>(
    identity: &str,
    title: [Option<Control>; TITLE],
    columns: &[&[Control]],
) -> DeviceSpec {
    let mut grid = [[None; DEVICE_COLUMNS]; GRID_ROWS];
    for (column, controls) in columns.iter().enumerate() {
        for (row, control) in controls.iter().enumerate() {
            grid[row][column] = Some(*control);
        }
    }
    DeviceSpec {
        identity: identity.to_owned(),
        switchable: false,
        width: DeviceWidth::Full,
        title: std::array::from_fn(|at| title.get(at).copied().flatten()),
        grid,
    }
}
