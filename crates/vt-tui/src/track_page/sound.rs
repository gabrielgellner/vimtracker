//! The Sound page: a sound track's is a rack of four devices, its machine, its filter, its output
//! and how it plays its notes, each with a grid of its own (the track pages study,
//! `docs/designs/track-pages.md`, §2.1). A group's, a return's and the master's are still rows of
//! their output's settings, the tables below.

use vt_model::action::TrackControl;
use vt_model::grid::GRID_ROWS;
use vt_model::song::{Machine, Track};

use super::controls::base;
use super::{Control, DEVICE_COLUMNS, DeviceSpec, DeviceWidth};

/// The Sound page's devices on a sound track, top to bottom: its machine, its filter, its output
/// and how it plays. Each is a full-width device, and the four fill the rows inside the
/// keyboard's box, as the FX page's slots do. Other tracks have none, and their page is rows.
pub(super) fn devices(track: &Track) -> Vec<DeviceSpec> {
    if !track.is_sound() {
        return Vec::new();
    }
    vec![
        machine(track.machine),
        device(
            "Filter",
            [Some(Control::FilterType), Some(Control::FilterMode)],
            &FILTER,
        ),
        device("Output", [], &OUTPUT),
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

/// The Sound page of a return, which has no machine or filter.
pub const RETURN_ROWS: [&[Control]; 3] = [
    &[Control::Trim],
    &[Control::Level],
    &[base(TrackControl::Pan)],
];

/// The Sound page of a group, which sends to the returns as well.
pub const GROUP_ROWS: [&[Control]; 5] = [
    &[Control::Trim],
    &[Control::Level],
    &[base(TrackControl::Pan)],
    &[base(TrackControl::SendA)],
    &[base(TrackControl::SendB)],
];

/// The Sound page of the master.
pub const MASTER_ROWS: [&[Control]; 2] = [&[Control::Trim], &[Control::Level]];
