//! A sample that grows while it's played: frames written one after another into fixed chunks,
//! readable from the first, as the Deluge's recordings are samples from their first frame. The
//! recorder (#288) writes into one on the audio thread while a voice plays it there and the screen
//! reads it here, and lazy loading (#287) can fill one from its decoder the same way.
//!
//! - **Chunks.** The frames are in fixed chunks, each put in place ahead of the writer by a thread
//!   that may allocate ([`Growing::make_room`]), so writing a frame never allocates. A writer that
//!   finds no chunk ready stops, rather than waiting.
//! - **The overlap.** Each chunk has the next chunk's first few frames copied onto its end, the
//!   Deluge's trick, so a read of a frame and the one after it, as interpolation needs, never
//!   looks in two chunks.
//! - **Only what's written.** The count of frames written is published after the frames are, so a
//!   reader on any thread only ever reads frames already there.
//!
//! A frame is kept as one 64-bit atomic, its two sides' bits side by side, so it's read whole
//! whichever thread wrote it. There's one writer at a time.

use std::fmt;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// Frames in each chunk: about two thirds of a second at 48 kHz.
pub const CHUNK_FRAMES: usize = 1 << 15;

/// How many of the next chunk's first frames are copied onto the end of each, so a read of a frame
/// and those after it stays in one chunk: one is all linear interpolation needs, and four leave
/// room for a four-point one.
pub const OVERLAP_FRAMES: usize = 4;

/// One chunk: its frames, then the next chunk's first [`OVERLAP_FRAMES`].
type Chunk = Box<[AtomicU64]>;

/// Frames in fixed chunks, written in order by one writer and read by any thread.
pub struct Growing {
    /// Frames in each chunk, a power of two.
    chunk: usize,
    /// The chunks, in order, each put in place once and never taken away.
    chunks: Box<[OnceLock<Chunk>]>,
    /// How many chunks are in place, which only [`Growing::make_room`] changes.
    ready: AtomicUsize,
    /// How many frames are written: every one before it can be read.
    written: AtomicU64,
}

impl fmt::Debug for Growing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Growing")
            .field("written", &self.written())
            .field("capacity", &self.capacity())
            .finish()
    }
}

/// A frame's two sides as one word.
#[inline]
fn pack([left, right]: [f32; 2]) -> u64 {
    u64::from(left.to_bits()) | (u64::from(right.to_bits()) << 32)
}

#[inline]
fn unpack(word: u64) -> [f32; 2] {
    [
        f32::from_bits(word as u32),
        f32::from_bits((word >> 32) as u32),
    ]
}

impl Growing {
    /// Room for at least so many frames, in chunks of [`CHUNK_FRAMES`], none of them made yet.
    pub fn new(most_frames: u64) -> Self {
        Self::with_chunks(CHUNK_FRAMES, most_frames)
    }

    /// Room for at least so many frames, in chunks of about so many frames: the next power of two.
    pub fn with_chunks(chunk_frames: usize, most_frames: u64) -> Self {
        let chunk = chunk_frames.max(OVERLAP_FRAMES).next_power_of_two();
        let count = most_frames.div_ceil(chunk as u64).max(1) as usize;
        Self {
            chunk,
            chunks: (0..count).map(|_| OnceLock::new()).collect(),
            ready: AtomicUsize::new(0),
            written: AtomicU64::new(0),
        }
    }

    /// Frames in each chunk.
    pub fn chunk_frames(&self) -> usize {
        self.chunk
    }

    /// The most frames it can ever hold.
    pub fn capacity(&self) -> u64 {
        (self.chunks.len() * self.chunk) as u64
    }

    /// How many frames are written, every one of which can be read.
    #[inline]
    pub fn written(&self) -> u64 {
        self.written.load(Ordering::Acquire)
    }

    /// How many frames the chunks in place hold, written or not.
    pub fn room(&self) -> u64 {
        (self.ready.load(Ordering::Acquire) * self.chunk) as u64
    }

    /// Put chunks in place until there's room for at least so many frames past those written, or
    /// it's full, and say how many were made. This allocates, so it's for a thread that may, kept
    /// ahead of the writer. One thread makes room at a time.
    pub fn make_room(&self, ahead: u64) -> usize {
        let wanted = self.written().saturating_add(ahead);
        let mut made = 0;
        let mut ready = self.ready.load(Ordering::Acquire);
        while ready < self.chunks.len() && ((ready * self.chunk) as u64) < wanted {
            let chunk: Chunk = (0..self.chunk + OVERLAP_FRAMES)
                .map(|_| AtomicU64::new(0))
                .collect();
            if self.chunks[ready].set(chunk).is_ok() {
                made += 1;
            }
            ready += 1;
            self.ready.store(ready, Ordering::Release);
        }
        made
    }

    /// Write the next frame, and say whether there was room for it: none once it's full, or if
    /// the chunk it goes in isn't in place yet. Only one writer may call this. It never allocates.
    #[inline]
    pub fn push(&self, frame: [f32; 2]) -> bool {
        let at = self.written.load(Ordering::Relaxed) as usize;
        let (number, offset) = (at / self.chunk, at % self.chunk);
        let Some(chunk) = self.chunks.get(number).and_then(OnceLock::get) else {
            return false;
        };
        let word = pack(frame);
        chunk[offset].store(word, Ordering::Relaxed);
        // The overlap: the chunk before carries this chunk's first frames on its end.
        if offset < OVERLAP_FRAMES
            && number > 0
            && let Some(before) = self.chunks[number - 1].get()
        {
            before[self.chunk + offset].store(word, Ordering::Relaxed);
        }
        self.written.store(at as u64 + 1, Ordering::Release);
        true
    }

    /// A frame, if it's written.
    #[inline]
    pub fn get(&self, index: usize) -> Option<[f32; 2]> {
        self.pair(index).map(|(frame, _)| frame)
    }

    /// A frame and the one after it, for interpolating between them, if the first is written: the
    /// first again for the second if that isn't yet, as at the end of a sample. Both come from one
    /// chunk, however near its end the first is.
    #[inline]
    pub fn pair(&self, index: usize) -> Option<([f32; 2], [f32; 2])> {
        let written = self.written() as usize;
        if index >= written {
            return None;
        }
        let chunk = self.chunks.get(index / self.chunk)?.get()?;
        let offset = index % self.chunk;
        let first = unpack(chunk[offset].load(Ordering::Relaxed));
        let second = if index + 1 < written {
            unpack(chunk[offset + 1].load(Ordering::Relaxed))
        } else {
            first
        };
        Some((first, second))
    }

    /// Every frame written so far, in one buffer. This allocates.
    pub fn to_frames(&self) -> Vec<[f32; 2]> {
        let written = self.written() as usize;
        let mut frames = Vec::with_capacity(written);
        for (number, chunk) in self.chunks.iter().enumerate() {
            let start = number * self.chunk;
            if start >= written {
                break;
            }
            let Some(chunk) = chunk.get() else {
                break;
            };
            let count = (written - start).min(self.chunk);
            frames.extend(
                chunk[..count]
                    .iter()
                    .map(|word| unpack(word.load(Ordering::Relaxed))),
            );
        }
        frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame that says where it is, different on each side.
    fn frame(at: usize) -> [f32; 2] {
        [at as f32, -(at as f32) - 0.5]
    }

    #[test]
    fn a_frame_reads_back_whole() {
        for frame in [[0.25, -1.0], [f32::MIN_POSITIVE, f32::MAX], [-0.0, 0.0]] {
            assert_eq!(unpack(pack(frame)), frame);
        }
    }

    #[test]
    fn only_written_frames_can_be_read() {
        let growing = Growing::with_chunks(8, 64);
        growing.make_room(64);
        assert_eq!(growing.get(0), None);
        for at in 0..10 {
            assert!(growing.push(frame(at)));
        }
        assert_eq!(growing.written(), 10);
        assert_eq!(growing.get(9), Some(frame(9)));
        assert_eq!(growing.get(10), None);
        // The last frame written has no next one yet, so it stands in for it.
        assert_eq!(growing.pair(9), Some((frame(9), frame(9))));
        assert!(growing.push(frame(10)));
        assert_eq!(growing.pair(9), Some((frame(9), frame(10))));
    }

    #[test]
    fn writing_stops_where_no_chunk_is_ready() {
        let growing = Growing::with_chunks(8, 64);
        assert!(!growing.push(frame(0)));
        assert_eq!(growing.make_room(10), 2);
        assert_eq!(growing.room(), 16);
        for at in 0..16 {
            assert!(growing.push(frame(at)));
        }
        assert!(!growing.push(frame(16)));
        assert_eq!(growing.written(), 16);
        // Room is made ahead of what's written, up to what it can hold.
        growing.make_room(1000);
        assert_eq!(growing.room(), growing.capacity());
        assert_eq!(growing.capacity(), 64);
    }

    #[test]
    fn each_chunk_carries_the_next_ones_first_frames() {
        let growing = Growing::with_chunks(8, 64);
        growing.make_room(64);
        for at in 0..40 {
            growing.push(frame(at));
        }
        for at in 0..39 {
            assert_eq!(
                growing.pair(at),
                Some((frame(at), frame(at + 1))),
                "at {at}"
            );
        }
        for (number, chunk) in growing.chunks.iter().take(4).enumerate() {
            let chunk = chunk.get().unwrap();
            for offset in 0..OVERLAP_FRAMES {
                let word = chunk[8 + offset].load(Ordering::Relaxed);
                assert_eq!(unpack(word), frame((number + 1) * 8 + offset));
            }
        }
    }

    #[test]
    fn its_frames_come_out_in_one_buffer() {
        let growing = Growing::with_chunks(8, 64);
        growing.make_room(64);
        for at in 0..21 {
            growing.push(frame(at));
        }
        let frames: Vec<[f32; 2]> = (0..21).map(frame).collect();
        assert_eq!(growing.to_frames(), frames);
    }

    #[test]
    fn a_reader_on_another_thread_only_sees_frames_written() {
        let growing = std::sync::Arc::new(Growing::with_chunks(64, 20_000));
        growing.make_room(20_000);
        let reader = {
            let growing = std::sync::Arc::clone(&growing);
            std::thread::spawn(move || {
                let mut seen = 0;
                while seen < 20_000 {
                    let written = growing.written() as usize;
                    for at in seen..written {
                        assert_eq!(growing.get(at), Some(frame(at)));
                    }
                    seen = written;
                }
            })
        };
        for at in 0..20_000 {
            assert!(growing.push(frame(at)));
        }
        reader.join().unwrap();
    }
}
