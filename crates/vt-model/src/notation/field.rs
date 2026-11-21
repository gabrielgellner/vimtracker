//! A text in a grammar, edited a token at a time: moved through by the grammar, its tokens turned,
//! taken away, rested and typed over, and checked on every change, keeping the last text that
//! read for the column to play while what's typed doesn't (decision 456).
//!
//! The cursor isn't kept here: whoever shows the field keeps it, by its token's place, and each
//! change says where it goes after.

use std::ops::Range;

use super::{Bad, Bracket, Grammar, Kind, Token, is_rest, split};
use crate::shape::ShapeColumn;

/// A move of the cursor among the tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    /// `h` `l`: the token before or after, into and out of groups, in reading order.
    Left,
    Right,
    /// `w` `b`: the next or previous token at the same depth, a group as one.
    Next,
    Previous,
    /// `^` `$`: the first or last token.
    First,
    Last,
    /// `%`: the bracket matching the one under the cursor, or the next one's.
    Match,
    /// `(` `)`: out to the start or the end of the group the cursor is in.
    GroupStart,
    GroupEnd,
}

/// What `d`, `c` and `y` cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Object {
    /// `it`: the token, a group's bracket standing for the group.
    Token,
    /// `at`: the token and the modifiers after it.
    TokenAround,
    /// `i[` `i<` `i(`: what the nearest group of these brackets round the cursor holds.
    Inside(Bracket),
    /// `a[` `a<` `a(`: the same group with its brackets.
    Around(Bracket),
}

/// Where what's typed goes, against the token under the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Before,
    After,
    Over,
}

/// Where a separator was put in for what's typed, which goes again if nothing is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Gap {
    None,
    Before,
    After,
}

/// What's being typed into the text: from where, and where the next character goes, by byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Insert {
    pub from: usize,
    pub to: usize,
    gap: Gap,
    /// Back to the tokens once one is typed, as `r` is.
    one: bool,
}

/// A text-like setting's text while it's edited.
#[derive(Debug, Clone)]
pub struct Field {
    grammar: &'static Grammar,
    column: ShapeColumn,
    text: String,
    tokens: Vec<Token>,
    /// The last text that read, which the column plays while the text doesn't.
    good: String,
    bad: Option<Bad>,
    /// Tokens `~` rested, by place, and what they were, for `~` to give back.
    rested: Vec<(usize, String)>,
    /// What `y` and `d` took, for `p`.
    yanked: Option<String>,
    insert: Option<Insert>,
}

impl Field {
    /// A field of a setting's text as it shows, in its grammar, on its column.
    pub fn new(grammar: &'static Grammar, text: &str, column: ShapeColumn) -> Self {
        let mut field = Self {
            grammar,
            column,
            text: text.to_owned(),
            tokens: Vec::new(),
            good: text.to_owned(),
            bad: None,
            rested: Vec::new(),
            yanked: None,
            insert: None,
        };
        field.settle();
        field
    }

    pub fn grammar(&self) -> &'static Grammar {
        self.grammar
    }

    pub fn column(&self) -> ShapeColumn {
        self.column
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    /// The text of the token at a place.
    pub fn token(&self, at: usize) -> Option<&str> {
        self.tokens
            .get(at)
            .map(|token| &self.text[token.at.clone()])
    }

    /// The last text that read.
    pub fn good(&self) -> &str {
        &self.good
    }

    /// Where and why the text doesn't read, if it doesn't.
    pub fn bad(&self) -> Option<&Bad> {
        self.bad.as_ref()
    }

    /// The token the text stops reading at, or none where it's past the last.
    pub fn bad_token(&self) -> Option<usize> {
        let bad = self.bad.as_ref()?;
        self.tokens
            .iter()
            .position(|token| token.at.contains(&bad.at))
    }

    /// What's being typed, if anything is.
    pub fn inserting(&self) -> Option<Insert> {
        self.insert
    }

    /// How many tokens it always has, where it's of fixed length.
    pub fn fixed(&self) -> Option<usize> {
        (self.grammar.fixed)(self.column)
    }

    /// The text again as the setting shows it, once it's taken what was read.
    pub fn set(&mut self, text: &str) {
        if text != self.text {
            self.text = text.to_owned();
            self.settle();
        }
    }

    /// Back to the last text that read, and out of typing.
    pub fn put_back(&mut self) {
        self.insert = None;
        self.text.clone_from(&self.good);
        self.settle();
    }

    fn settle(&mut self) {
        let count = self.tokens.len();
        self.tokens = split(self.grammar, &self.text);
        if self.tokens.len() != count {
            self.rested.clear();
        }
        match (self.grammar.read)(&self.text, self.column) {
            Ok(()) => {
                self.good.clone_from(&self.text);
                self.bad = None;
            }
            Err(bad) => self.bad = Some(bad),
        }
    }

    /// Where a motion takes the cursor from a token, `count` times, or none where `h` from the
    /// first leaves the field.
    pub fn moved(&self, at: usize, motion: Motion, count: usize) -> Option<usize> {
        let count = count.max(1);
        let Some(last) = self.tokens.len().checked_sub(1) else {
            return (motion != Motion::Left).then_some(0);
        };
        let at = at.min(last);
        Some(match motion {
            Motion::Left if at == 0 => return None,
            Motion::Left => at.saturating_sub(count),
            Motion::Right => (at + count).min(last),
            Motion::First => 0,
            Motion::Last => last,
            Motion::Next => (0..count).fold(at, |at, _| self.next(at)),
            Motion::Previous => (0..count).fold(at, |at, _| self.previous(at)),
            Motion::Match => self
                .partner(at)
                .or_else(|| {
                    (at + 1..=last)
                        .find(|next| self.tokens[*next].kind.bracket().is_some())
                        .and_then(|next| self.partner(next))
                })
                .unwrap_or(at),
            Motion::GroupStart => self.group(at).unwrap_or(at),
            Motion::GroupEnd => self
                .group(at)
                .and_then(|open| self.partner(open))
                .unwrap_or(at),
        })
    }

    /// The next token at the same depth, past a group the cursor opens, or where it is at the end
    /// of its group.
    fn next(&self, at: usize) -> usize {
        let depth = self.tokens[at].depth;
        let from = match self.tokens[at].kind {
            Kind::Open(_) => self.partner(at).unwrap_or(at),
            _ => at,
        };
        for (next, token) in self.tokens.iter().enumerate().skip(from + 1) {
            if token.depth < depth {
                break;
            }
            if token.depth == depth && !matches!(token.kind, Kind::Close(_)) {
                return next;
            }
        }
        at
    }

    /// The token before at the same depth, a group by its opening bracket.
    fn previous(&self, at: usize) -> usize {
        let depth = self.tokens[at].depth;
        for before in (0..at).rev() {
            let token = &self.tokens[before];
            if token.depth < depth {
                break;
            }
            if token.depth == depth {
                return match token.kind {
                    Kind::Close(_) => self.partner(before).unwrap_or(before),
                    _ => before,
                };
            }
        }
        at
    }

    /// The bracket matching the one at a place, if it's a bracket and has one.
    fn partner(&self, at: usize) -> Option<usize> {
        let token = self.tokens.get(at)?;
        let (bracket, opens) = token.kind.bracket()?;
        let matches = |other: &Token| {
            other.depth == token.depth
                && other.kind
                    == if opens {
                        Kind::Close(bracket)
                    } else {
                        Kind::Open(bracket)
                    }
        };
        if opens {
            (at + 1..self.tokens.len()).find(|other| matches(&self.tokens[*other]))
        } else {
            (0..at).rev().find(|other| matches(&self.tokens[*other]))
        }
    }

    /// The opening bracket of the group a token is inside, not counting its own.
    fn enclosing(&self, at: usize) -> Option<usize> {
        let depth = self.tokens.get(at)?.depth.checked_sub(1)?;
        (0..at).rev().find(|before| {
            let token = &self.tokens[*before];
            token.depth == depth && matches!(token.kind, Kind::Open(_))
        })
    }

    /// The opening bracket of the group the cursor is in: its own, on a bracket.
    fn group(&self, at: usize) -> Option<usize> {
        match self.tokens.get(at)?.kind {
            Kind::Open(_) => Some(at),
            Kind::Close(_) => self.partner(at),
            _ => self.enclosing(at),
        }
    }

    /// A token, or the whole group where it's a bracket.
    fn whole(&self, at: usize) -> Range<usize> {
        match self.tokens[at].kind {
            Kind::Open(_) => at..self.partner(at).map_or(at + 1, |close| close + 1),
            Kind::Close(_) => self.partner(at).unwrap_or(at)..at + 1,
            _ => at..at + 1,
        }
    }

    /// The tokens an object covers from the cursor, or none where there's no such group.
    pub fn object(&self, at: usize, object: Object) -> Option<Range<usize>> {
        if at >= self.tokens.len() {
            return None;
        }
        match object {
            Object::Token => Some(self.whole(at)),
            Object::TokenAround => {
                let whole = self.whole(at);
                let depth = self.tokens[whole.start].depth;
                let modifiers = self.tokens[whole.end..]
                    .iter()
                    .take_while(|token| token.kind == Kind::Modifier && token.depth == depth)
                    .count();
                Some(whole.start..whole.end + modifiers)
            }
            Object::Inside(bracket) | Object::Around(bracket) => {
                let mut open = self.group(at)?;
                while self.tokens[open].kind != Kind::Open(bracket) {
                    open = self.enclosing(open)?;
                }
                let close = self.partner(open)?;
                Some(match object {
                    Object::Inside(_) => open + 1..close,
                    _ => open..close + 1,
                })
            }
        }
    }

    /// The bytes of a run of tokens: between the brackets, where it's none inside a group.
    fn bytes(&self, tokens: &Range<usize>) -> Range<usize> {
        match tokens.is_empty() {
            true => self.tokens[tokens.start - 1].at.end..self.tokens[tokens.start].at.start,
            false => self.tokens[tokens.start].at.start..self.tokens[tokens.end - 1].at.end,
        }
    }

    fn replace(&mut self, bytes: Range<usize>, with: &str) {
        self.text.replace_range(bytes, with);
        self.settle();
    }

    /// A token's text in place of what's there, where there's a token at the place, or the
    /// whole text where there are none.
    pub fn put(&mut self, at: usize, text: &str) {
        let bytes = match self.tokens.get(at) {
            Some(token) => token.at.clone(),
            None => 0..self.text.len(),
        };
        self.replace(bytes, text);
    }

    /// `=` `-` `+` `_`: the token turned in its own units.
    pub fn turn(&mut self, at: usize, sign: i16, large: bool) -> Result<(), &'static str> {
        let token = self.tokens.get(at).ok_or("Nothing to turn")?;
        let text = &self.text[token.at.clone()];
        let turned = (self.grammar.turn)(token.kind, text, sign, large).ok_or("It doesn't turn")?;
        self.put(at, &turned);
        Ok(())
    }

    /// `x`: the token taken away, or on a field of fixed length back to what it starts as. Where
    /// the cursor goes after.
    pub fn remove(&mut self, at: usize) -> Result<usize, &'static str> {
        if self.fixed().is_some() {
            let blank = self.grammar.blank;
            self.put(at, blank);
            return Ok(at);
        }
        self.delete(at, Object::Token)
    }

    /// `~`: the token rested, or a rest given back what it was while the field has been open.
    pub fn rest(&mut self, at: usize) -> Result<(), &'static str> {
        let token = self.tokens.get(at).ok_or("Nothing to rest")?;
        if token.kind.bracket().is_some() {
            return Err("A group's brackets don't rest; ~ rests a token in it");
        }
        let text = self.text[token.at.clone()].to_owned();
        if text != self.grammar.rest && !is_rest(&text) {
            let rest = self.grammar.rest;
            self.put(at, rest);
            self.rested.retain(|(place, _)| *place != at);
            self.rested.push((at, text));
            return Ok(());
        }
        let was = self
            .rested
            .iter()
            .position(|(place, _)| *place == at)
            .ok_or("A rest already, with nothing to give back")?;
        let (_, was) = self.rested.remove(was);
        self.put(at, &was);
        Ok(())
    }

    /// `d`: what an object covers taken away, and kept for `p`; on a field of fixed length, each
    /// token back to what it starts as. Where the cursor goes after.
    pub fn delete(&mut self, at: usize, object: Object) -> Result<usize, &'static str> {
        let tokens = self.object(at, object).ok_or("No such group here")?;
        if tokens.is_empty() {
            return Err("Nothing inside it");
        }
        self.yank(at, object)?;
        if self.fixed().is_some() {
            let blank = self.grammar.blank;
            for place in tokens.clone().rev() {
                self.put(place, blank);
            }
            return Ok(tokens.start);
        }
        let mut bytes = self.bytes(&tokens);
        // A space either side goes with it, after it where there's one.
        let after = self.text[bytes.end..]
            .chars()
            .take_while(|c| c.is_whitespace())
            .map(char::len_utf8)
            .sum::<usize>();
        if after > 0 {
            bytes.end += after;
        } else {
            let before = self.text[..bytes.start]
                .chars()
                .rev()
                .take_while(|c| c.is_whitespace())
                .map(char::len_utf8)
                .sum::<usize>();
            bytes.start -= before;
        }
        self.replace(bytes, "");
        Ok(tokens.start.min(self.tokens.len().saturating_sub(1)))
    }

    /// `c`: what an object covers taken away, kept for `p`, and typed in its place; on a field of
    /// fixed length, only a token, typed over.
    pub fn change(&mut self, at: usize, object: Object) -> Result<(), &'static str> {
        let tokens = self.object(at, object).ok_or("No such group here")?;
        if self.fixed().is_some() {
            if tokens.len() != 1 {
                return Err("A field of fixed length changes a token at a time");
            }
            return self.insert(tokens.start, Place::Over, false);
        }
        if !tokens.is_empty() {
            self.yank(at, object)?;
        }
        let bytes = self.bytes(&tokens);
        self.replace(bytes.clone(), "");
        self.insert = Some(Insert {
            from: bytes.start,
            to: bytes.start,
            gap: Gap::None,
            one: false,
        });
        Ok(())
    }

    /// `y`: what an object covers, kept for `p`.
    pub fn yank(&mut self, at: usize, object: Object) -> Result<(), &'static str> {
        let tokens = self.object(at, object).ok_or("No such group here")?;
        if tokens.is_empty() {
            return Err("Nothing inside it");
        }
        self.yanked = Some(self.text[self.bytes(&tokens)].to_owned());
        Ok(())
    }

    /// `p`: what was kept, after the token, or over the tokens from it on a field of fixed length.
    /// Where the cursor goes after: on the last token put.
    pub fn paste(&mut self, at: usize) -> Result<usize, &'static str> {
        let yanked = self
            .yanked
            .clone()
            .ok_or("Nothing kept to paste: y or d first")?;
        if self.fixed().is_some() {
            let pasted: Vec<String> = split(self.grammar, &yanked)
                .iter()
                .map(|token| yanked[token.at.clone()].to_owned())
                .collect();
            let room = self.tokens.len().saturating_sub(at).min(pasted.len());
            for (place, text) in pasted.iter().enumerate().take(room).rev() {
                self.put(at + place, text);
            }
            return Ok(at + room.saturating_sub(1));
        }
        if self.tokens.is_empty() {
            self.replace(0..self.text.len(), &yanked);
            return Ok(0);
        }
        let end = self.bytes(&self.whole(at)).end;
        let pasted = format!("{}{yanked}", self.grammar.between);
        let from = end + self.grammar.between.len();
        self.replace(end..end, &pasted);
        Ok(self.place_from(from))
    }

    /// The first token that ends after a byte, or the last.
    fn place_from(&self, byte: usize) -> usize {
        self.tokens
            .iter()
            .position(|token| token.at.end > byte)
            .unwrap_or(self.tokens.len().saturating_sub(1))
    }

    /// Start typing, before the token, after it or over it; `one` goes back to the tokens once one
    /// is typed, as `r` does, as does typing over a token on a field of fixed length.
    pub fn insert(&mut self, at: usize, place: Place, one: bool) -> Result<(), &'static str> {
        let fixed = self.fixed().is_some();
        if fixed && place != Place::Over {
            return Err("A field of fixed length has no room between its tokens");
        }
        let one = one || (fixed && self.grammar.width.is_some());
        let between = self.grammar.between;
        let insert = |from, gap| Insert {
            from,
            to: from,
            gap,
            one,
        };
        if self.tokens.is_empty() {
            self.insert = Some(insert(self.text.len(), Gap::None));
            return Ok(());
        }
        let bytes = self.bytes(&self.whole(at.min(self.tokens.len() - 1)));
        let (gap, from) = match place {
            Place::Over => {
                self.replace(bytes.clone(), "");
                (Gap::None, bytes.start)
            }
            Place::Before => {
                self.replace(bytes.start..bytes.start, between);
                (Gap::After, bytes.start)
            }
            Place::After => {
                self.replace(bytes.end..bytes.end, between);
                (Gap::Before, bytes.end + between.len())
            }
        };
        let gap = if between.is_empty() { Gap::None } else { gap };
        self.insert = Some(insert(from, gap));
        Ok(())
    }

    /// A character typed, and whether typing goes on: a token typed as `r` types one, or over a
    /// token on a field of fixed length, ends it, and [`Field::end_insert`] says where the cursor
    /// goes.
    pub fn typed(&mut self, c: char) -> bool {
        let Some(mut insert) = self.insert else {
            return false;
        };
        self.text.insert(insert.to, c);
        insert.to += c.len_utf8();
        self.insert = Some(insert);
        self.settle();
        let typed = self.text[insert.from..insert.to].chars().count();
        !(insert.one && self.grammar.width.is_some_and(|width| typed >= width))
    }

    /// `Backspace`: the last character typed taken back.
    pub fn backspace(&mut self) {
        let Some(mut insert) = self.insert else {
            return;
        };
        let Some(c) = self.text[insert.from..insert.to].chars().next_back() else {
            return;
        };
        insert.to -= c.len_utf8();
        self.insert = Some(insert);
        self.replace(insert.to..insert.to + c.len_utf8(), "");
    }

    /// Back to the tokens, and where the cursor goes: on what was typed.
    pub fn end_insert(&mut self) -> usize {
        let Some(insert) = self.insert.take() else {
            return 0;
        };
        if insert.from == insert.to {
            let gap = self.grammar.between.len();
            match insert.gap {
                Gap::None => {}
                Gap::Before => self.replace(insert.from - gap..insert.from, ""),
                Gap::After => self.replace(insert.to..insert.to + gap, ""),
            }
        }
        let from = match insert.gap {
            Gap::Before if insert.from == insert.to => insert.from - self.grammar.between.len(),
            _ => insert.from,
        };
        self.place_from(from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notation::GrammarId;
    use crate::notation::tests::WORDS;
    use crate::scale::{Scale, ScaleKind};

    fn note() -> ShapeColumn {
        ShapeColumn::Note {
            scale: Scale::default(),
            writes: None,
        }
    }

    fn words(text: &str) -> Field {
        Field::new(&WORDS, text, note())
    }

    /// The text of each place a motion goes from the first token, one after another.
    fn walk(field: &Field, from: usize, motions: &[(Motion, usize)]) -> Vec<String> {
        let mut at = from;
        motions
            .iter()
            .map(|(motion, count)| {
                at = field.moved(at, *motion, *count).expect("in the field");
                field.token(at).unwrap().to_owned()
            })
            .collect()
    }

    #[test]
    fn h_and_l_go_by_token_into_and_out_of_groups_and_w_b_stay_at_a_depth() {
        let field = words("10 [. 20*2] <30 40> 50");
        let right = [(Motion::Right, 1); 6];
        assert_eq!(walk(&field, 0, &right), ["[", ".", "20", "*2", "]", "<"]);
        // `h` from the first leaves the field; counts go so far and stop at the ends.
        assert_eq!(field.moved(0, Motion::Left, 1), None);
        assert_eq!(field.moved(3, Motion::Left, 9), Some(0));
        assert_eq!(field.moved(3, Motion::Right, 99), Some(10));
        // `w` takes a group as one token, and inside one stays inside it.
        let next = [(Motion::Next, 1); 4];
        assert_eq!(walk(&field, 0, &next), ["[", "<", "50", "50"]);
        assert_eq!(walk(&field, 2, &[(Motion::Next, 1); 3]), ["20", "*2", "*2"]);
        assert_eq!(
            walk(&field, 10, &[(Motion::Previous, 1); 3]),
            ["<", "[", "10"]
        );
        assert_eq!(walk(&field, 0, &[(Motion::Next, 2)]), ["<"]);
        assert_eq!(walk(&field, 4, &[(Motion::Previous, 1)]), ["20"]);
    }

    #[test]
    fn percent_matches_brackets_and_parentheses_go_out_to_the_group() {
        let field = words("10 [. <20 30>] 40");
        assert_eq!(walk(&field, 1, &[(Motion::Match, 1)]), ["]"]);
        assert_eq!(walk(&field, 7, &[(Motion::Match, 1)]), ["["]);
        // From a token that isn't one, the next bracket's match, as Vim's `%`.
        assert_eq!(walk(&field, 0, &[(Motion::Match, 1)]), ["]"]);
        assert_eq!(walk(&field, 4, &[(Motion::GroupStart, 1)]), ["<"]);
        assert_eq!(walk(&field, 4, &[(Motion::GroupEnd, 1)]), [">"]);
        assert_eq!(walk(&field, 2, &[(Motion::GroupEnd, 1)]), ["]"]);
        // At the top, there's no group to go out to.
        assert_eq!(walk(&field, 0, &[(Motion::GroupEnd, 1)]), ["10"]);
        assert_eq!(
            walk(&field, 0, &[(Motion::First, 1), (Motion::Last, 1)]),
            ["10", "40"]
        );
    }

    #[test]
    fn objects_cover_a_token_its_modifiers_or_a_group() {
        let mut field = words("10 [. 20*2?] <30 40> 50");
        let covered = |field: &Field, at, object| {
            let tokens = field.object(at, object).unwrap();
            field.text[field.bytes(&tokens)].to_owned()
        };
        assert_eq!(covered(&field, 3, Object::Token), "20");
        assert_eq!(covered(&field, 3, Object::TokenAround), "20*2?");
        assert_eq!(
            covered(&field, 3, Object::Inside(Bracket::Square)),
            ". 20*2?"
        );
        assert_eq!(
            covered(&field, 3, Object::Around(Bracket::Square)),
            "[. 20*2?]"
        );
        assert_eq!(covered(&field, 1, Object::Token), "[. 20*2?]");
        assert_eq!(covered(&field, 8, Object::Inside(Bracket::Angle)), "30 40");
        assert_eq!(field.object(3, Object::Inside(Bracket::Angle)), None);
        // `da<` takes the alternation away and a space with it; `yat` then `p` repeats a token
        // with its modifiers.
        let at = field.delete(9, Object::Around(Bracket::Angle)).unwrap();
        assert_eq!(field.text(), "10 [. 20*2?] 50");
        assert_eq!(field.token(at), Some("50"));
        field.yank(3, Object::TokenAround).unwrap();
        let at = field.paste(0).unwrap();
        assert_eq!(field.text(), "10 20*2? [. 20*2?] 50");
        assert_eq!(field.token(at), Some("20"));
        assert!(field.bad().is_none());
    }

    #[test]
    fn c_types_over_a_group_or_inside_one() {
        let mut field = words("10 [20 30] 40");
        field.change(2, Object::Inside(Bracket::Square)).unwrap();
        assert_eq!(field.text(), "10 [] 40");
        for c in ". 50".chars() {
            field.typed(c);
        }
        assert_eq!(field.end_insert(), 2);
        assert_eq!(field.text(), "10 [. 50] 40");
        field.change(2, Object::Inside(Bracket::Square)).unwrap();
        assert_eq!(field.end_insert(), 2);
        assert_eq!(field.text(), "10 [] 40");
        // What was there is kept for `p`.
        assert_eq!(field.paste(0), Ok(1));
        assert_eq!(field.text(), "10 . 50 [] 40");
    }

    #[test]
    fn x_takes_a_token_away_and_tilde_rests_it_and_gives_it_back() {
        let mut field = words("10 20 30");
        assert_eq!(field.remove(1), Ok(1));
        assert_eq!(field.text(), "10 30");
        assert_eq!(field.remove(1), Ok(0));
        assert_eq!(field.text(), "10");
        let mut field = words("10 20 30");
        field.rest(1).unwrap();
        assert_eq!(field.text(), "10 . 30");
        field.turn(0, 1, true).unwrap();
        assert_eq!(field.text(), "20 . 30");
        field.rest(1).unwrap();
        assert_eq!(field.text(), "20 20 30");
        assert!(field.rest(1).is_ok());
        assert!(field.rest(1).is_ok());
        let mut field = words("10 ~ 30");
        assert!(field.rest(1).is_err(), "a rest with nothing to give back");
        assert_eq!(field.turn(1, 1, false), Err("It doesn't turn"));
    }

    /// Text that doesn't read is marked where it stops reading, and the last that read is kept.
    #[test]
    fn typing_is_checked_as_it_goes_and_the_last_good_text_is_kept() {
        let mut field = words("10 30");
        field.insert(0, Place::After, false).unwrap();
        assert_eq!(field.text(), "10  30");
        for c in "[2".chars() {
            assert!(field.typed(c));
        }
        assert_eq!(field.text(), "10 [2 30");
        assert_eq!(field.bad_token(), Some(1));
        assert_eq!(field.bad().unwrap().why, "an open with no close");
        assert_eq!(field.good(), "10  30");
        field.typed(']');
        assert!(field.bad().is_none());
        assert_eq!(field.good(), "10 [2] 30");
        field.backspace();
        assert_eq!(field.text(), "10 [2 30");
        assert_eq!(field.end_insert(), 1);
        field.put_back();
        assert_eq!(field.text(), "10 [2] 30");
        // Nothing typed takes back the space put in for it.
        field.insert(4, Place::Before, false).unwrap();
        assert_eq!(field.text(), "10 [2]  30");
        assert_eq!(field.end_insert(), 4);
        assert_eq!(field.text(), "10 [2] 30");
        field.insert(0, Place::Over, true).unwrap();
        assert!(field.typed('4'));
        assert!(field.typed('4'));
        assert_eq!(field.end_insert(), 0);
        assert_eq!(field.text(), "44 [2] 30");
    }

    fn pentatonic() -> ShapeColumn {
        ShapeColumn::Note {
            scale: Scale {
                on: true,
                kind: ScaleKind::MinorPentatonic,
                ..Scale::default()
            },
            writes: None,
        }
    }

    /// On a field of fixed length, `Weights`' digits, nothing is taken away or put between: `x`
    /// puts back an `F`, `p` writes over, and typing over a digit is back after one.
    #[test]
    fn a_field_of_fixed_length_keeps_its_length() {
        let mut field = Field::new(GrammarId::Weights.grammar(), "F08FF", pentatonic());
        assert_eq!(field.fixed(), Some(5));
        assert_eq!(field.remove(1), Ok(1));
        assert_eq!(field.text(), "FF8FF");
        field.rest(2).unwrap();
        assert_eq!(field.text(), "FF0FF");
        field.rest(2).unwrap();
        assert_eq!(field.text(), "FF8FF");
        field.yank(2, Object::Token).unwrap();
        assert_eq!(field.paste(4), Ok(4));
        assert_eq!(field.text(), "FF8F8");
        assert!(field.insert(0, Place::After, false).is_err());
        field.insert(1, Place::Over, false).unwrap();
        assert_eq!(field.text(), "F8F8");
        assert!(field.bad().is_some(), "a digit short while it's typed");
        assert_eq!(field.good(), "FF8F8");
        assert!(!field.typed('g'), "back after one");
        assert_eq!(field.end_insert(), 1);
        assert_eq!(field.bad_token(), Some(1));
        assert_eq!(field.good(), "FF8F8");
        field.put_back();
        assert_eq!(field.text(), "FF8F8");
        assert_eq!(field.delete(0, Object::Token), Ok(0));
        assert_eq!(field.text(), "FF8F8");
    }
}
