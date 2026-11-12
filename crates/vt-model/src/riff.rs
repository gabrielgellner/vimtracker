//! The markers other tools leave inside a WAV file, which `hound` and `symphonia` pass over: `cue `
//! points and the `labl` names a `LIST` of type `adtl` gives them, the loops and root note of a
//! `smpl` chunk, and the root note of an `inst` chunk. They seed a file's bank entry when it joins
//! the bank (#279), and `:export` writes them into a new file (#283). Nothing here writes a file:
//! [`with_markers`] gives a copy of a WAV's bytes.
//!
//! The layouts are from Microsoft and IBM's *Multimedia Programming Interface and Data
//! Specifications 1.0* (1991), section 3, "Waveform Audio File Format": the `cue ` chunk (a count,
//! then 24 bytes a point: its id, its position, the chunk it's in, that chunk's start, the block's
//! start and its frame within it), the `LIST` chunk's `adtl` list with its `labl`, `note` and `ltxt`
//! chunks, and the `smpl` chunk (36 bytes, the MIDI unity note fifth of its nine numbers, then 24
//! bytes a loop: its cue id, type, start, end, fraction and play count). The `inst` chunk is from
//! the same family, as libsndfile and the Deluge read it: seven bytes, the unshifted note first.
//! Every number is little-endian, and a chunk of odd length is followed by a byte of padding.
//!
//! The Deluge's `audio_file.cpp` (GPL 3) reads the same chunks, and was read as prior art.

/// One cue point: a frame a tool marked, at the file's own rate, with the name a `labl` gave it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue {
    /// Its id, which a label or a loop names it by.
    pub id: u32,
    pub frame: u64,
    pub label: Option<String>,
}

/// How a `smpl` loop plays, as its type says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopKind {
    Forward,
    PingPong,
    Backward,
    /// A type for a sampler's own use, which the specification leaves to its maker.
    Other(u32),
}

impl LoopKind {
    fn of(code: u32) -> Self {
        match code {
            0 => Self::Forward,
            1 => Self::PingPong,
            2 => Self::Backward,
            other => Self::Other(other),
        }
    }

    fn code(self) -> u32 {
        match self {
            Self::Forward => 0,
            Self::PingPong => 1,
            Self::Backward => 2,
            Self::Other(code) => code,
        }
    }
}

/// One loop of a `smpl` chunk, in frames at the file's own rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SampleLoop {
    pub start: u64,
    /// One past its last frame. The chunk holds the last frame itself, which plays.
    pub end: u64,
    pub kind: LoopKind,
}

/// What a WAV file's marker chunks say.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Markers {
    /// In order of their frames.
    pub cues: Vec<Cue>,
    /// The first is the one a sampler plays, its sustain loop.
    pub loops: Vec<SampleLoop>,
    /// The note it sounds at as recorded, as a MIDI note, `60` being `C-3`: the `smpl` chunk's
    /// unity note, or else the `inst` chunk's.
    pub root: Option<u8>,
}

impl Markers {
    /// Whether they say nothing.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// What reading a WAV's markers found: the markers, how many frames its audio is, if its `fmt `
/// and `data` say, and each chunk skipped as malformed, said in words.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Read {
    pub markers: Markers,
    pub frames: Option<u64>,
    pub problems: Vec<String>,
}

/// One chunk: its id and what's in it, cut short if the file is.
struct Chunk<'a> {
    id: [u8; 4],
    body: &'a [u8],
    /// Whether the file ends before the chunk does.
    short: bool,
}

/// The chunks of a RIFF `WAVE` file in order, or none if it isn't one.
fn chunks(bytes: &[u8]) -> Option<Vec<Chunk<'_>>> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut chunks = Vec::new();
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let id: [u8; 4] = bytes[at..at + 4].try_into().ok()?;
        let size = le32(&bytes[at + 4..at + 8]) as usize;
        let start = at + 8;
        let end = start.saturating_add(size);
        let short = end > bytes.len();
        chunks.push(Chunk {
            id,
            body: &bytes[start..end.min(bytes.len())],
            short,
        });
        if short {
            break;
        }
        at = end + size % 2;
    }
    Some(chunks)
}

fn le16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[0], bytes[1]])
}

fn le32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// A chunk's id as text, as `cue ` or `smpl`.
fn name(id: &[u8; 4]) -> String {
    String::from_utf8_lossy(id).into_owned()
}

/// Read the markers of a WAV file from its bytes. A file that isn't a WAV has none, and a chunk
/// that's malformed or cut short is skipped and said, so the audio still loads.
pub fn read(bytes: &[u8]) -> Read {
    let mut read = Read::default();
    let Some(chunks) = chunks(bytes) else {
        return read;
    };
    let mut block = None;
    let mut cues: Vec<Cue> = Vec::new();
    let mut labels: Vec<(u32, String)> = Vec::new();
    let mut smpl_root = None;
    let mut inst_root = None;
    for chunk in &chunks {
        let skipped = |why: &str| format!("its `{}` chunk {why}, so it's skipped", name(&chunk.id));
        match &chunk.id {
            b"fmt " if chunk.body.len() >= 14 => {
                block = Some(le16(&chunk.body[12..14])).filter(|align| *align > 0);
            }
            b"data" => {
                if let Some(align) = block {
                    read.frames = Some(chunk.body.len() as u64 / u64::from(align));
                }
            }
            b"cue " if chunk.short => read.problems.push(skipped("is cut short")),
            b"cue " => match cue_points(chunk.body) {
                Some(points) => cues = points,
                None => read.problems.push(skipped("is malformed")),
            },
            b"LIST" if chunk.body.starts_with(b"adtl") => {
                if chunk.short {
                    read.problems.push(skipped("is cut short"));
                } else {
                    labels = adtl_labels(&chunk.body[4..]);
                }
            }
            b"smpl" if chunk.short => read.problems.push(skipped("is cut short")),
            b"smpl" => match sampler(chunk.body) {
                Some((root, loops)) => {
                    smpl_root = root;
                    read.markers.loops = loops;
                }
                None => read.problems.push(skipped("is malformed")),
            },
            b"inst" if chunk.body.len() < 7 => read.problems.push(skipped("is cut short")),
            b"inst" => inst_root = Some(chunk.body[0]).filter(|note| *note < 128),
            _ => {}
        }
    }
    for (id, text) in labels {
        if let Some(cue) = cues
            .iter_mut()
            .find(|cue| cue.id == id && cue.label.is_none())
        {
            cue.label = Some(text);
        }
    }
    cues.sort_by_key(|cue| cue.frame);
    read.markers.cues = cues;
    read.markers.root = smpl_root.or(inst_root);
    read
}

/// A `cue ` chunk's points, or none if it says it has more than it holds.
fn cue_points(body: &[u8]) -> Option<Vec<Cue>> {
    let count = le32(body.get(0..4)?) as usize;
    let points = body.get(4..4 + count.checked_mul(24)?)?;
    Some(
        points
            .as_chunks::<24>()
            .0
            .iter()
            .map(|point| {
                let position = le32(&point[4..8]);
                let offset = le32(&point[20..24]);
                // The frame is the offset into its block; a few tools fill in only the position.
                let frame = if offset == 0 { position } else { offset };
                Cue {
                    id: le32(&point[0..4]),
                    frame: u64::from(frame),
                    label: None,
                }
            })
            .collect(),
    )
}

/// The `labl` names in an `adtl` list, by the id of the cue each names. A malformed one ends the
/// list there.
fn adtl_labels(mut list: &[u8]) -> Vec<(u32, String)> {
    let mut labels = Vec::new();
    while list.len() >= 8 {
        let id = &list[0..4];
        let size = le32(&list[4..8]) as usize;
        let Some(body) = list.get(8..8 + size) else {
            break;
        };
        if id == b"labl" && body.len() >= 4 {
            let text = &body[4..];
            let text = text.split(|byte| *byte == 0).next().unwrap_or(text);
            let text = String::from_utf8_lossy(text).trim().to_owned();
            if !text.is_empty() {
                labels.push((le32(&body[0..4]), text));
            }
        }
        list = list.get(8 + size + size % 2..).unwrap_or(&[]);
    }
    labels
}

/// A `smpl` chunk's root note, if it gives one, and its loops, or none if it says it has more
/// loops than it holds. A unity note of 0 is taken as none given, as the Deluge takes it.
fn sampler(body: &[u8]) -> Option<(Option<u8>, Vec<SampleLoop>)> {
    let header = body.get(0..36)?;
    let unity = le32(&header[12..16]);
    let root = u8::try_from(unity)
        .ok()
        .filter(|note| (1..128).contains(note));
    let count = le32(&header[28..32]) as usize;
    let loops = body.get(36..36 + count.checked_mul(24)?)?;
    let loops = loops
        .as_chunks::<24>()
        .0
        .iter()
        .map(|entry| SampleLoop {
            start: u64::from(le32(&entry[8..12])),
            end: u64::from(le32(&entry[12..16])) + 1,
            kind: LoopKind::of(le32(&entry[4..8])),
        })
        .collect();
    Some((root, loops))
}

/// Why markers can't be written into some bytes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WriteError {
    #[error("it isn't a WAV file")]
    NotWav,
    #[error("its `{0}` chunk is cut short")]
    Short(String),
    #[error("a marker is past the 4 GB a WAV can reach")]
    TooFar,
}

/// A WAV file's bytes with these markers in place of any it had: its own `cue `, `smpl`, `inst`
/// and `adtl` list are left out, and new ones follow its other chunks. Cues keep their ids, and
/// each label is a `labl` in an `adtl` list. A `smpl` chunk is written for a root note or a loop.
pub fn with_markers(bytes: &[u8], markers: &Markers) -> Result<Vec<u8>, WriteError> {
    let chunks = chunks(bytes).ok_or(WriteError::NotWav)?;
    let mut out = Vec::with_capacity(bytes.len() + 64 + markers.cues.len() * 40);
    out.extend_from_slice(b"RIFF\0\0\0\0WAVE");
    for chunk in &chunks {
        if chunk.short {
            return Err(WriteError::Short(name(&chunk.id)));
        }
        let ours = matches!(&chunk.id, b"cue " | b"smpl" | b"inst")
            || (&chunk.id == b"LIST" && chunk.body.starts_with(b"adtl"));
        if !ours {
            put_chunk(&mut out, &chunk.id, chunk.body);
        }
    }
    let far = |frame: u64| u32::try_from(frame).map_err(|_| WriteError::TooFar);
    if !markers.cues.is_empty() {
        let mut body = Vec::with_capacity(4 + markers.cues.len() * 24);
        body.extend_from_slice(&(markers.cues.len() as u32).to_le_bytes());
        for cue in &markers.cues {
            let frame = far(cue.frame)?;
            body.extend_from_slice(&cue.id.to_le_bytes());
            body.extend_from_slice(&frame.to_le_bytes());
            body.extend_from_slice(b"data");
            body.extend_from_slice(&0_u32.to_le_bytes());
            body.extend_from_slice(&0_u32.to_le_bytes());
            body.extend_from_slice(&frame.to_le_bytes());
        }
        put_chunk(&mut out, b"cue ", &body);
    }
    let labelled: Vec<(u32, &str)> = markers
        .cues
        .iter()
        .filter_map(|cue| Some((cue.id, cue.label.as_deref()?)))
        .collect();
    if !labelled.is_empty() {
        let mut list = b"adtl".to_vec();
        for (id, text) in labelled {
            let mut label = id.to_le_bytes().to_vec();
            label.extend_from_slice(text.as_bytes());
            label.push(0);
            put_chunk(&mut list, b"labl", &label);
        }
        put_chunk(&mut out, b"LIST", &list);
    }
    if markers.root.is_some() || !markers.loops.is_empty() {
        let mut body = Vec::with_capacity(36 + markers.loops.len() * 24);
        let mut number = |value: u32| body.extend_from_slice(&value.to_le_bytes());
        // Maker, product, period, unity note, its fraction, SMPTE format and offset.
        for value in [0, 0, 0, u32::from(markers.root.unwrap_or(0)), 0, 0, 0] {
            number(value);
        }
        number(markers.loops.len() as u32);
        number(0);
        for (at, each) in markers.loops.iter().enumerate() {
            let last = far(each.end.saturating_sub(1))?;
            for value in [at as u32, each.kind.code(), far(each.start)?, last, 0, 0] {
                body.extend_from_slice(&value.to_le_bytes());
            }
        }
        put_chunk(&mut out, b"smpl", &body);
    }
    let size = u32::try_from(out.len() - 8).map_err(|_| WriteError::TooFar)?;
    out[4..8].copy_from_slice(&size.to_le_bytes());
    Ok(out)
}

/// Write a chunk, with a byte of padding after one of odd length.
fn put_chunk(out: &mut Vec<u8>, id: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(id);
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body);
    if body.len() % 2 == 1 {
        out.push(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 16-bit mono WAV of this many frames, with these chunks after its audio.
    fn wav(frames: u32, extra: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut body = b"WAVE".to_vec();
        let mut format = Vec::new();
        for value in [1_u16, 1] {
            format.extend_from_slice(&value.to_le_bytes());
        }
        format.extend_from_slice(&48_000_u32.to_le_bytes());
        format.extend_from_slice(&96_000_u32.to_le_bytes());
        format.extend_from_slice(&2_u16.to_le_bytes());
        format.extend_from_slice(&16_u16.to_le_bytes());
        put_chunk(&mut body, b"fmt ", &format);
        put_chunk(&mut body, b"data", &vec![0; frames as usize * 2]);
        for (id, chunk) in extra {
            put_chunk(&mut body, id, chunk);
        }
        let mut file = b"RIFF".to_vec();
        file.extend_from_slice(&(body.len() as u32).to_le_bytes());
        file.extend_from_slice(&body);
        file
    }

    #[test]
    fn a_plain_wav_has_no_markers_and_its_length() {
        let read = read(&wav(1000, &[]));
        assert!(read.markers.is_empty());
        assert_eq!(read.frames, Some(1000));
        assert!(read.problems.is_empty());
        assert_eq!(super::read(b"FORM\0\0\0\0AIFF"), Read::default());
    }

    #[test]
    fn markers_written_into_a_wav_read_back_the_same() {
        let markers = Markers {
            cues: vec![
                Cue {
                    id: 1,
                    frame: 0,
                    label: Some("kick".to_owned()),
                },
                Cue {
                    id: 2,
                    frame: 250,
                    label: None,
                },
                Cue {
                    id: 3,
                    frame: 501,
                    label: Some("a".to_owned()),
                },
            ],
            loops: vec![SampleLoop {
                start: 100,
                end: 900,
                kind: LoopKind::PingPong,
            }],
            root: Some(57),
        };
        let file = with_markers(&wav(1000, &[]), &markers).unwrap();
        let read = read(&file);
        assert_eq!(read.markers, markers);
        assert_eq!(read.frames, Some(1000));
        assert!(read.problems.is_empty());
        let again = with_markers(&file, &Markers::default()).unwrap();
        assert_eq!(again, wav(1000, &[]), "old markers are left out");
    }

    #[test]
    fn a_cut_short_or_malformed_chunk_is_skipped_and_said() {
        let mut cue = 3_u32.to_le_bytes().to_vec();
        cue.extend_from_slice(&[0; 24]);
        let read = read(&wav(10, &[(b"cue ", cue)]));
        assert!(read.markers.cues.is_empty());
        assert_eq!(read.problems.len(), 1);
        assert!(read.problems[0].contains("`cue ` chunk is malformed"));
        let mut file = with_markers(
            &wav(10, &[]),
            &Markers {
                root: Some(60),
                ..Markers::default()
            },
        )
        .unwrap();
        file.truncate(file.len() - 10);
        let read = super::read(&file);
        assert_eq!(read.frames, Some(10), "the audio is still there");
        assert_eq!(read.markers.root, None);
        assert!(read.problems[0].contains("`smpl` chunk is cut short"));
    }

    #[test]
    fn the_root_note_is_the_smpl_chunks_or_else_the_insts() {
        let inst = vec![48, 0, 0, 0, 127, 1, 127];
        assert_eq!(
            read(&wav(4, &[(b"inst", inst.clone())])).markers.root,
            Some(48)
        );
        let mut smpl = vec![0; 36];
        smpl[12] = 0;
        let file = wav(4, &[(b"smpl", smpl.clone()), (b"inst", inst.clone())]);
        assert_eq!(
            read(&file).markers.root,
            Some(48),
            "a unity note of 0 is none"
        );
        smpl[12] = 62;
        let file = wav(4, &[(b"smpl", smpl), (b"inst", inst)]);
        assert_eq!(read(&file).markers.root, Some(62));
    }

    #[test]
    fn a_cue_with_only_its_position_is_at_its_position() {
        let mut cue = 1_u32.to_le_bytes().to_vec();
        cue.extend_from_slice(&7_u32.to_le_bytes());
        cue.extend_from_slice(&480_u32.to_le_bytes());
        cue.extend_from_slice(b"data");
        cue.extend_from_slice(&[0; 12]);
        let read = read(&wav(1000, &[(b"cue ", cue)]));
        assert_eq!(
            read.markers.cues,
            vec![Cue {
                id: 7,
                frame: 480,
                label: None
            }]
        );
    }
}
