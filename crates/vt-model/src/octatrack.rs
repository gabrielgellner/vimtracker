//! The Octatrack's `.ot` file, which it keeps beside a sample with the sample's attributes: its
//! tempo, trim, loop, gain and up to 64 slices, in frames of the file. One beside a file seeds the
//! file's bank entry when it joins the bank (#279), and `:export` writes one (#283), never beside a
//! source. Nothing here reads or writes a file: [`OtFile::read`] takes bytes, and
//! [`OtFile::to_bytes`] gives them.
//!
//! Elektron doesn't publish the format. The layout is the one documented by Kai Drange's
//! OctaChainer (`otwriter.h` and `otwriter.cpp`, GPL 2) and by Mike Robeson's `ot-tools-io` crate
//! (its `samples` module, `SampleSettingsFile`, GPL 3), found by changing one setting at a time on
//! the machine. 832 bytes, every number big-endian:
//!
//! | Offset | Size | Field |
//! |---|---|---|
//! | `0x000` | 16 | `FORM`, four zeros, `DPS1SMPA` |
//! | `0x010` | 7 | Zeros but for `0x015`, the version, 2 |
//! | `0x017` | 4 | Tempo: the BPM × 24 |
//! | `0x01B` | 4 | Trim length in bars, scaled, worked out from the tempo |
//! | `0x01F` | 4 | Loop length in bars, the same |
//! | `0x023` | 4 | Time-stretch: 0 off, 2 normal, 3 beat |
//! | `0x027` | 4 | Loop: 0 off, 1 on, 2 ping-pong |
//! | `0x02B` | 2 | Gain, 0 to 96 for -24 to +24 dB in halves, 48 being 0 dB |
//! | `0x02D` | 1 | Trig quantisation: `0xFF` direct, 0 the pattern's length, 1 up a step count |
//! | `0x02E` | 4 | Trim start |
//! | `0x032` | 4 | Trim end |
//! | `0x036` | 4 | Loop start |
//! | `0x03A` | 64 × 12 | Slices: start, end and loop start, `0xFFFFFFFF` for no loop |
//! | `0x33A` | 4 | How many of the slices are used |
//! | `0x33E` | 2 | Checksum: the sum of the bytes from `0x010` up to it, as 16 bits |

/// How long an `.ot` file is.
pub const OT_SIZE: usize = 832;

/// The most slices an `.ot` holds.
pub const OT_SLICES: usize = 64;

/// What an `.ot` starts with, but for its four bytes of zeros.
const FORM: &[u8; 4] = b"FORM";
const KIND: &[u8; 8] = b"DPS1SMPA";

/// The version byte the machine writes, at `0x015`.
const VERSION: u8 = 2;

/// Where the slices start, and the slice count and checksum after them.
const SLICES_AT: usize = 0x3A;
const COUNT_AT: usize = 0x33A;
const CHECKSUM_AT: usize = 0x33E;

/// A slice's loop start meaning it doesn't loop.
const NO_LOOP: u32 = u32::MAX;

/// One of an `.ot`'s slices, in frames of the file. They can overlap, as each has its own end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OtSlice {
    pub start: u32,
    pub end: u32,
    pub loop_start: Option<u32>,
}

/// How an `.ot`'s sample loops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtLoop {
    Off,
    On,
    PingPong,
}

/// What an `.ot` file says of its sample.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtFile {
    /// The BPM × 24, as the file keeps it.
    pub tempo: u32,
    /// The trim's and the loop's lengths in bars, scaled as the machine keeps them and worked out
    /// from the tempo; kept only to be written back.
    pub trim_bars: u32,
    pub loop_bars: u32,
    /// 0 off, 2 normal, 3 beat.
    pub stretch: u32,
    pub loop_mode: OtLoop,
    /// 0 to 96, for -24 to +24 dB in halves.
    pub gain: u16,
    pub quantize: u8,
    pub trim_start: u32,
    /// One past the last frame that plays.
    pub trim_end: u32,
    pub loop_start: u32,
    /// Up to [`OT_SLICES`], in the machine's order.
    pub slices: Vec<OtSlice>,
}

/// Why some bytes aren't an `.ot` file.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OtError {
    #[error("it's {0} bytes, not the 832 of an .ot file")]
    Size(usize),
    #[error("it doesn't start as an .ot file does")]
    Header,
    #[error("its checksum is wrong")]
    Checksum,
    #[error("it says it has {0} slices, more than an .ot holds")]
    Slices(u32),
    #[error("its loop mode {0} isn't one the Octatrack has")]
    Loop(u32),
}

fn be16(bytes: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([bytes[at], bytes[at + 1]])
}

fn be32(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// The sum of the bytes after the header and before the checksum, kept to 16 bits.
fn checksum(bytes: &[u8]) -> u16 {
    bytes[0x10..CHECKSUM_AT]
        .iter()
        .fold(0_u16, |sum, byte| sum.wrapping_add(u16::from(*byte)))
}

impl OtFile {
    /// A file for a sample this many frames long at this BPM: whole, unlooped and unsliced, at
    /// 0 dB, as the machine writes one for a sample it hasn't changed.
    pub fn new(frames: u32, bpm: u32) -> Self {
        Self {
            tempo: bpm * 24,
            trim_bars: 0,
            loop_bars: 0,
            stretch: 0,
            loop_mode: OtLoop::Off,
            gain: 48,
            quantize: 0xFF,
            trim_start: 0,
            trim_end: frames,
            loop_start: 0,
            slices: Vec::new(),
        }
    }

    /// Read an `.ot` file's bytes.
    pub fn read(bytes: &[u8]) -> Result<Self, OtError> {
        if bytes.len() != OT_SIZE {
            return Err(OtError::Size(bytes.len()));
        }
        if &bytes[0..4] != FORM || &bytes[8..16] != KIND {
            return Err(OtError::Header);
        }
        if checksum(bytes) != be16(bytes, CHECKSUM_AT) {
            return Err(OtError::Checksum);
        }
        let count = be32(bytes, COUNT_AT);
        if count as usize > OT_SLICES {
            return Err(OtError::Slices(count));
        }
        let loop_mode = match be32(bytes, 0x27) {
            0 => OtLoop::Off,
            1 => OtLoop::On,
            2 => OtLoop::PingPong,
            other => return Err(OtError::Loop(other)),
        };
        let slices = (0..count as usize)
            .map(|number| {
                let at = SLICES_AT + number * 12;
                OtSlice {
                    start: be32(bytes, at),
                    end: be32(bytes, at + 4),
                    loop_start: Some(be32(bytes, at + 8)).filter(|start| *start != NO_LOOP),
                }
            })
            .collect();
        Ok(Self {
            tempo: be32(bytes, 0x17),
            trim_bars: be32(bytes, 0x1B),
            loop_bars: be32(bytes, 0x1F),
            stretch: be32(bytes, 0x23),
            loop_mode,
            gain: be16(bytes, 0x2B),
            quantize: bytes[0x2D],
            trim_start: be32(bytes, 0x2E),
            trim_end: be32(bytes, 0x32),
            loop_start: be32(bytes, 0x36),
            slices,
        })
    }

    /// The file's bytes, with its checksum. Slices past the 64th are left out.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0; OT_SIZE];
        bytes[0..4].copy_from_slice(FORM);
        bytes[8..16].copy_from_slice(KIND);
        bytes[0x15] = VERSION;
        let mut put =
            |at: usize, value: u32| bytes[at..at + 4].copy_from_slice(&value.to_be_bytes());
        put(0x17, self.tempo);
        put(0x1B, self.trim_bars);
        put(0x1F, self.loop_bars);
        put(0x23, self.stretch);
        let loop_mode = match self.loop_mode {
            OtLoop::Off => 0,
            OtLoop::On => 1,
            OtLoop::PingPong => 2,
        };
        put(0x27, loop_mode);
        put(0x2E, self.trim_start);
        put(0x32, self.trim_end);
        put(0x36, self.loop_start);
        let slices = &self.slices[..self.slices.len().min(OT_SLICES)];
        for (number, slice) in slices.iter().enumerate() {
            let at = SLICES_AT + number * 12;
            put(at, slice.start);
            put(at + 4, slice.end);
            put(at + 8, slice.loop_start.unwrap_or(NO_LOOP));
        }
        put(COUNT_AT, slices.len() as u32);
        bytes[0x2B..0x2D].copy_from_slice(&self.gain.to_be_bytes());
        bytes[0x2D] = self.quantize;
        let sum = checksum(&bytes);
        bytes[CHECKSUM_AT..CHECKSUM_AT + 2].copy_from_slice(&sum.to_be_bytes());
        bytes
    }

    /// The gain in decibels, from -24 to +24.
    pub fn gain_db(&self) -> f32 {
        (f32::from(self.gain.min(96)) - 48.0) / 2.0
    }

    /// The tempo in thousandths of a BPM, as a song keeps its own, if it's one the machine can
    /// have, 30 to 300 BPM.
    pub fn millibpm(&self) -> Option<u32> {
        (720..=7200)
            .contains(&self.tempo)
            .then(|| self.tempo * 1000 / 24)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sliced() -> OtFile {
        OtFile {
            loop_mode: OtLoop::On,
            gain: 42,
            trim_start: 100,
            trim_end: 47_000,
            loop_start: 24_000,
            slices: vec![
                OtSlice {
                    start: 100,
                    end: 12_000,
                    loop_start: None,
                },
                OtSlice {
                    start: 12_000,
                    end: 47_000,
                    loop_start: Some(30_000),
                },
            ],
            ..OtFile::new(48_000, 120)
        }
    }

    #[test]
    fn an_ot_written_by_us_reads_back_the_same() {
        let file = sliced();
        let bytes = file.to_bytes();
        assert_eq!(bytes.len(), OT_SIZE);
        assert_eq!(&bytes[0..16], b"FORM\0\0\0\0DPS1SMPA");
        assert_eq!(OtFile::read(&bytes), Ok(file));
    }

    #[test]
    fn the_fields_are_where_the_documentation_puts_them() {
        let bytes = sliced().to_bytes();
        assert_eq!(&bytes[0x10..0x17], &[0, 0, 0, 0, 0, 2, 0]);
        assert_eq!(be32(&bytes, 0x17), 120 * 24);
        assert_eq!(be32(&bytes, 0x27), 1);
        assert_eq!(be16(&bytes, 0x2B), 42);
        assert_eq!(bytes[0x2D], 0xFF);
        assert_eq!(be32(&bytes, 0x2E), 100);
        assert_eq!(be32(&bytes, 0x32), 47_000);
        assert_eq!(be32(&bytes, 0x36), 24_000);
        assert_eq!(be32(&bytes, 0x3A + 12), 12_000);
        assert_eq!(
            be32(&bytes, 0x3A + 8),
            u32::MAX,
            "the first slice doesn't loop"
        );
        assert_eq!(be32(&bytes, 0x33A), 2);
        let sum: u32 = bytes[16..830].iter().map(|byte| u32::from(*byte)).sum();
        assert_eq!(u32::from(be16(&bytes, 0x33E)), sum % 65_536);
    }

    #[test]
    fn its_gain_and_tempo() {
        let file = sliced();
        assert_eq!(file.gain_db(), -3.0);
        assert_eq!(file.millibpm(), Some(120_000));
        assert_eq!(OtFile { tempo: 0, ..file }.millibpm(), None);
    }

    #[test]
    fn a_damaged_ot_is_refused() {
        let bytes = sliced().to_bytes();
        assert_eq!(OtFile::read(&bytes[..800]), Err(OtError::Size(800)));
        let mut wrong = bytes.clone();
        wrong[0x40] ^= 1;
        assert_eq!(OtFile::read(&wrong), Err(OtError::Checksum));
        let mut header = bytes.clone();
        header[0] = b'R';
        assert_eq!(OtFile::read(&header), Err(OtError::Header));
    }
}
