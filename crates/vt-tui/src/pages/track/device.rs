//! A page as a rack of devices ([`DeviceSpec`]): where each sits in the rack, and where the cursor
//! is among them. The views draw them with
//! `widgets/device`.

use crate::controls::{DeviceSpec, DeviceWidth};

/// Where the cursor is in a page's rack: in which device, on which of its rows, the title being 0,
/// and in which column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceCursor {
    pub device: usize,
    pub row: usize,
    pub column: usize,
}

/// Where a device sits in its rack: in which band, top to bottom, and on which side, 0 for the
/// left or the whole width and 1 for the right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RackPlace {
    pub band: usize,
    pub side: usize,
}

/// Where each device sits in a rack, in the order they're given, which is the order they're read
/// in: a full-width device has a band of its own, and a half-width one goes beside the one before
/// it if that's a half on the left, and starts a band on the left otherwise.
pub(crate) fn rack_places(devices: &[DeviceSpec]) -> Vec<RackPlace> {
    let mut places: Vec<RackPlace> = Vec::with_capacity(devices.len());
    for (at, device) in devices.iter().enumerate() {
        let beside = at > 0
            && device.width == DeviceWidth::Half
            && devices[at - 1].width == DeviceWidth::Half
            && places[at - 1].side == 0;
        let place = match places.last() {
            Some(last) if beside => RackPlace {
                band: last.band,
                side: 1,
            },
            Some(last) => RackPlace {
                band: last.band + 1,
                side: 0,
            },
            None => RackPlace { band: 0, side: 0 },
        };
        places.push(place);
    }
    places
}

/// The device above or below one in its rack: the one on the same side in the band there, or that
/// band's only one, if there's a band there.
pub(crate) fn device_above_below(
    devices: &[DeviceSpec],
    device: usize,
    below: bool,
) -> Option<usize> {
    let places = rack_places(devices);
    let here = *places.get(device)?;
    let band = if below {
        here.band + 1
    } else {
        here.band.checked_sub(1)?
    };
    let in_band: Vec<usize> = (0..places.len())
        .filter(|at| places[*at].band == band)
        .collect();
    in_band
        .iter()
        .copied()
        .find(|at| places[*at].side == here.side)
        .or_else(|| in_band.first().copied())
}

/// The device beside one in its band, on its right or its left, if there's one there.
pub(crate) fn device_beside(devices: &[DeviceSpec], device: usize, right: bool) -> Option<usize> {
    let places = rack_places(devices);
    let here = *places.get(device)?;
    let side = if right {
        here.side + 1
    } else {
        here.side.checked_sub(1)?
    };
    (0..places.len()).find(|at| places[*at].band == here.band && places[*at].side == side)
}

/// The devices in one's column of the rack, top to bottom: those above it, it, and those below it,
/// each the one above or below the last.
pub(crate) fn rack_column(devices: &[DeviceSpec], device: usize) -> Vec<usize> {
    let mut column = vec![device];
    while let Some(above) = device_above_below(devices, column[column.len() - 1], false) {
        column.push(above);
    }
    column.reverse();
    while let Some(below) = device_above_below(devices, column[column.len() - 1], true) {
        column.push(below);
    }
    column
}
