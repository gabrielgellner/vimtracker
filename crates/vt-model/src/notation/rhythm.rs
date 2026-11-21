//! An `Own` rhythm's grammar (#1091, decision 461): a line of hits and rests in Tidal's
//! mini-notation, `x . [. x] <x .> x*2 x!3 x@2 x? x?30 x(3,8,2)`, read stepwise, a top-level token
//! being `Step` rows, and compiled into the [`Beat`]s its render walks.
//!
//! The meanings of the tokens and the shape of the reading are adapted from TidalCycles'
//! `tidal-core/src/Sound/Tidal/ParseBP.hs`, by Alex McLean and the TidalCycles authors, GPL-3.0
//! as this program is: `resolve_size`'s `!` as places of their own and `@` as one place made
//! longer, `<>` turning a lap at a time with a choice inside turning when it comes round, and a
//! Euclid's `(k,n,r)` as `k` hits over `n` parts of its place. The Rust is written fresh. Three
//! meanings are this program's own (the study's Decided 5, 6 and 10): a token is `Step` rows, not
//! a share of a cycle; Euclid's third number moves the hits later, as `Rotate` does, where
//! Tidal's moves them earlier; and `.` is a rest, as the guides print rhythms, with `~` read as
//! one too, so there are no Tidal feet. `?30` plays 30% of the hits, as `Chance` counts.
//! Nothing is taken from Strudel's code.

use super::{
    About, Around, Bad, Bracket, Grammar, Kind, Offer, Spot, Token, as_it_is, groups, is_rest,
    nth_of, split,
};
use crate::shape::{Beat, Euclid, Role, ShapeColumn, What, World};

/// The longest text a rhythm takes, in bytes.
pub const MAX_RHYTHM: usize = 128;
/// The most times `*` plays a token in its place.
const MOST_FAST: u16 = 16;
/// The most places `!` and `@` give a token, and the most parts a Euclid's has.
const MOST_PLACES: u16 = 64;
/// The most groups inside groups.
const MOST_DEPTH: usize = 8;
/// What a bare `?` plays, in percent.
const HALF: u8 = 50;

pub(super) static RHYTHM: Grammar = Grammar {
    kinds: &[
        Kind::Hit,
        Kind::Rest,
        Kind::Modifier,
        Kind::Value,
        Kind::Open(Bracket::Square),
        Kind::Close(Bracket::Square),
        Kind::Open(Bracket::Angle),
        Kind::Close(Bracket::Angle),
        Kind::Open(Bracket::Round),
        Kind::Close(Bracket::Round),
        Kind::Unread,
    ],
    split: split_rhythm,
    read: |text, _| compile(text).map(|_| ()),
    meaning: |kind| match kind {
        Kind::Hit => "a hit: the row it lands on gets a value",
        Kind::Rest => "a rest: nothing on its rows",
        Kind::Modifier => {
            "a change to the token before it: *n plays it n times in its place, !n gives it n \
             places, @n makes it n places long, ? plays it half the time and ?30 30% of it"
        }
        Kind::Value => "one of a Euclid's numbers: its hits, its parts, then how many parts later",
        Kind::Open(Bracket::Square) | Kind::Close(Bracket::Square) => {
            "a group: its tokens share its place"
        }
        Kind::Open(Bracket::Angle) | Kind::Close(Bracket::Angle) => {
            "a choice: one of its tokens each lap, the next each time the line comes round"
        }
        Kind::Open(Bracket::Round) | Kind::Close(Bracket::Round) => {
            "a Euclid's numbers: (3,8) spreads 3 hits over 8 parts of the token's place, and \
             (3,8,2) moves them 2 parts later"
        }
        _ => "what isn't a hit, x, a rest, . or ~, a group or a change, so the text doesn't read",
    },
    about: |kind, _, _| {
        let name = match kind {
            Kind::Hit => "Hit",
            Kind::Rest => "Rest",
            Kind::Modifier => "Change",
            Kind::Value => "Euclid's number",
            Kind::Open(Bracket::Square) | Kind::Close(Bracket::Square) => "Group",
            Kind::Open(Bracket::Angle) | Kind::Close(Bracket::Angle) => "Choice",
            Kind::Open(Bracket::Round) | Kind::Close(Bracket::Round) => "Euclid",
            _ => "Unread",
        };
        About {
            name: name.to_owned(),
            subject: String::new(),
        }
    },
    turn,
    offer,
    // A rhythm's table holds each row's hit, by its place among the hits.
    rows: |tokens, at| nth_of(tokens, at, Kind::Hit),
    fixed: |_| None,
    blank: ".",
    rest: ".",
    most: None,
    width: None,
    between: " ",
    types: |_| false,
    from_setting: as_it_is,
    to_setting: as_it_is,
};

/// Whether a character ends a word: a space, a comma or a bracket.
fn ends(c: char) -> bool {
    c.is_whitespace() || c == ',' || Bracket::of(c).is_some()
}

/// A modifier's mark.
fn modifies(c: char) -> bool {
    matches!(c, '*' | '!' | '@' | '?')
}

/// Hits and rests a character each, brackets, a modifier with its number, a number by itself,
/// and anything else up to the next space, comma or bracket.
fn split_rhythm(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        let digits = |chars: &mut std::iter::Peekable<std::str::CharIndices>| {
            let mut end = at + c.len_utf8();
            while let Some((next, digit)) = chars.next_if(|(_, c)| c.is_ascii_digit()) {
                end = next + digit.len_utf8();
            }
            end
        };
        let token = if c.is_whitespace() || c == ',' {
            continue;
        } else if let Some((bracket, opens)) = Bracket::of(c) {
            let kind = if opens {
                Kind::Open(bracket)
            } else {
                Kind::Close(bracket)
            };
            Token::new(kind, at..at + 1)
        } else if matches!(c, 'x' | 'X') {
            Token::new(Kind::Hit, at..at + 1)
        } else if is_rest(&text[at..at + c.len_utf8()]) {
            Token::new(Kind::Rest, at..at + 1)
        } else if modifies(c) {
            Token::new(Kind::Modifier, at..digits(&mut chars))
        } else if c.is_ascii_digit() {
            Token::new(Kind::Value, at..digits(&mut chars))
        } else {
            let mut end = at + c.len_utf8();
            while let Some((next, more)) =
                chars.next_if(|(_, c)| !ends(*c) && !modifies(*c) && !matches!(c, 'x' | '.' | '~'))
            {
                end = next + more.len_utf8();
            }
            Token::new(Kind::Unread, at..end)
        };
        tokens.push(token);
    }
    tokens
}

fn bad(at: usize, why: impl Into<String>) -> Bad {
    Bad {
        at,
        why: why.into(),
    }
}

/// A modifier's mark and its number, if it has one.
fn modifier(text: &str) -> (char, Option<u16>) {
    let mut chars = text.chars();
    let mark = chars.next().unwrap_or('*');
    (mark, chars.as_str().parse().ok())
}

/// Reading a rhythm's tokens into beats.
struct Reader<'a> {
    text: &'a str,
    tokens: Vec<Token>,
    next: usize,
    beats: Vec<Beat>,
    hits: u8,
}

impl Reader<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.next)
    }

    fn word(&self, token: &Token) -> &str {
        &self.text[token.at.clone()]
    }

    /// A group's or a choice's tokens, its head already among the beats, to the bracket that
    /// closes it, or for the line to its end.
    fn line(
        &mut self,
        head: usize,
        opened: Option<(Bracket, usize)>,
        depth: usize,
    ) -> Result<(), Bad> {
        loop {
            let Some(token) = self.peek().cloned() else {
                if let Some((bracket, at)) = opened {
                    let (open, close) = brackets(bracket);
                    return Err(bad(at, format!("{open} at {at} has no {close}")));
                }
                break;
            };
            match token.kind {
                Kind::Close(bracket) => {
                    if opened.is_some_and(|(open, _)| open == bracket) {
                        self.next += 1;
                        break;
                    }
                    let (open, close) = brackets(bracket);
                    return Err(bad(
                        token.at.start,
                        format!("{close} has no {open} before it"),
                    ));
                }
                Kind::Hit | Kind::Rest | Kind::Open(Bracket::Square | Bracket::Angle) => {
                    self.step(depth)?;
                }
                Kind::Open(Bracket::Round) => {
                    return Err(bad(
                        token.at.start,
                        "A Euclid's ( ) goes after a hit or a group, as x(3,8)",
                    ));
                }
                Kind::Modifier => {
                    return Err(bad(
                        token.at.start,
                        format!(
                            "{} changes the token before it, and there's none",
                            self.word(&token)
                        ),
                    ));
                }
                Kind::Value => {
                    return Err(bad(
                        token.at.start,
                        "A number goes after a change, as *2, or in a Euclid's ( )",
                    ));
                }
                _ => {
                    return Err(bad(
                        token.at.start,
                        format!("{} isn't a hit, x, or a rest, . or ~", self.word(&token)),
                    ));
                }
            }
        }
        self.beats[head].size = u16::try_from(self.beats.len() - head).unwrap_or(u16::MAX);
        Ok(())
    }

    /// A hit, a rest, a group or a choice, and the changes after it.
    fn step(&mut self, depth: usize) -> Result<(), Bad> {
        let token = self.tokens[self.next].clone();
        self.next += 1;
        let head = self.beats.len();
        match token.kind {
            Kind::Hit => {
                let hit = self.hits;
                self.hits = self
                    .hits
                    .checked_add(1)
                    .ok_or_else(|| bad(token.at.start, "A line has at most 255 hits"))?;
                self.beats.push(Beat::new(What::Hit(hit)));
            }
            Kind::Open(bracket) => {
                if depth >= MOST_DEPTH {
                    return Err(bad(
                        token.at.start,
                        format!("Groups go {MOST_DEPTH} deep at most"),
                    ));
                }
                let what = if bracket == Bracket::Angle {
                    What::Choice
                } else {
                    What::Group
                };
                self.beats.push(Beat::new(what));
                self.line(head, Some((bracket, token.at.start)), depth + 1)?;
            }
            _ => self.beats.push(Beat::new(What::Rest)),
        }
        while let Some(token) = self.peek().cloned() {
            match token.kind {
                Kind::Modifier => {
                    self.next += 1;
                    self.change(head, &token)?;
                }
                Kind::Open(Bracket::Round) => {
                    self.next += 1;
                    self.euclid(head, token.at.start)?;
                }
                _ => break,
            }
        }
        Ok(())
    }

    /// A modifier on the beat at `head`.
    fn change(&mut self, head: usize, token: &Token) -> Result<(), Bad> {
        let at = token.at.start;
        let beat = &mut self.beats[head];
        let within = |number: Option<u16>, low: u16, high: u16, what: &str| match number {
            Some(number) if (low..=high).contains(&number) => Ok(number),
            _ => Err(bad(at, format!("{what}, from {low} to {high}"))),
        };
        match modifier(&self.text[token.at.clone()]) {
            ('*', number) => {
                beat.fast = within(number, 1, MOST_FAST, "* takes how many times, as *2")?;
            }
            ('!', None) => beat.repeat = 2,
            ('!', number) => {
                beat.repeat = within(number, 1, MOST_PLACES, "! takes how many places, as !3")?;
            }
            ('@', number) => {
                beat.weight = within(
                    number,
                    1,
                    MOST_PLACES,
                    "@ takes how many places long, as @2",
                )?;
            }
            ('?', None) => beat.chance = HALF,
            (_, number) => {
                beat.chance = within(number, 0, 100, "? takes the percent it plays, as ?30")? as u8;
            }
        }
        Ok(())
    }

    /// A Euclid's numbers after the `(` at `open`, to its `)`.
    fn euclid(&mut self, head: usize, open: usize) -> Result<(), Bad> {
        let mut numbers = Vec::with_capacity(3);
        loop {
            let Some(token) = self.peek().cloned() else {
                return Err(bad(open, format!("( at {open} has no )")));
            };
            self.next += 1;
            match token.kind {
                Kind::Value if numbers.len() < 3 => {
                    numbers.push((
                        self.word(&token).parse::<u16>().unwrap_or(u16::MAX),
                        token.at.start,
                    ));
                }
                Kind::Close(Bracket::Round) => break,
                _ => {
                    return Err(bad(
                        token.at.start,
                        "A Euclid's ( ) holds two or three numbers, as (3,8) or (3,8,2)",
                    ));
                }
            }
        }
        let [(hits, at_hits), (parts, at_parts), rest @ ..] = numbers.as_slice() else {
            return Err(bad(
                open,
                "A Euclid's ( ) holds two or three numbers, as (3,8) or (3,8,2)",
            ));
        };
        if !(1..=MOST_PLACES).contains(parts) {
            return Err(bad(
                *at_parts,
                format!("A Euclid's parts go from 1 to {MOST_PLACES}"),
            ));
        }
        if hits > parts {
            return Err(bad(
                *at_hits,
                format!("{hits} hits don't fit in {parts} parts"),
            ));
        }
        let later = rest.first().map_or(0, |(later, _)| later % parts);
        self.beats[head].euclid = Some(Euclid::new(*hits, *parts, later));
        Ok(())
    }
}

/// A bracket's opening and closing characters.
fn brackets(bracket: Bracket) -> (char, char) {
    match bracket {
        Bracket::Square => ('[', ']'),
        Bracket::Angle => ('<', '>'),
        Bracket::Round => ('(', ')'),
    }
}

/// A rhythm's text read into its beats, the line first, or where and why it doesn't read.
pub fn compile(text: &str) -> Result<Vec<Beat>, Bad> {
    if text.len() > MAX_RHYTHM {
        return Err(bad(
            MAX_RHYTHM,
            format!("A rhythm is at most {MAX_RHYTHM} characters"),
        ));
    }
    let mut reader = Reader {
        text,
        tokens: split(&RHYTHM, text),
        next: 0,
        beats: vec![Beat::new(What::Group)],
        hits: 0,
    };
    reader.line(0, None, 0)?;
    Ok(reader.beats)
}

/// A named rhythm's part as an `Own` rhythm's text, a token a pulse, each hit with the part's
/// `Child` chance where that's less than every one: the son clave's bell is
/// `x . . x . . x . . . x . x . . .`.
pub fn preset(rhythm: World, part: Role, child: u8) -> String {
    (0..rhythm.pulses())
        .map(|pulse| match rhythm.strikes(part, pulse) {
            true if child < 100 => format!("x?{child}"),
            true => "x".to_owned(),
            false => ".".to_owned(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A token turned: a change's number by one, or by four where `large`, a chance by 10% or 25%,
/// and a Euclid's number by one or four.
fn turn(kind: Kind, text: &str, sign: i16, large: bool, _: ShapeColumn) -> Option<String> {
    let by = |small: i16, big: i16| sign * if large { big } else { small };
    match kind {
        Kind::Modifier => {
            let (mark, number) = modifier(text);
            let (now, low, high, by) = match mark {
                '?' => (number.unwrap_or(u16::from(HALF)), 0, 100, by(10, 25)),
                '*' => (number?, 1, MOST_FAST, by(1, 4)),
                '!' => (number.unwrap_or(2), 1, MOST_PLACES, by(1, 4)),
                _ => (number?, 1, MOST_PLACES, by(1, 4)),
            };
            let turned = (i32::from(now) + i32::from(by)).clamp(i32::from(low), i32::from(high));
            Some(format!("{mark}{turned}"))
        }
        Kind::Value => {
            let now: i32 = text.parse().ok()?;
            Some(
                (now + i32::from(by(1, 4)))
                    .clamp(0, i32::from(MOST_PLACES))
                    .to_string(),
            )
        }
        _ => None,
    }
}

fn offer_of(text: &str, meaning: &str) -> Offer {
    Offer {
        text: text.to_owned(),
        meaning: meaning.to_owned(),
    }
}

/// What can go in a token's place or after one: a hit or a rest, a group, and after a token the
/// changes it can take.
fn offer(spot: Spot, _: &Around) -> Vec<Offer> {
    let atoms = || {
        let mut offers = vec![offer_of("x", "a hit"), offer_of(".", "a rest")];
        offers.extend(groups(".").into_iter().filter(|offer| offer.text != "."));
        offers
    };
    let changes = || {
        vec![
            offer_of("*2", "twice in its place"),
            offer_of("!2", "again, in a place of its own"),
            offer_of("@2", "twice as long"),
            offer_of("?", "half the time; ?30 for 30%"),
            offer_of("(3,8)", "3 hits spread over 8 parts of its place"),
        ]
    };
    match spot {
        Spot::On {
            kind: Kind::Open(_) | Kind::Close(_),
            ..
        } => groups("."),
        Spot::On {
            kind: Kind::Modifier,
            ..
        } => changes(),
        Spot::On {
            kind: Kind::Value, ..
        } => (1..=16)
            .map(|number: u16| offer_of(&number.to_string(), ""))
            .collect(),
        Spot::On { .. } | Spot::After(None) => atoms(),
        Spot::After(Some(_)) => {
            let mut offers = atoms();
            offers.extend(changes());
            offers
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::Scale;

    fn note() -> ShapeColumn {
        ShapeColumn::Note {
            scale: Scale::default(),
            writes: None,
        }
    }

    fn kinds(text: &str) -> Vec<(&str, Kind)> {
        split(&RHYTHM, text)
            .into_iter()
            .map(|token| (&text[token.at.clone()], token.kind))
            .collect()
    }

    #[test]
    fn a_line_splits_into_hits_rests_groups_and_changes() {
        assert_eq!(
            kinds("x.~[x x]*2 x?30(3,8)"),
            [
                ("x", Kind::Hit),
                (".", Kind::Rest),
                ("~", Kind::Rest),
                ("[", Kind::Open(Bracket::Square)),
                ("x", Kind::Hit),
                ("x", Kind::Hit),
                ("]", Kind::Close(Bracket::Square)),
                ("*2", Kind::Modifier),
                ("x", Kind::Hit),
                ("?30", Kind::Modifier),
                ("(", Kind::Open(Bracket::Round)),
                ("3", Kind::Value),
                ("8", Kind::Value),
                (")", Kind::Close(Bracket::Round)),
            ]
        );
        assert_eq!(kinds("bd x"), [("bd", Kind::Unread), ("x", Kind::Hit)]);
    }

    #[test]
    fn what_doesnt_read_says_where_and_why() {
        let read = |text| (RHYTHM.read)(text, note());
        assert_eq!(read("x . [. x] x(3,8)"), Ok(()));
        assert_eq!(read("x <x . x?> x!3 x@2 x(3,8,2) ~"), Ok(()));
        assert_eq!(read(""), Ok(()));
        assert_eq!(read("x [x x"), Err(bad(2, "[ at 2 has no ]")));
        assert_eq!(read("x x]"), Err(bad(3, "] has no [ before it")));
        assert_eq!(
            read("*2 x"),
            Err(bad(0, "*2 changes the token before it, and there's none"))
        );
        assert_eq!(read("x bd").map_err(|bad| bad.at), Err(2));
        assert_eq!(read("x(9,8)"), Err(bad(2, "9 hits don't fit in 8 parts")));
        assert_eq!(read("x(3)").map_err(|bad| bad.at), Err(1));
        assert_eq!(read("x*0").map_err(|bad| bad.at), Err(1));
        assert_eq!(read("x?101").map_err(|bad| bad.at), Err(1));
        assert!(read(&"x ".repeat(80)).is_err(), "too long");
    }

    #[test]
    fn the_changes_land_on_the_beat_before_them() {
        let beats = compile("x*2 [x .]!3 x@2 x?30 x(3,8,10)").unwrap();
        let line = beats[0];
        assert_eq!((line.what, line.size), (What::Group, beats.len() as u16));
        assert_eq!(beats[1].fast, 2);
        assert_eq!(
            (beats[2].what, beats[2].size, beats[2].repeat),
            (What::Group, 3, 3)
        );
        assert_eq!(beats[5].weight, 2);
        assert_eq!(beats[6].chance, 30);
        // 3 of 8, `x..x..x.`, moved 10 parts later, 2 round its 8: `x.x..x..`.
        let tresillo = Euclid {
            parts: 8,
            hits: 0b0010_0101,
        };
        assert_eq!(beats[7].euclid, Some(Euclid::new(3, 8, 2)));
        assert_eq!(Euclid::new(3, 8, 10), Euclid::new(3, 8, 2));
        assert_eq!(
            Euclid::new(3, 8, 0),
            Euclid {
                parts: 8,
                hits: 0b0100_1001
            }
        );
        assert_eq!(beats[7].euclid, Some(tresillo));
        assert_eq!(beats[7].what, What::Hit(4));
    }

    #[test]
    fn a_change_turns_by_its_own_units() {
        assert_eq!(
            turn(Kind::Modifier, "*2", 1, false, note()).as_deref(),
            Some("*3")
        );
        assert_eq!(
            turn(Kind::Modifier, "*2", -1, true, note()).as_deref(),
            Some("*1")
        );
        assert_eq!(
            turn(Kind::Modifier, "?", 1, false, note()).as_deref(),
            Some("?60")
        );
        assert_eq!(
            turn(Kind::Modifier, "?90", 1, true, note()).as_deref(),
            Some("?100")
        );
        assert_eq!(
            turn(Kind::Modifier, "!", 1, false, note()).as_deref(),
            Some("!3")
        );
        assert_eq!(
            turn(Kind::Value, "3", 1, false, note()).as_deref(),
            Some("4")
        );
        assert_eq!(turn(Kind::Hit, "x", 1, false, note()), None);
    }

    #[test]
    fn a_named_rhythm_is_its_onsets_a_token_a_pulse() {
        assert_eq!(
            preset(World::SonClave, Role::Bell, 100),
            "x . . x . . x . . . x . x . . ."
        );
        assert_eq!(
            preset(World::Tresillo, Role::Bell, 70),
            "x?70 . . x?70 . . x?70 ."
        );
        for rhythm in World::ALL {
            for part in Role::ALL {
                assert!(compile(&preset(rhythm, part, 100)).is_ok());
            }
        }
    }

    #[test]
    fn a_hit_lights_by_its_place_among_the_hits() {
        let tokens = split(&RHYTHM, "x . [x x]*2");
        assert_eq!((RHYTHM.rows)(&tokens, 0), Some(0));
        assert_eq!((RHYTHM.rows)(&tokens, 1), None);
        assert_eq!((RHYTHM.rows)(&tokens, 4), Some(2));
        assert_eq!((RHYTHM.rows)(&tokens, 6), None);
    }
}
