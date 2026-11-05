//! The `:` command line.

use std::path::PathBuf;

use thiserror::Error;

/// A parsed command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    Write(Option<PathBuf>),
    Edit(PathBuf),
    Quit { force: bool },
    New(PathBuf),
    Sample(PathBuf),
    Inst(u32),
    Bpm(u32),
    Len(usize),
    TrackAdd(usize),
    TrackDel,
    Mute,
    Solo,
    Double,
    SetStep(u32),
}

/// Why a command line was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LineError {
    #[error("unknown command: {0}")]
    Unknown(String),
    #[error("{0} needs a path")]
    NeedsPath(&'static str),
    #[error(":inst takes a hex number")]
    Inst,
    #[error(":bpm takes a number from 20 to 400")]
    Bpm,
    #[error(":len takes a number of steps, 1 or more")]
    Len,
    #[error(":track takes add [n] or del")]
    Track,
    #[error(":set takes step=n")]
    Set,
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
        "inst" => u32::from_str_radix(argument, 16)
            .map(Line::Inst)
            .map_err(|_| LineError::Inst),
        "bpm" => millibpm(argument).map(Line::Bpm).ok_or(LineError::Bpm),
        "len" => match argument.parse() {
            Ok(0) | Err(_) => Err(LineError::Len),
            Ok(length) => Ok(Line::Len(length)),
        },
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
        "double" => Ok(Line::Double),
        "set" => argument
            .strip_prefix("step=")
            .and_then(|n| n.parse().ok())
            .map(Line::SetStep)
            .ok_or(LineError::Set),
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
        assert_eq!(parse("len 32"), Ok(Line::Len(32)));
        assert_eq!(parse("track add"), Ok(Line::TrackAdd(1)));
        assert_eq!(parse("track add 4"), Ok(Line::TrackAdd(4)));
        assert_eq!(parse("track del"), Ok(Line::TrackDel));
        assert_eq!(parse("track add 0"), Err(LineError::Track));
        assert_eq!(parse("sample a b.wav"), Ok(Line::Sample("a b.wav".into())));
    }

    #[test]
    fn bad_commands_are_refused() {
        assert_eq!(parse("len 0"), Err(LineError::Len));
        assert_eq!(parse("e"), Err(LineError::NeedsPath(":e")));
        assert_eq!(parse("nope"), Err(LineError::Unknown("nope".into())));
    }
}
