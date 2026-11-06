//! The `:` command line.

use std::path::PathBuf;

use thiserror::Error;
use vt_model::label::{is_label, slot_of};

/// A parsed command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    Write(Option<PathBuf>),
    Edit(PathBuf),
    Quit {
        force: bool,
    },
    New(PathBuf),
    Sample(PathBuf),
    /// Load a kit by name or path, or choose one.
    Kit(Option<PathBuf>),
    Inst(u32),
    /// Mark a slot as labelled and make it current, or with `None` unmark the current one.
    Label(Option<u32>),
    Bpm(u32),
    Len(usize),
    TrackAdd(usize),
    TrackDel,
    Mute,
    Solo,
    Double,
    SetStep(u32),
    SetSkip(u32),
    SetLimiter(bool),
    Seed(u64),
    Swing(u16),
    Chain(u32),
    Render {
        path: PathBuf,
        rows: Option<usize>,
        tail: bool,
    },
    TrackSwing(Option<u16>),
    TrackLevel(u8),
    Reset(Option<usize>),
}

/// Why a command line was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LineError {
    #[error("unknown command: {0}")]
    Unknown(String),
    #[error("{0} needs a path")]
    NeedsPath(&'static str),
    #[error(":inst takes a hex number or a label")]
    Inst,
    #[error(":label takes two characters, the first a letter, or off")]
    Label,
    #[error(":bpm takes a number from 20 to 400")]
    Bpm,
    #[error(":len takes a number of steps, 1 or more, with 0x for hex: 0x10")]
    Len,
    #[error(":track takes add [n] or del")]
    Track,
    #[error(":set takes step=n, skip=n, or limiter=on or off")]
    Set,
    #[error(":chain takes a chain ID in hex")]
    Chain,
    #[error(":render takes a path, --rows n and --tail")]
    Render,
    #[error(":seed takes a number")]
    Seed,
    #[error(":swing takes a percent from 50 to 75, to one decimal")]
    Swing,
    #[error(":tswing takes a percent from 50 to 75, or off")]
    TrackSwing,
    #[error(":tvol takes a level in hex, from 00 to FF")]
    TrackLevel,
    #[error(":reset takes a number of steps, 1 or more, with 0x for hex, or off")]
    Reset,
}

/// Expand a leading `~/` using the home directory.
fn path(text: &str) -> PathBuf {
    if let Some(rest) = text.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(text)
}

/// Read a tempo as thousandths of a BPM, so `137.63` is exact.
fn millibpm(text: &str) -> Option<u32> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if fraction.len() > 3 || (whole.is_empty() && fraction.is_empty()) {
        return None;
    }
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if !digits(whole) || !digits(fraction) {
        return None;
    }
    let whole: u32 = if whole.is_empty() {
        0
    } else {
        whole.parse().ok()?
    };
    let scaled: u32 = format!("{fraction:0<3}").parse().ok()?;
    whole.checked_mul(1000)?.checked_add(scaled)
}

/// Read a number of steps, 1 or more: decimal, or hex after `0x` as in Rust, since rows are numbered
/// in hex on screen.
fn steps(text: &str) -> Option<usize> {
    let parsed = match text.strip_prefix("0x") {
        Some(digits) => usize::from_str_radix(digits, 16).ok(),
        None => text.parse().ok(),
    };
    parsed.filter(|steps| *steps > 0)
}

/// Read a swing percent as tenths, so `66.7` is 667.
pub(crate) fn swing(text: &str) -> Option<u16> {
    let thousandths = millibpm(text)?;
    if !thousandths.is_multiple_of(100) {
        return None;
    }
    u16::try_from(thousandths / 100).ok()
}

/// Read `:render`'s arguments. Anything that isn't a flag is the path, which can hold spaces.
fn render(argument: &str) -> Result<Line, LineError> {
    let mut words = Vec::new();
    let mut rows = None;
    let mut tail = false;
    let mut parts = argument.split_whitespace();
    while let Some(part) = parts.next() {
        match part {
            "--tail" => tail = true,
            "--rows" => {
                let count = parts.next().and_then(|n| n.parse().ok());
                rows = Some(count.filter(|n| *n > 0).ok_or(LineError::Render)?);
            }
            word => words.push(word),
        }
    }
    if words.is_empty() {
        return Err(LineError::Render);
    }
    Ok(Line::Render {
        path: path(&words.join(" ")),
        rows,
        tail,
    })
}

/// Parse a command line, without its leading `:`.
pub fn parse(text: &str) -> Result<Line, LineError> {
    let text = text.trim();
    let (name, argument) = match text.split_once(char::is_whitespace) {
        Some((name, rest)) => (name, rest.trim()),
        None => (text, ""),
    };
    let needed = |label| {
        if argument.is_empty() {
            Err(LineError::NeedsPath(label))
        } else {
            Ok(path(argument))
        }
    };
    match name {
        "w" => Ok(Line::Write((!argument.is_empty()).then(|| path(argument)))),
        "e" => needed(":e").map(Line::Edit),
        "q" => Ok(Line::Quit { force: false }),
        "q!" => Ok(Line::Quit { force: true }),
        "new" => needed(":new").map(Line::New),
        "sample" => needed(":sample").map(Line::Sample),
        "kit" => Ok(Line::Kit((!argument.is_empty()).then(|| path(argument)))),
        "inst" => slot_of(argument)
            .or_else(|| u32::from_str_radix(argument, 16).ok())
            .map(Line::Inst)
            .ok_or(LineError::Inst),
        "label" if argument == "off" => Ok(Line::Label(None)),
        "label" if is_label(argument) => Ok(Line::Label(slot_of(argument))),
        "label" => Err(LineError::Label),
        "bpm" => millibpm(argument).map(Line::Bpm).ok_or(LineError::Bpm),
        "len" => steps(argument).map(Line::Len).ok_or(LineError::Len),
        "track" => match argument.split_whitespace().collect::<Vec<_>>()[..] {
            ["add"] => Ok(Line::TrackAdd(1)),
            ["add", count] => count
                .parse()
                .ok()
                .filter(|count| *count > 0)
                .map(Line::TrackAdd)
                .ok_or(LineError::Track),
            ["del"] => Ok(Line::TrackDel),
            _ => Err(LineError::Track),
        },
        "seed" => argument
            .parse()
            .map(Line::Seed)
            .map_err(|_| LineError::Seed),
        "swing" => swing(argument).map(Line::Swing).ok_or(LineError::Swing),
        "tswing" if argument == "off" => Ok(Line::TrackSwing(None)),
        "tswing" => swing(argument)
            .map(|value| Line::TrackSwing(Some(value)))
            .ok_or(LineError::TrackSwing),
        "tvol" => u8::from_str_radix(argument, 16)
            .map(Line::TrackLevel)
            .map_err(|_| LineError::TrackLevel),
        "reset" if argument == "off" => Ok(Line::Reset(None)),
        "reset" => steps(argument)
            .map(|steps| Line::Reset(Some(steps)))
            .ok_or(LineError::Reset),
        "render" => render(argument),
        "chain" => u32::from_str_radix(argument, 16)
            .map(Line::Chain)
            .map_err(|_| LineError::Chain),
        "double" => Ok(Line::Double),
        "set" => match argument.split_once('=') {
            Some(("step", n)) => n.parse().map(Line::SetStep).map_err(|_| LineError::Set),
            Some(("skip", n)) => n.parse().map(Line::SetSkip).map_err(|_| LineError::Set),
            Some(("limiter", "on")) => Ok(Line::SetLimiter(true)),
            Some(("limiter", "off")) => Ok(Line::SetLimiter(false)),
            _ => Err(LineError::Set),
        },
        "mute" => Ok(Line::Mute),
        "solo" => Ok(Line::Solo),
        other => Err(LineError::Unknown(other.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tempos_are_exact_thousandths() {
        assert_eq!(parse("bpm 137.63"), Ok(Line::Bpm(137_630)));
        assert_eq!(parse("bpm 120"), Ok(Line::Bpm(120_000)));
        assert_eq!(parse("bpm 99.5"), Ok(Line::Bpm(99_500)));
        assert_eq!(parse("bpm 99.1234"), Err(LineError::Bpm));
        assert_eq!(parse("bpm fast"), Err(LineError::Bpm));
    }

    #[test]
    fn commands_parse() {
        assert_eq!(parse("w"), Ok(Line::Write(None)));
        assert_eq!(parse("w a/b"), Ok(Line::Write(Some("a/b".into()))));
        assert_eq!(parse("q!"), Ok(Line::Quit { force: true }));
        assert_eq!(parse("inst 1A"), Ok(Line::Inst(0x1A)));
        assert_eq!(parse("inst bd"), Ok(Line::Inst(0xBD)));
        assert_eq!(parse("inst 5"), Ok(Line::Inst(5)));
        assert_eq!(parse("inst 100"), Ok(Line::Inst(0x100)));
        assert_eq!(parse("inst ch"), Ok(Line::Inst(slot_of("CH").unwrap())));
        assert_eq!(parse("inst 1h"), Err(LineError::Inst));
        assert_eq!(parse("label BD"), Ok(Line::Label(Some(0xBD))));
        assert_eq!(parse("label off"), Ok(Line::Label(None)));
        assert_eq!(parse("label 1A"), Err(LineError::Label));
        assert_eq!(parse("len 32"), Ok(Line::Len(32)));
        assert_eq!(parse("seed 42"), Ok(Line::Seed(42)));
        assert_eq!(
            parse("render a b.wav --rows 8 --tail"),
            Ok(Line::Render {
                path: "a b.wav".into(),
                rows: Some(8),
                tail: true,
            })
        );
        assert!(parse("render").is_err());
        assert!(parse("render a.wav --rows 0").is_err());
        assert!(parse("seed x").is_err());
        assert_eq!(parse("swing 66.7"), Ok(Line::Swing(667)));
        assert_eq!(parse("swing 50"), Ok(Line::Swing(500)));
        assert!(parse("swing 66.75").is_err());
        assert_eq!(parse("tswing 60"), Ok(Line::TrackSwing(Some(600))));
        assert_eq!(parse("tswing off"), Ok(Line::TrackSwing(None)));
        assert_eq!(parse("reset 64"), Ok(Line::Reset(Some(64))));
        assert_eq!(parse("reset off"), Ok(Line::Reset(None)));
        assert!(parse("reset 0").is_err());
        assert!(parse("reset").is_err());
        assert_eq!(parse("reset 0x40"), Ok(Line::Reset(Some(64))));
        assert_eq!(parse("len 16"), Ok(Line::Len(16)));
        assert_eq!(parse("len 0x10"), Ok(Line::Len(16)));
        assert_eq!(parse("len 0xfF"), Ok(Line::Len(255)));
        assert_eq!(
            parse("len 10"),
            Ok(Line::Len(10)),
            "plain numbers stay decimal"
        );
        for wrong in [
            "len 0x",
            "len 0x0",
            "len 0",
            "len 0xg",
            "len x10",
            "len 0X10",
            "reset x40",
            "reset 0x0",
        ] {
            assert!(parse(wrong).is_err(), "{wrong}");
        }
        assert_eq!(parse("tvol C0"), Ok(Line::TrackLevel(0xC0)));
        assert_eq!(parse("tvol"), Err(LineError::TrackLevel));
        assert_eq!(parse("tvol 100"), Err(LineError::TrackLevel));
        assert_eq!(parse("track add"), Ok(Line::TrackAdd(1)));
        assert_eq!(parse("track add 4"), Ok(Line::TrackAdd(4)));
        assert_eq!(parse("track del"), Ok(Line::TrackDel));
        assert_eq!(parse("set step=2"), Ok(Line::SetStep(2)));
        assert_eq!(parse("set skip=8"), Ok(Line::SetSkip(8)));
        assert_eq!(parse("set skip"), Err(LineError::Set));
        assert_eq!(parse("set limiter=off"), Ok(Line::SetLimiter(false)));
        assert_eq!(parse("set limiter=on"), Ok(Line::SetLimiter(true)));
        assert_eq!(parse("set limiter=2"), Err(LineError::Set));
        assert_eq!(parse("set other=1"), Err(LineError::Set));
        assert_eq!(parse("track add 0"), Err(LineError::Track));
        assert_eq!(parse("sample a b.wav"), Ok(Line::Sample("a b.wav".into())));
        assert_eq!(parse("kit"), Ok(Line::Kit(None)));
        assert_eq!(parse("kit 909"), Ok(Line::Kit(Some("909".into()))));
    }

    #[test]
    fn bad_commands_are_refused() {
        assert_eq!(parse("len 0"), Err(LineError::Len));
        assert_eq!(parse("e"), Err(LineError::NeedsPath(":e")));
        assert_eq!(parse("nope"), Err(LineError::Unknown("nope".into())));
    }
}
