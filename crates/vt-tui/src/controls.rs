//! A track's settings as the program addresses and describes them, with nothing of the screen in
//! it: no page, no cursor and no ratatui.
//!
//! - [`Control`]: the program's address for one setting of one track, with its names.
//! - [`ParamView`], from [`view`]: a setting resolved where it is into what a widget draws and
//!   edits it by, and [`set`], the action that writes a value back. `toggle_on` says whether a
//!   setting that's only off or on is on.
//! - [`key`]: what the Roto's layouts name a control by.
//! - `tuning`: what a sampler track's `Tune` and `Cents` are of and how they read.
//! - [`devices`]: a track's settings as racks of devices, a rack for each [`Page`].
//!
//! The track page, the mixer, the status line, the wheel, automation and the Roto all read these,
//! so they're beside `pages` and `roto` and neither's.

mod control;
pub mod devices;
mod keys;
pub mod param;
pub(crate) mod tuning;
mod view;

pub use control::{AccumulatorField, ArpField, Control, LaneField};
pub(crate) use control::{base, macro_name, no_modes};
pub(crate) use devices::{DEVICE_COLUMNS, DEVICE_ROWS, macros_device, modulator_bypassed};
pub use devices::{DeviceSpec, DeviceWidth, Page};
pub(crate) use keys::STEP_KEYS;
pub use keys::key;
pub use param::ParamView;
pub use view::{set, view};
pub(crate) use view::{share, toggle_on};
