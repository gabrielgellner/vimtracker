//! The graphics protocol itself, as kitty and Ghostty have it: the placeholder characters that say
//! where an image goes, the commands that send, place and take off images, and the question at
//! start of whether the terminal draws them.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::sync::mpsc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ratatui::crossterm::terminal;
use ratatui::style::Color;

use super::encoding::png;

/// The character that stands for a piece of an image.
pub(crate) const PLACEHOLDER: char = '\u{10EEEE}';
/// The most characters an image can cover across or down: one for each mark that numbers a row or a
/// column.
pub(crate) const MOST_CELLS: usize = 297;
/// The marks that give a placeholder's row and column, all of kitty's `rowcolumn-diacritics.txt`,
/// which Ghostty's `graphics_unicode.zig` has in the same order.
pub(crate) const POSITIONS: [char; MOST_CELLS] = [
    '\u{305}',
    '\u{30D}',
    '\u{30E}',
    '\u{310}',
    '\u{312}',
    '\u{33D}',
    '\u{33E}',
    '\u{33F}',
    '\u{346}',
    '\u{34A}',
    '\u{34B}',
    '\u{34C}',
    '\u{350}',
    '\u{351}',
    '\u{352}',
    '\u{357}',
    '\u{35B}',
    '\u{363}',
    '\u{364}',
    '\u{365}',
    '\u{366}',
    '\u{367}',
    '\u{368}',
    '\u{369}',
    '\u{36A}',
    '\u{36B}',
    '\u{36C}',
    '\u{36D}',
    '\u{36E}',
    '\u{36F}',
    '\u{483}',
    '\u{484}',
    '\u{485}',
    '\u{486}',
    '\u{487}',
    '\u{592}',
    '\u{593}',
    '\u{594}',
    '\u{595}',
    '\u{597}',
    '\u{598}',
    '\u{599}',
    '\u{59C}',
    '\u{59D}',
    '\u{59E}',
    '\u{59F}',
    '\u{5A0}',
    '\u{5A1}',
    '\u{5A8}',
    '\u{5A9}',
    '\u{5AB}',
    '\u{5AC}',
    '\u{5AF}',
    '\u{5C4}',
    '\u{610}',
    '\u{611}',
    '\u{612}',
    '\u{613}',
    '\u{614}',
    '\u{615}',
    '\u{616}',
    '\u{617}',
    '\u{657}',
    '\u{658}',
    '\u{659}',
    '\u{65A}',
    '\u{65B}',
    '\u{65D}',
    '\u{65E}',
    '\u{6D6}',
    '\u{6D7}',
    '\u{6D8}',
    '\u{6D9}',
    '\u{6DA}',
    '\u{6DB}',
    '\u{6DC}',
    '\u{6DF}',
    '\u{6E0}',
    '\u{6E1}',
    '\u{6E2}',
    '\u{6E4}',
    '\u{6E7}',
    '\u{6E8}',
    '\u{6EB}',
    '\u{6EC}',
    '\u{730}',
    '\u{732}',
    '\u{733}',
    '\u{735}',
    '\u{736}',
    '\u{73A}',
    '\u{73D}',
    '\u{73F}',
    '\u{740}',
    '\u{741}',
    '\u{743}',
    '\u{745}',
    '\u{747}',
    '\u{749}',
    '\u{74A}',
    '\u{7EB}',
    '\u{7EC}',
    '\u{7ED}',
    '\u{7EE}',
    '\u{7EF}',
    '\u{7F0}',
    '\u{7F1}',
    '\u{7F3}',
    '\u{816}',
    '\u{817}',
    '\u{818}',
    '\u{819}',
    '\u{81B}',
    '\u{81C}',
    '\u{81D}',
    '\u{81E}',
    '\u{81F}',
    '\u{820}',
    '\u{821}',
    '\u{822}',
    '\u{823}',
    '\u{825}',
    '\u{826}',
    '\u{827}',
    '\u{829}',
    '\u{82A}',
    '\u{82B}',
    '\u{82C}',
    '\u{82D}',
    '\u{951}',
    '\u{953}',
    '\u{954}',
    '\u{F82}',
    '\u{F83}',
    '\u{F86}',
    '\u{F87}',
    '\u{135D}',
    '\u{135E}',
    '\u{135F}',
    '\u{17DD}',
    '\u{193A}',
    '\u{1A17}',
    '\u{1A75}',
    '\u{1A76}',
    '\u{1A77}',
    '\u{1A78}',
    '\u{1A79}',
    '\u{1A7A}',
    '\u{1A7B}',
    '\u{1A7C}',
    '\u{1B6B}',
    '\u{1B6D}',
    '\u{1B6E}',
    '\u{1B6F}',
    '\u{1B70}',
    '\u{1B71}',
    '\u{1B72}',
    '\u{1B73}',
    '\u{1CD0}',
    '\u{1CD1}',
    '\u{1CD2}',
    '\u{1CDA}',
    '\u{1CDB}',
    '\u{1CE0}',
    '\u{1DC0}',
    '\u{1DC1}',
    '\u{1DC3}',
    '\u{1DC4}',
    '\u{1DC5}',
    '\u{1DC6}',
    '\u{1DC7}',
    '\u{1DC8}',
    '\u{1DC9}',
    '\u{1DCB}',
    '\u{1DCC}',
    '\u{1DD1}',
    '\u{1DD2}',
    '\u{1DD3}',
    '\u{1DD4}',
    '\u{1DD5}',
    '\u{1DD6}',
    '\u{1DD7}',
    '\u{1DD8}',
    '\u{1DD9}',
    '\u{1DDA}',
    '\u{1DDB}',
    '\u{1DDC}',
    '\u{1DDD}',
    '\u{1DDE}',
    '\u{1DDF}',
    '\u{1DE0}',
    '\u{1DE1}',
    '\u{1DE2}',
    '\u{1DE3}',
    '\u{1DE4}',
    '\u{1DE5}',
    '\u{1DE6}',
    '\u{1DFE}',
    '\u{20D0}',
    '\u{20D1}',
    '\u{20D4}',
    '\u{20D5}',
    '\u{20D6}',
    '\u{20D7}',
    '\u{20DB}',
    '\u{20DC}',
    '\u{20E1}',
    '\u{20E7}',
    '\u{20E9}',
    '\u{20F0}',
    '\u{2CEF}',
    '\u{2CF0}',
    '\u{2CF1}',
    '\u{2DE0}',
    '\u{2DE1}',
    '\u{2DE2}',
    '\u{2DE3}',
    '\u{2DE4}',
    '\u{2DE5}',
    '\u{2DE6}',
    '\u{2DE7}',
    '\u{2DE8}',
    '\u{2DE9}',
    '\u{2DEA}',
    '\u{2DEB}',
    '\u{2DEC}',
    '\u{2DED}',
    '\u{2DEE}',
    '\u{2DEF}',
    '\u{2DF0}',
    '\u{2DF1}',
    '\u{2DF2}',
    '\u{2DF3}',
    '\u{2DF4}',
    '\u{2DF5}',
    '\u{2DF6}',
    '\u{2DF7}',
    '\u{2DF8}',
    '\u{2DF9}',
    '\u{2DFA}',
    '\u{2DFB}',
    '\u{2DFC}',
    '\u{2DFD}',
    '\u{2DFE}',
    '\u{2DFF}',
    '\u{A66F}',
    '\u{A67C}',
    '\u{A67D}',
    '\u{A6F0}',
    '\u{A6F1}',
    '\u{A8E0}',
    '\u{A8E1}',
    '\u{A8E2}',
    '\u{A8E3}',
    '\u{A8E4}',
    '\u{A8E5}',
    '\u{A8E6}',
    '\u{A8E7}',
    '\u{A8E8}',
    '\u{A8E9}',
    '\u{A8EA}',
    '\u{A8EB}',
    '\u{A8EC}',
    '\u{A8ED}',
    '\u{A8EE}',
    '\u{A8EF}',
    '\u{A8F0}',
    '\u{A8F1}',
    '\u{AAB0}',
    '\u{AAB2}',
    '\u{AAB3}',
    '\u{AAB7}',
    '\u{AAB8}',
    '\u{AABE}',
    '\u{AABF}',
    '\u{AAC1}',
    '\u{FE20}',
    '\u{FE21}',
    '\u{FE22}',
    '\u{FE23}',
    '\u{FE24}',
    '\u{FE25}',
    '\u{FE26}',
    '\u{10A0F}',
    '\u{10A38}',
    '\u{1D185}',
    '\u{1D186}',
    '\u{1D187}',
    '\u{1D188}',
    '\u{1D189}',
    '\u{1D1AA}',
    '\u{1D1AB}',
    '\u{1D1AC}',
    '\u{1D1AD}',
    '\u{1D242}',
    '\u{1D243}',
    '\u{1D244}',
];

/// The most base64 the protocol takes in one command.
pub(super) const CHUNK: usize = 4096;

/// Lines of placeholders for an image of this size, up to [`MOST_CELLS`] across and down.
pub fn placeholder_lines(width: usize, height: usize) -> Vec<String> {
    (0..height)
        .map(|row| {
            (0..width)
                .flat_map(|column| [PLACEHOLDER, POSITIONS[row], POSITIONS[column]])
                .collect()
        })
        .collect()
}

/// The colour that tells the terminal which image a placeholder belongs to.
pub fn id_color(id: u32) -> Color {
    Color::Rgb((id >> 16) as u8, (id >> 8) as u8, id as u8)
}

/// Write one command.
fn command(out: &mut impl Write, controls: &str, data: &[u8]) -> io::Result<()> {
    out.write_all(b"\x1b_G")?;
    out.write_all(controls.as_bytes())?;
    out.write_all(b";")?;
    out.write_all(data)?;
    out.write_all(b"\x1b\\")
}

/// A PNG to send, and how many characters it covers.
pub(crate) struct Image<'a> {
    pub id: u32,
    pub png: &'a [u8],
    pub columns: usize,
    pub rows: usize,
}

/// Send an image, in pieces. `q=2` keeps the terminal from answering, as its answers would arrive as key presses.
pub(crate) fn transmit(out: &mut impl Write, image: &Image) -> io::Result<()> {
    let encoded = STANDARD.encode(image.png);
    let mut chunks = encoded.as_bytes().chunks(CHUNK).peekable();
    let mut first = true;
    while let Some(chunk) = chunks.next() {
        let more = u8::from(chunks.peek().is_some());
        let controls = if first {
            let Image {
                id, columns, rows, ..
            } = image;
            format!("a=T,U=1,f=100,i={id},c={columns},r={rows},q=2,m={more}")
        } else {
            format!("q=2,m={more}")
        };
        command(out, &controls, chunk)?;
        first = false;
    }
    Ok(())
}

/// Send an image for the program to place itself with [`place_at`], rather than with placeholders.
pub fn store(out: &mut impl Write, id: u32, png: &[u8]) -> io::Result<()> {
    let encoded = STANDARD.encode(png);
    let mut chunks = encoded.as_bytes().chunks(CHUNK).peekable();
    let mut first = true;
    while let Some(chunk) = chunks.next() {
        let more = u8::from(chunks.peek().is_some());
        let controls = if first {
            format!("a=t,f=100,i={id},q=2,m={more}")
        } else {
            format!("q=2,m={more}")
        };
        command(out, &controls, chunk)?;
        first = false;
    }
    Ok(())
}

/// Where an image is placed directly: at a character, this many pixels into it across and down,
/// with this many of its rows of pixels shown from the top. Each offset is less than a character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub column: u16,
    pub row: u16,
    pub across: usize,
    pub down: usize,
    pub height: usize,
}

/// Place a stored image, or move it, as placing it again under the same placement id moves it.
/// An image can be placed in many places at once, each under a placement id of its own. It's
/// drawn above the text, and the terminal's cursor is put back where it was. An offset down of
/// none is left out, as the sample page's lines and marks have.
pub(crate) fn place_at(
    out: &mut impl Write,
    id: u32,
    placement: u32,
    at: Placement,
) -> io::Result<()> {
    let Placement {
        column,
        row,
        across,
        down,
        height,
    } = at;
    write!(out, "\x1b7\x1b[{};{}H", row + 1, column + 1)?;
    let down = if down > 0 {
        format!(",Y={down}")
    } else {
        String::new()
    };
    command(
        out,
        &format!("a=p,i={id},p={placement},X={across}{down},h={height},C=1,z=1,q=2"),
        &[],
    )?;
    out.write_all(b"\x1b8")
}

/// Take one placement of an image off the screen, keeping the image to place again.
pub(crate) fn unplace(out: &mut impl Write, id: u32, placement: u32) -> io::Result<()> {
    command(out, &format!("a=d,d=i,i={id},p={placement},q=2"), &[])
}

/// Take every placement of an image off the screen, keeping the image to place again.
pub(crate) fn unplace_all(out: &mut impl Write, id: u32) -> io::Result<()> {
    command(out, &format!("a=d,d=i,i={id},q=2"), &[])
}

/// Have the terminal forget an image, with every placement of it.
pub(super) fn forget(out: &mut impl Write, id: u32) -> io::Result<()> {
    command(out, &format!("a=d,d=I,i={id},q=2"), &[])
}

/// A character's size in pixels until the terminal says: the author's screen's, kitty full screen
/// on a MacBook.
pub const DEFAULT_CELL: (usize, usize) = (15, 39);

/// The id the question about images asks with, which no image of the program's has.
pub(super) const QUERY_ID: u32 = 31;
/// How long the terminal has to answer it. Both answer at once, so this is only for one that never
/// does.
const QUERY_WAIT: Duration = Duration::from_secs(2);

/// What the terminal said to the question about images, once it has said all of it: whether it
/// takes a PNG. The question is followed by one for the terminal's attributes, which every terminal
/// answers, and in order, so an answer that ends without an OK first is a no.
pub(super) fn answer(reply: &[u8]) -> Option<bool> {
    let text = String::from_utf8_lossy(reply);
    let attributes = text.find("\x1b[?")?;
    text[attributes..]
        .contains('c')
        .then(|| text[..attributes].contains(&format!("\x1b_Gi={QUERY_ID};OK\x1b\\")))
}

/// Whether the terminal draws images with the graphics protocol. It asks the terminal to take a PNG
/// of one dot and not show it, as the program's own images are, and reads the answer before
/// anything else reads the keyboard.
pub(crate) fn supports_graphics() -> io::Result<bool> {
    let mut tty = OpenOptions::new().read(true).write(true).open("/dev/tty")?;
    let mut reader: File = tty.try_clone()?;
    terminal::enable_raw_mode()?;
    let dot = STANDARD.encode(png(&[vec![true]], [255; 3]));
    let asked = command(&mut tty, &format!("a=q,i={QUERY_ID},f=100"), dot.as_bytes())
        .and_then(|()| tty.write_all(b"\x1b[c"))
        .and_then(|()| tty.flush());
    let (send, receive) = mpsc::channel();
    // On its own thread, so a terminal that never answers can't hold up the program. If it
    // doesn't, the program stops, and the thread with it.
    std::thread::spawn(move || {
        let mut reply = Vec::new();
        let mut buffer = [0; 256];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(read) => reply.extend(&buffer[..read]),
            }
            if let Some(answer) = answer(&reply) {
                let _ = send.send(answer);
                return;
            }
        }
    });
    let supported = receive.recv_timeout(QUERY_WAIT).unwrap_or(false);
    terminal::disable_raw_mode()?;
    asked.map(|()| supported)
}
