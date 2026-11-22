//! A text in a grammar, edited a token at a time: moved through by the grammar, its tokens turned,
//! taken away, rested and typed over, and checked on every change, keeping the last text that
//! read for the column to play while what's typed doesn't (decision 456).
//!
//! The cursor isn't kept here: whoever shows the field keeps it, by its token's place, and each
//! change says where it goes after.

use std::ops::Range;

use super::{Around, Bad, Bracket, Grammar, GrammarId, Kind, Offer, Spot, Token, is_rest, split};
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
    /// `D` `C`: from the token to the end of the group it's in, or of the line at the top, so the
    /// brackets round it stay.
    ToEnd,
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
    /// How long the separator put in is: the grammar's, or a Euclid's comma inside its `( )`.
    sep: usize,
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
    /// On a kit line, the names of the tracks it spans, which its hits are (#1093): empty
    /// otherwise.
    names: Vec<String>,
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
            names: Vec::new(),
        };
        field.settle();
        field
    }

    /// The same field on a kit line, a `Mini`'s line whose hits are the names of the tracks it
    /// spans (#1093): read in the kit line's grammar, a name no track of the line has marked as
    /// text that doesn't read. With no names, or on any other grammar's field, it's the field as
    /// it was.
    #[must_use]
    pub fn in_kit(mut self, names: &[String]) -> Self {
        if names.is_empty() || !std::ptr::eq(self.grammar, GrammarId::Mini.grammar()) {
            return self;
        }
        self.grammar = GrammarId::Kit.grammar();
        self.names = names.to_vec();
        self.tokens.clear();
        self.settle();
        self
    }

    /// The same field where `!` enters a pattern (decision 474): on the Note column, a `Mini`'s
    /// line in which `x` is the last note typed too. On any other column or grammar, it's the
    /// field as it was.
    #[must_use]
    pub fn with_insert(mut self) -> Self {
        if !std::ptr::eq(self.grammar, GrammarId::Mini.grammar())
            || !matches!(self.column, ShapeColumn::Note { .. })
        {
            return self;
        }
        self.grammar = &super::mini::PATTERN;
        self.tokens.clear();
        self.settle();
        self
    }

    /// The names of the tracks a kit line spans, or none for any other field.
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// Another text in the same grammar, on the same column and kit line, as typed.
    #[must_use]
    pub fn retyped(&self, text: &str) -> Self {
        let mut field = Self::new(self.grammar, text, self.column);
        if !self.names.is_empty() {
            field.names.clone_from(&self.names);
            field.settle();
        }
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
        match (self.grammar.read)(&self.text, self.column, &self.names) {
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

    /// What goes between a token and one typed beside it: a comma among a Euclid's numbers, in its
    /// `( )`, and the grammar's separator anywhere else.
    fn between_at(&self, at: usize) -> &'static str {
        let inside = self
            .tokens
            .get(at)
            .filter(|token| token.kind.bracket().is_none());
        match inside.and_then(|_| self.enclosing(at)) {
            Some(open) if self.tokens[open].kind == Kind::Open(Bracket::Round) => ",",
            _ => self.grammar.between,
        }
    }

    /// Whether a text has a mark changing the token before it, or a Euclid's `(`, at a byte.
    fn opens_or_modifies(&self, text: &str, at: usize) -> bool {
        split(self.grammar, text).iter().any(|token| {
            token.at.start == at
                && matches!(token.kind, Kind::Modifier | Kind::Open(Bracket::Round))
        })
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
            Object::ToEnd => {
                let start = self.whole(at).start;
                let end = self
                    .enclosing(start)
                    .and_then(|open| self.partner(open))
                    .unwrap_or(self.tokens.len());
                Some(start..end)
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
        let turned = (self.grammar.turn)(token.kind, text, sign, large, self.column, &self.names)
            .ok_or("It doesn't turn")?;
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
        if self.grammar.rest.is_empty() {
            return Err("Nothing rests here: x takes it away");
        }
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
        // A Euclid's comma goes as a space does.
        let apart = |c: &char| c.is_whitespace() || *c == ',';
        let after = self.text[bytes.end..]
            .chars()
            .take_while(apart)
            .map(char::len_utf8)
            .sum::<usize>();
        if after > 0 {
            bytes.end += after;
        } else {
            let before = self.text[..bytes.start]
                .chars()
                .rev()
                .take_while(apart)
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
            sep: 0,
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

    /// Why there's no room for another token, where it has as many as it takes.
    fn room(&self, more: usize) -> Result<(), &'static str> {
        match self.grammar.most {
            Some((most, full)) if self.tokens.len() + more > most => Err(full),
            _ => Ok(()),
        }
    }

    /// How many tokens it has room for, where it has a most; and how many it takes.
    pub fn most(&self) -> Option<usize> {
        self.grammar.most.map(|(most, _)| most)
    }

    /// `p`: what was kept, after the token, or over the tokens from it on a field of fixed length.
    /// Where the cursor goes after: on the last token put.
    pub fn paste(&mut self, at: usize) -> Result<usize, &'static str> {
        let yanked = self
            .yanked
            .clone()
            .ok_or("Nothing kept to paste: y or d first")?;
        self.room(split(self.grammar, &yanked).len())?;
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
        if place != Place::Over && !self.tokens.is_empty() {
            self.room(1)?;
        }
        let one = one || (fixed && self.grammar.width.is_some());
        let between = self.between_at(at);
        let insert = |from, gap| Insert {
            from,
            to: from,
            gap,
            sep: between.len(),
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

    /// Start typing at the end, after the last token, keeping the text (decision 463): `i` and `a`
    /// on a setting's cell. A field of fixed length has no room there.
    pub fn append(&mut self) -> Result<(), &'static str> {
        if self.fixed().is_some() {
            return Err("A field of fixed length has no room after its tokens");
        }
        let end = self.text.trim_end().len();
        if end < self.text.len() {
            self.replace(end..self.text.len(), "");
        }
        let (from, gap) = if self.tokens.is_empty() {
            (end, Gap::None)
        } else {
            self.room(1)?;
            let between = self.grammar.between;
            self.replace(end..end, between);
            let gap = if between.is_empty() {
                Gap::None
            } else {
                Gap::Before
            };
            (end + between.len(), gap)
        };
        self.insert = Some(Insert {
            from,
            to: from,
            gap,
            sep: self.grammar.between.len(),
            one: false,
        });
        Ok(())
    }

    /// `cc` and `S` on a setting's cell: the whole text kept for `p` and typed again from nothing,
    /// or on a field of fixed length each token put back to what it starts as, to be typed over.
    pub fn replace_all(&mut self) {
        if !self.text.trim().is_empty() {
            self.yanked = Some(self.text.trim().to_owned());
        }
        if let Some(count) = self.fixed() {
            let blank = vec![self.grammar.blank; count].join(self.grammar.between);
            self.replace(0..self.text.len(), &blank);
            return;
        }
        self.replace(0..self.text.len(), "");
        self.insert = Some(Insert {
            from: 0,
            to: 0,
            gap: Gap::None,
            sep: 0,
            one: false,
        });
    }

    /// A character typed, and whether typing goes on: a token typed as `r` types one, or over a
    /// token on a field of fixed length, ends it, and [`Field::end_insert`] says where the cursor
    /// goes.
    pub fn typed(&mut self, c: char) -> bool {
        let Some(mut insert) = self.insert else {
            return false;
        };
        // A mark typed first after a token, `?`, `*2` or a Euclid's `(`, joins the token rather
        // than standing apart from it.
        if insert.from == insert.to && insert.gap == Gap::Before && insert.sep > 0 {
            let start = insert.from - insert.sep;
            let mut joined = self.text.clone();
            joined.replace_range(start..insert.from, "");
            joined.insert(start, c);
            if self.opens_or_modifies(&joined, start) {
                self.text.replace_range(start..insert.from, "");
                insert = Insert {
                    from: start,
                    to: start,
                    gap: Gap::None,
                    ..insert
                };
            }
        }
        // A closing bracket typed where its partner was put in steps over it.
        if Bracket::of(c).is_some_and(|(_, opens)| !opens) && self.text[insert.to..].starts_with(c)
        {
            insert.to += c.len_utf8();
            self.insert = Some(insert);
            self.settle();
            return true;
        }
        self.text.insert(insert.to, c);
        let at = insert.to;
        insert.to += c.len_utf8();
        // An opening bracket the grammar reads as one gets its partner after the cursor, as an
        // editor's auto-pairs do, so a group is never left open.
        if let Some((bracket, true)) = Bracket::of(c)
            && split(self.grammar, &self.text)
                .iter()
                .any(|token| token.at.start == at && token.kind == Kind::Open(bracket))
        {
            self.text.insert(insert.to, bracket.close());
        }
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
        // An opening bracket taken back takes its partner put in after it, the pair being empty.
        let mut end = insert.to + c.len_utf8();
        if let Some((bracket, true)) = Bracket::of(c)
            && self.text[end..].starts_with(bracket.close())
        {
            end += bracket.close().len_utf8();
        }
        self.replace(insert.to..end, "");
    }

    /// Back to the tokens, and where the cursor goes: on the last token typed.
    pub fn end_insert(&mut self) -> usize {
        let Some(insert) = self.insert.take() else {
            return 0;
        };
        if insert.from == insert.to {
            let gap = insert.sep;
            match insert.gap {
                Gap::None => {}
                Gap::Before => self.replace(insert.from - gap..insert.from, ""),
                Gap::After => self.replace(insert.to..insert.to + gap, ""),
            }
        }
        // On the last token typed, as Vim's `Esc` leaves the cursor on the last character, so `D`
        // and `x` go from there; with nothing typed, where it was.
        let from = match insert.gap {
            _ if insert.from < insert.to => insert.to - 1,
            Gap::Before => insert.from - insert.sep,
            _ => insert.from,
        };
        self.place_from(from)
    }

    /// The word being typed: from the last space, comma or bracket typed to where the next
    /// character goes, which `C-n` completes.
    fn word(&self, insert: Insert) -> Range<usize> {
        let ends = |c: char| c.is_whitespace() || c == ',' || Bracket::of(c).is_some();
        let start = self.text[insert.from..insert.to]
            .char_indices()
            .rev()
            .find(|(_, c)| ends(*c))
            .map_or(insert.from, |(at, c)| insert.from + at + c.len_utf8());
        start..insert.to
    }

    /// What's typed of the word being typed, while it is.
    pub fn typed_word(&self) -> Option<&str> {
        let insert = self.insert?;
        Some(&self.text[self.word(insert)])
    }

    /// Where `K` and `C-n` ask what can go, from the cursor, and the bytes what's chosen goes in
    /// place of: the token's, or while typing the word typed so far, after the token before it.
    pub fn spot(&self, at: usize) -> (Spot<'_>, Range<usize>) {
        if let Some(insert) = self.insert {
            let word = self.word(insert);
            let before = self
                .tokens
                .iter()
                .rev()
                .find(|token| token.at.end <= word.start)
                .map(|token| (token.kind, &self.text[token.at.clone()]));
            return (Spot::After(before), word);
        }
        match self.tokens.get(at) {
            Some(token) => (
                Spot::On {
                    kind: token.kind,
                    at,
                    text: &self.text[token.at.clone()],
                },
                token.at.clone(),
            ),
            None => (Spot::After(None), self.text.len()..self.text.len()),
        }
    }

    /// What can go at the cursor, for `K` to list and `C-n` to complete from.
    /// On a kit line, the names it spans are what's around it, where whoever asks gives none.
    pub fn offers(&self, at: usize, around: &Around) -> Vec<Offer> {
        let named = Around {
            tracks: if around.tracks.is_empty() {
                &self.names
            } else {
                around.tracks
            },
            ..*around
        };
        (self.grammar.offer)(self.spot(at).0, &named)
    }

    /// What `K` or `C-n` offered, chosen: while typing, in place of the word typed so far, typing
    /// going on unless a token as long as each is ends it; and among the tokens in the token's
    /// place, where on a group's bracket another pair of brackets changes the group's, and
    /// anything else goes in place of the group. Where the cursor goes, once back among the
    /// tokens.
    pub fn choose(&mut self, at: usize, chosen: &str) -> Result<Option<usize>, &'static str> {
        if let Some(mut insert) = self.insert {
            let word = self.word(insert);
            insert.to = word.start + chosen.len();
            self.insert = Some(insert);
            self.replace(word, chosen);
            let typed = self.text[insert.from..insert.to].chars().count();
            let ends = insert.one && self.grammar.width.is_some_and(|width| typed >= width);
            return Ok(ends.then(|| self.end_insert()));
        }
        let token = self.tokens.get(at).ok_or("Nothing here to replace")?;
        if token.kind.bracket().is_none() {
            self.put(at, chosen);
            return Ok(Some(at));
        }
        let whole = self.whole(at);
        if let Some((open, close)) = pair(chosen) {
            // Close first, so the opening bracket's place stays where it was.
            for place in [whole.end - 1, whole.start] {
                let token = &self.tokens[place];
                let opens = matches!(token.kind, Kind::Open(_));
                let with = if opens { open } else { close };
                self.replace(token.at.clone(), with);
            }
            return Ok(Some(at));
        }
        let bytes = self.bytes(&whole);
        self.replace(bytes, chosen);
        Ok(Some(whole.start))
    }
}

/// A pair of brackets offered for a group's, `[ ]` or `< >`, as the text of each.
fn pair(text: &str) -> Option<(&str, &str)> {
    let mut brackets = text.split_whitespace();
    let (open, close) = (brackets.next()?, brackets.next()?);
    let bracket = |text: &str| text.chars().next().and_then(Bracket::of);
    match (bracket(open)?, bracket(close)?, brackets.next()) {
        ((opened, true), (closed, false), None) if opened == closed => Some((open, close)),
        _ => None,
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
        assert_eq!(field.end_insert(), 3, "on the 50, the last typed");
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
        // The `[` closes itself as it's typed, so the text reads all along.
        for c in "[2".chars() {
            assert!(field.typed(c));
        }
        assert_eq!(field.text(), "10 [2] 30");
        assert!(field.bad().is_none());
        // Typing its `]` steps over the one put in, and taking that back leaves the group open.
        field.typed(']');
        assert_eq!(field.text(), "10 [2] 30");
        assert_eq!(field.good(), "10 [2] 30");
        field.backspace();
        assert_eq!(field.text(), "10 [2 30");
        assert_eq!(field.bad_token(), Some(1));
        assert_eq!(field.bad().unwrap().why, "an open with no close");
        assert_eq!(field.good(), "10 [2] 30");
        assert_eq!(field.end_insert(), 2, "on the 2, the last typed");
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

    fn around() -> Around<'static> {
        Around {
            column: note(),
            from: 0,
            to: 0,
            tracks: &[],
            reached: &[],
        }
    }

    /// `K` asks in a token's place, and while typing after the token before the word typed.
    #[test]
    fn k_asks_in_a_tokens_place_and_typing_after_the_token_before() {
        let mut field = words("10 [20 30]");
        let (spot, bytes) = field.spot(0);
        assert_eq!(
            spot,
            Spot::On {
                kind: Kind::Value,
                at: 0,
                text: "10"
            }
        );
        assert_eq!(bytes, 0..2);
        assert_eq!(field.offers(1, &around())[0].text, "[ ]");
        field.insert(2, Place::After, false).unwrap();
        assert_eq!(field.typed_word(), Some(""));
        assert_eq!(field.spot(2).0, Spot::After(Some((Kind::Value, "20"))));
        field.typed('3');
        assert_eq!(field.typed_word(), Some("3"));
        assert_eq!(field.spot(2).1, 7..8);
        assert_eq!(field.offers(2, &around()).last().unwrap().text, "*2");
    }

    /// What's chosen goes in place of the word typed, and typing goes on; among the tokens, in
    /// the token's place, another pair of brackets changing a group's and a rest taking its place.
    #[test]
    fn what_k_offers_goes_in_place_of_the_token_or_the_word() {
        let mut field = words("10 [20 30]");
        field.insert(0, Place::After, false).unwrap();
        field.typed('4');
        assert_eq!(field.choose(0, "40"), Ok(None));
        assert_eq!(field.text(), "10 40 [20 30]");
        assert!(field.typed(' '));
        assert_eq!(field.typed_word(), Some(""), "a new word after a space");
        field.backspace();
        field.end_insert();
        assert_eq!(field.choose(0, "50"), Ok(Some(0)));
        assert_eq!(field.text(), "50 40 [20 30]");
        assert_eq!(field.choose(2, "< >"), Ok(Some(2)));
        assert_eq!(field.text(), "50 40 <20 30>");
        assert_eq!(field.choose(2, "."), Ok(Some(2)));
        assert_eq!(field.text(), "50 40 .");
        assert_eq!(pair("[ >"), None);
    }

    /// On `Weights`, a weight chosen while typing over one is typed, and back on it.
    #[test]
    fn a_weight_chosen_while_typing_over_one_ends_the_typing() {
        let mut field = Field::new(GrammarId::Weights.grammar(), "F08FF", pentatonic());
        let around = Around {
            column: pentatonic(),
            ..around()
        };
        assert_eq!(field.offers(1, &around).len(), 16);
        field.insert(1, Place::Over, false).unwrap();
        assert_eq!(field.offers(1, &around).len(), 16);
        assert_eq!(field.choose(1, "C"), Ok(Some(1)));
        assert_eq!(field.text(), "FC8FF");
        assert!(field.inserting().is_none());
    }

    /// `i` and `a` on a cell type at the end, keeping the text, and with nothing typed leave it as
    /// it was (decision 463).
    #[test]
    fn appending_keeps_the_text_and_types_after_it() {
        let mut field = Field::new(GrammarId::Rhythm.grammar(), "x . [. x] ", note());
        field.append().unwrap();
        for c in "x(3,8)".chars() {
            field.typed(c);
        }
        assert_eq!(field.text(), "x . [. x] x(3,8)");
        assert!(field.bad().is_none());
        let mut field = Field::new(GrammarId::Rhythm.grammar(), "x .", note());
        field.append().unwrap();
        field.end_insert();
        assert_eq!(field.text(), "x .");
        let mut empty = Field::new(GrammarId::List.grammar(), "", note());
        empty.append().unwrap();
        empty.typed('C');
        assert_eq!(empty.text(), "C");
        let mut weights = Field::new(GrammarId::Weights.grammar(), "F08FF", pentatonic());
        assert!(weights.append().is_err());
    }

    /// `cc` and `S` type the whole text again, keeping it for `p`; on a field of fixed length each
    /// token goes back to what it starts as, to be typed over.
    #[test]
    fn replacing_it_all_starts_from_nothing_and_keeps_what_was_there() {
        let mut field = Field::new(GrammarId::Moves.grammar(), "P L R", note());
        field.replace_all();
        assert_eq!(field.text(), "");
        assert!(field.inserting().is_some());
        field.typed('R');
        field.end_insert();
        assert_eq!(field.paste(0), Ok(1));
        assert_eq!(field.text(), "R P L R");
        let mut weights = Field::new(GrammarId::Weights.grammar(), "F08FF", pentatonic());
        weights.replace_all();
        assert_eq!(weights.text(), "FFFFF");
        assert!(weights.inserting().is_none());
    }

    /// A rhythm field, typing `typed` from `place` on the token at `at`, then back to the tokens.
    fn typed_at(text: &str, at: usize, place: Place, typed: &str) -> Field {
        let mut field = Field::new(GrammarId::Rhythm.grammar(), text, note());
        field.insert(at, place, false).unwrap();
        for c in typed.chars() {
            field.typed(c);
        }
        field.end_insert();
        field
    }

    /// Inside a Euclid's `( )`, what's typed beside a number goes after a comma, so `a` on the 8
    /// gives it a rotation and `i` on the 3 a number before it, and the line still reads.
    #[test]
    fn typing_beside_a_euclids_number_goes_after_a_comma() {
        let after = typed_at("x(3,8) .", 3, Place::After, "2");
        assert_eq!(after.text(), "x(3,8,2) .");
        assert!(after.bad().is_none());
        assert_eq!(
            typed_at("x(3,8) .", 2, Place::Before, "2").text(),
            "x(2,3,8) ."
        );
        assert_eq!(
            typed_at("x(3,8) .", 3, Place::Over, "16").text(),
            "x(3,16) ."
        );
        // Nothing typed takes the comma back out.
        assert_eq!(typed_at("x(3,8) .", 3, Place::After, "").text(), "x(3,8) .");
        // Outside it, a space as ever.
        assert_eq!(
            typed_at("x(3,8) .", 5, Place::After, "x").text(),
            "x(3,8) . x"
        );
    }

    /// A mark typed first after `a` joins the token, and anything else stands apart.
    #[test]
    fn a_mark_typed_after_a_token_joins_it() {
        assert_eq!(typed_at("x . x", 0, Place::After, "?30").text(), "x?30 . x");
        assert_eq!(typed_at("x . x", 2, Place::After, "*2").text(), "x . x*2");
        assert_eq!(
            typed_at("x . x", 0, Place::After, "(3,8").text(),
            "x(3,8) . x"
        );
        assert_eq!(typed_at("x . x", 0, Place::After, "x").text(), "x x . x");
        // After a group's bracket, a Euclid joins the group.
        assert_eq!(
            typed_at("x [x x]", 4, Place::After, "(3,8").text(),
            "x [x x](3,8)"
        );
    }

    /// An opening bracket gets its partner after the cursor; typing the partner steps over it,
    /// and taking the opening one back with nothing in it takes both.
    #[test]
    fn brackets_close_themselves_as_they_open() {
        let mut field = Field::new(GrammarId::Rhythm.grammar(), "", note());
        field.append().unwrap();
        for c in "x [x x".chars() {
            field.typed(c);
        }
        assert_eq!(field.text(), "x [x x]");
        assert!(field.bad().is_none(), "never open while typing");
        field.typed(']');
        field.typed(' ');
        field.typed('<');
        assert_eq!(field.text(), "x [x x] <>");
        field.backspace();
        assert_eq!(field.text(), "x [x x] ");
        for c in "<x .> x(3,8)".chars() {
            field.typed(c);
        }
        field.end_insert();
        assert_eq!(field.text(), "x [x x] <x .> x(3,8)");
        assert!(field.bad().is_none());
    }

    /// `D` takes away from the token to the end of the line, or of the group it's in, its brackets
    /// kept, and keeps it for `p`; `C` types in its place.
    #[test]
    fn d_takes_away_to_the_end_of_the_line_or_its_group() {
        let mut line = Field::new(GrammarId::Rhythm.grammar(), "x . [x x] x", note());
        assert_eq!(line.delete(1, Object::ToEnd), Ok(0));
        assert_eq!(line.text(), "x");
        assert_eq!(line.paste(0), Ok(1));
        assert_eq!(line.text(), "x . [x x] x");
        let mut group = Field::new(GrammarId::Rhythm.grammar(), "x [x . x] x", note());
        group.delete(3, Object::ToEnd).unwrap();
        assert_eq!(group.text(), "x [x] x");
        let mut euclid = Field::new(GrammarId::Rhythm.grammar(), "x(3,8,2)", note());
        euclid.delete(3, Object::ToEnd).unwrap();
        assert_eq!(euclid.text(), "x(3)");
        let mut changed = Field::new(GrammarId::Rhythm.grammar(), "x . x x", note());
        changed.change(2, Object::ToEnd).unwrap();
        for c in "[x x]".chars() {
            changed.typed(c);
        }
        changed.end_insert();
        assert_eq!(changed.text(), "x . [x x]");
    }
}
