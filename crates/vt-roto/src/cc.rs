//! The Roto's controls as MIDI CCs, from Appendix B of the SysEx API and the vendor's Ableton
//! script. Everything is on channel 16. A knob is 14 bits, its high seven on CC 12 to 19 and its
//! low seven on CC 44 to 51, sent high first; a switch is seven bits, on CC 20 to 27. The same
//! numbers go back to the Roto to move a knob or light a switch, and MIX mode's meters are CCs to
//! it too. The numbers are the same in MIX and PLUGIN modes, so what a knob means is the mode's
//! business, not this module's.

/// The channel every CC is on, 16, as the status byte counts it from 0.
pub const CHANNEL: u8 = 15;

/// A CC on the Roto's channel.
pub const CONTROL_CHANGE: u8 = 0xB0 | CHANNEL;

/// Where each kind of control starts.
pub const KNOB_HIGH: u8 = 12;
pub const KNOB_LOW: u8 = 44;
pub const SWITCH: u8 = 20;
pub const TRANSPORT: u8 = 28;
pub const TOUCH: u8 = 52;
/// Sent with a knob's number, for its setting back at its default: MIX mode's "level at 0 dB",
/// which the Ableton script takes for any knob's default.
pub const DEFAULT: u8 = 64;
pub const METER: u8 = 65;

/// How many knobs, switches and meters.
pub const KNOBS: u8 = 8;

/// How many transport buttons: eight, then the arrows the Ableton script uses for rewind and fast
/// forward.
pub const TRANSPORT_BUTTONS: u8 = 10;

/// A switch pressed or lit, and let go or dark.
pub const ON: u8 = 0x7F;
pub const OFF: u8 = 0x00;

/// What a CC from the Roto means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControlEvent {
    /// A knob turned to a value from 0 to 16383.
    Knob { index: u8, value: u16 },
    /// A switch, with its 7-bit value: [`ON`] pressed and [`OFF`] let go, or a toggle's state.
    Switch { index: u8, value: u8 },
    /// A transport button, the first eight as the Roto labels them and the last two its arrows.
    Transport { index: u8, value: u8 },
    /// A knob touched or let go.
    Touch { index: u8, touched: bool },
    /// A knob's setting asked back to its default.
    Default { index: u8 },
}

/// Puts each knob's two CCs together. The Roto sends a knob's high seven bits first and then its
/// low seven, so the value is whole once the low arrives; a low with no high before it uses the
/// last high seen, or 0.
#[derive(Debug, Clone, Default)]
pub struct ControlDecoder {
    high: [u8; KNOBS as usize],
}

impl ControlDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// What a CC means, if it's one of the Roto's and complete: a 3-byte channel message.
    pub fn feed(&mut self, message: &[u8]) -> Option<ControlEvent> {
        let [status, number, value] = *message else {
            return None;
        };
        if status != CONTROL_CHANGE {
            return None;
        }
        let value = value & 0x7F;
        match number {
            KNOB_HIGH..=19 => {
                self.high[usize::from(number - KNOB_HIGH)] = value;
                None
            }
            KNOB_LOW..=51 => {
                let index = number - KNOB_LOW;
                let high = self.high[usize::from(index)];
                Some(ControlEvent::Knob {
                    index,
                    value: (u16::from(high) << 7) | u16::from(value),
                })
            }
            SWITCH..=27 => Some(ControlEvent::Switch {
                index: number - SWITCH,
                value,
            }),
            TRANSPORT..=37 => Some(ControlEvent::Transport {
                index: number - TRANSPORT,
                value,
            }),
            TOUCH..=59 => Some(ControlEvent::Touch {
                index: number - TOUCH,
                touched: value >= 0x40,
            }),
            DEFAULT if value < KNOBS => Some(ControlEvent::Default { index: value }),
            _ => None,
        }
    }
}

/// The CCs for a control event, as the Roto sends it, or as the program sends a knob's position
/// and a switch's light back: a knob is two messages, the high seven bits first.
pub fn encode(event: ControlEvent) -> Vec<[u8; 3]> {
    match event {
        ControlEvent::Knob { index, value } => knob_position(index, value).to_vec(),
        ControlEvent::Switch { index, value } => {
            vec![[CONTROL_CHANGE, SWITCH + index % KNOBS, value & 0x7F]]
        }
        ControlEvent::Transport { index, value } => vec![[
            CONTROL_CHANGE,
            TRANSPORT + index % TRANSPORT_BUTTONS,
            value & 0x7F,
        ]],
        ControlEvent::Touch { index, touched } => vec![[
            CONTROL_CHANGE,
            TOUCH + index % KNOBS,
            if touched { ON } else { OFF },
        ]],
        ControlEvent::Default { index } => vec![[CONTROL_CHANGE, DEFAULT, index % KNOBS]],
    }
}

/// Moves a knob to a value from 0 to 16383.
pub fn knob_position(index: u8, value: u16) -> [[u8; 3]; 2] {
    let index = index % KNOBS;
    let value = value.min(crate::bytes::PAIR_MAX);
    [
        [CONTROL_CHANGE, KNOB_HIGH + index, (value >> 7) as u8],
        [CONTROL_CHANGE, KNOB_LOW + index, (value & 0x7F) as u8],
    ]
}

/// Lights a switch or puts it out.
pub fn switch_light(index: u8, lit: bool) -> [u8; 3] {
    [
        CONTROL_CHANGE,
        SWITCH + index % KNOBS,
        if lit { ON } else { OFF },
    ]
}

/// Which side of a track's meter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
}

/// Sets one side of a MIX meter, from 0 to 127. In MIX mode's single track, the track's meter is
/// the first.
pub fn meter(track: u8, side: Side, level: u8) -> [u8; 3] {
    let offset = match side {
        Side::Left => 0,
        Side::Right => 1,
    };
    [
        CONTROL_CHANGE,
        METER + (track % KNOBS) * 2 + offset,
        level & 0x7F,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_knob_is_whole_once_its_low_byte_comes() {
        let mut decoder = ControlDecoder::new();
        let [high, low] = knob_position(3, 10_000);
        assert_eq!(high, [0xBF, 15, (10_000 >> 7) as u8]);
        assert_eq!(decoder.feed(&high), None);
        assert_eq!(
            decoder.feed(&low),
            Some(ControlEvent::Knob {
                index: 3,
                value: 10_000
            })
        );
    }

    #[test]
    fn another_channel_is_not_the_rotos() {
        let mut decoder = ControlDecoder::new();
        assert_eq!(decoder.feed(&[0xB0, SWITCH, ON]), None);
    }

    #[test]
    fn the_meters_take_two_ccs_a_track() {
        assert_eq!(meter(0, Side::Left, 100), [0xBF, 65, 100]);
        assert_eq!(meter(7, Side::Right, 1), [0xBF, 80, 1]);
    }
}
