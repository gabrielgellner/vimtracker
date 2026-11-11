//! MIDI bytes into whole messages. A MIDI port hands over bytes as the system has them, and a long
//! SysEx message can come in pieces, so the link puts them back together here before reading them.

/// The longest SysEx message kept. The Roto's longest is under 300 bytes; anything past this is
/// dropped, so a stuck message can't grow without end.
pub const MAX_SYSEX: usize = 4096;

/// Splits a stream of MIDI bytes into whole messages: SysEx from `F0` to `F7`, and channel
/// messages with their data, running status included. Real-time bytes, such as clock, are dropped,
/// as the Roto's link has no use for them.
#[derive(Debug, Default)]
pub struct Splitter {
    sysex: Option<Vec<u8>>,
    running: Option<u8>,
    message: Vec<u8>,
}

/// How many data bytes follow a status byte.
fn data_bytes(status: u8) -> usize {
    match status & 0xF0 {
        0xC0 | 0xD0 => 1,
        0x80..=0xE0 => 2,
        _ => match status {
            0xF1 | 0xF3 => 1,
            0xF2 => 2,
            _ => 0,
        },
    }
}

impl Splitter {
    pub fn new() -> Self {
        Self::default()
    }

    /// The whole messages these bytes complete, in order.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        let mut done = Vec::new();
        for &byte in bytes {
            if byte >= 0xF8 {
                continue;
            }
            if let Some(sysex) = &mut self.sysex {
                if byte == 0xF7 {
                    let mut message = self.sysex.take().unwrap_or_default();
                    message.push(byte);
                    done.push(message);
                } else if byte >= 0x80 {
                    // A status byte ends a SysEx message that never finished; it's dropped.
                    self.sysex = None;
                    self.start(byte, &mut done);
                } else if sysex.len() < MAX_SYSEX {
                    sysex.push(byte);
                } else {
                    self.sysex = None;
                }
                continue;
            }
            if byte >= 0x80 {
                self.start(byte, &mut done);
                continue;
            }
            if self.message.is_empty() {
                // A data byte under running status starts a message of the last status.
                match self.running {
                    Some(status) => self.message.push(status),
                    None => continue,
                }
            }
            self.message.push(byte);
            let status = self.message[0];
            if self.message.len() == 1 + data_bytes(status) {
                done.push(std::mem::take(&mut self.message));
            }
        }
        done
    }

    fn start(&mut self, status: u8, done: &mut Vec<Vec<u8>>) {
        self.message.clear();
        if status == 0xF0 {
            self.sysex = Some(vec![status]);
            self.running = None;
            return;
        }
        if status < 0xF0 {
            self.running = Some(status);
        } else {
            self.running = None;
        }
        if data_bytes(status) == 0 {
            done.push(vec![status]);
        } else {
            self.message.push(status);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sysex_in_pieces_comes_out_whole() {
        let mut splitter = Splitter::new();
        assert!(splitter.feed(&[0xF0, 0x00, 0x22]).is_empty());
        assert_eq!(
            splitter.feed(&[0x03, 0x02, 0x0A, 0x02, 0xF7]),
            vec![vec![0xF0, 0x00, 0x22, 0x03, 0x02, 0x0A, 0x02, 0xF7]]
        );
    }

    #[test]
    fn running_status_and_real_time_bytes() {
        let mut splitter = Splitter::new();
        assert_eq!(
            splitter.feed(&[0xBF, 12, 1, 0xF8, 44, 2]),
            vec![vec![0xBF, 12, 1], vec![0xBF, 44, 2]]
        );
    }

    #[test]
    fn an_unfinished_sysex_is_dropped_at_the_next_status() {
        let mut splitter = Splitter::new();
        assert_eq!(
            splitter.feed(&[0xF0, 1, 2, 0xBF, 20, 127]),
            vec![vec![0xBF, 20, 127]]
        );
    }
}
