//! A Gen page setting's field of tokens (#1089, decision 459): `l` on a cell whose text is in a
//! grammar, `Weights`, a `List`'s values, a `Tonnetz`'s moves or a mask, goes into its tokens, as
//! the shape panel's rows do (decision 456), drawn over the cell with what each token is for
//! under it. The token editor's keys come first, in `Mode::Tokens`; a key they don't bind leaves
//! the field and does what it does on the page. Each change that reads is written to the
//! generator as one undo step, as any turn on the page is.

use ratatui::crossterm::event::KeyCode;
use vt_model::notation::{Around, Field};
use vt_model::shape::ShapeColumn;

use crate::controls::Control;
use crate::input::bindings::Cmd;
use crate::input::keys::Key;
use crate::input::mode::{Mode, View};
use crate::input::tokens::{self, Did, TokenKey, Tokens};
use crate::overlays::Overlay;
use crate::overlays::shape::token_named;
use crate::pages::{Ctx, CtxMut, Outcome};
use crate::services::Services;
use crate::widgets::tokens::{reached, said_after};

use super::generators::type_gen;

/// A Gen page setting's text while the cursor is in its tokens.
#[derive(Debug, Clone)]
pub struct GenTokens {
    /// The setting: its generator, by its phrase and its place in the phrase's list, and which.
    pub control: Control,
    pub tokens: Tokens,
    /// The token the cursor is on, by its place.
    pub at: usize,
}

/// The same setting, cursor and text, as the page compares its state.
impl PartialEq for GenTokens {
    fn eq(&self, other: &Self) -> bool {
        self.control == other.control
            && self.at == other.at
            && self.tokens.field.text() == other.tokens.field.text()
    }
}

impl GenTokens {
    /// The field of a Gen page setting under the cursor, `control`, where its text is in a
    /// grammar, on the first token.
    pub(crate) fn open(services: &Services, view: View, control: Control) -> Option<Self> {
        let field = field_of(services, view, control)?;
        Some(Self {
            control,
            tokens: Tokens::new(field),
            at: 0,
        })
    }

    /// After a key: what reads written to the generator and heard, and the token named.
    fn did(&mut self, did: Did, ctx: &mut CtxMut) -> Outcome {
        match did {
            Did::Out => {
                ctx.services.status.clear();
                return Outcome::Close;
            }
            // A field of fixed length has no room between its tokens: `i` types it all, as on
            // the cell.
            Did::TypeAll => return Outcome::Insert(crate::app::Entry::default()),
            Did::Refused(why) => {
                ctx.services.status = why;
                return Outcome::Done;
            }
            Did::Moved => {}
            Did::Changed => self.write(ctx.services, ctx.modes.view),
        }
        self.say(ctx.services, ctx.modes.view);
        Outcome::Done
    }

    /// The field's text into the setting where it reads, and back as the setting shows it once
    /// it's no longer being typed.
    fn write(&mut self, services: &mut Services, view: View) {
        let field = &self.tokens.field;
        if field.bad().is_some() {
            return;
        }
        let text = (field.grammar().to_setting)(field.text());
        type_gen(services, view, self.control, &text);
        if self.tokens.field.inserting().is_none()
            && let Some(shown) = field_of(services, view, self.control)
        {
            self.tokens.field.set(shown.text());
        }
    }

    /// The status line names the token, or says why the text doesn't read.
    fn say(&self, services: &mut Services, view: View) {
        let field = &self.tokens.field;
        services.status = match (field.bad(), field.inserting()) {
            (Some(bad), _) => format!(
                "{}; the column plays the last text that read. Esc puts it back",
                bad.why
            ),
            (None, Some(_)) => String::new(),
            (None, None) => {
                let after = match (self.control, self.live(services, view)) {
                    (Control::Gen { param, .. }, Some(live)) => said_after(
                        param,
                        field,
                        self.at,
                        &live.generator.shape,
                        &live.context(),
                    ),
                    _ => String::new(),
                };
                format!("{}{after}", token_named(field, self.at).0)
            }
        };
    }

    /// What's around the field, for what `K` and `Tab` offer in it: the generator's column, its
    /// From and To, and on a `Tonnetz`'s moves the chord each would reach in the cursor's place.
    fn around(
        &self,
        services: &Services,
        view: View,
    ) -> Option<(ShapeColumn, u8, u8, Vec<String>)> {
        let live = self.live(services, view)?;
        let shape = live.generator.shape;
        let Control::Gen { param, .. } = self.control else {
            return None;
        };
        let reached = reached(param, &shape, &live.context(), self.at);
        Some((live.generator.column, shape.from, shape.to, reached))
    }

    fn live(
        &self,
        services: &Services,
        view: View,
    ) -> Option<crate::services::generators::LiveGen> {
        let Control::Gen {
            phrase, generator, ..
        } = self.control
        else {
            return None;
        };
        services.live_generator(services.cursor.track, phrase, generator, view)
    }
}

/// A Gen page setting's text as a field of tokens, where it's in a grammar, as its generator has
/// it now.
fn field_of(services: &Services, view: View, control: Control) -> Option<Field> {
    let Control::Gen {
        phrase,
        generator,
        param,
    } = control
    else {
        return None;
    };
    let live = services.live_generator(services.cursor.track, phrase, generator, view)?;
    let context = live.context();
    let grammar = param.grammar(context.column)?.grammar();
    let text = (grammar.from_setting)(&param.shown(&live.generator.shape, &context));
    Some(Field::new(grammar, &text, context.column).in_kit(&live.names))
}

impl Overlay for GenTokens {
    const MODE: Mode = Mode::Tokens;

    const KEYS: &'static [(&'static str, Cmd, &'static str)] = tokens::KEYS;

    fn key(&mut self, cmd: Cmd, ctx: &mut CtxMut) -> Outcome {
        let Cmd::InTokens(key) = cmd else {
            return Outcome::Passed;
        };
        let did = match key {
            TokenKey::Offer => {
                let Some((column, from, to, reached)) = self.around(ctx.services, ctx.modes.view)
                else {
                    return Outcome::Done;
                };
                let around = Around {
                    column,
                    from,
                    to,
                    tracks: &[],
                    reached: &reached,
                };
                self.tokens.offer(self.at, &around)
            }
            key => self.tokens.key(key, &mut self.at),
        };
        self.did(did, ctx)
    }

    fn takes_text(&self) -> bool {
        self.tokens.completes()
    }

    fn types_now(&self, key: Key) -> bool {
        match key.code {
            KeyCode::Char(c) => !key.command && self.tokens.types(c),
            _ => false,
        }
    }

    /// What's typed, a token typed in place, a count or a key of the list `K` or `Tab` opened;
    /// `Tab` and `Shift-Tab` play otherwise, and any other key is passed back, to leave the field
    /// and do what it does on the page.
    fn text(&mut self, key: Key, ctx: &mut CtxMut) -> Outcome {
        let typing = self.tokens.completes();
        match key.code {
            KeyCode::Tab if !typing => return Outcome::Do(Cmd::PauseOrResume),
            KeyCode::BackTab if !typing => return Outcome::Do(Cmd::PlayFromStart),
            KeyCode::Char(c) if !typing && !self.tokens.types(c) && !c.is_ascii_digit() => {
                return Outcome::Passed;
            }
            _ if key.command => return Outcome::Done,
            _ => {}
        }
        let Some((column, from, to, reached)) = self.around(ctx.services, ctx.modes.view) else {
            return Outcome::Done;
        };
        let around = Around {
            column,
            from,
            to,
            tracks: &[],
            reached: &reached,
        };
        let did = self.tokens.text(key.code, &mut self.at, &around);
        self.did(did, ctx)
    }

    /// Drawn by the Gen page, over its cell.
    fn draw(&mut self, _: &mut ratatui::Frame, _: ratatui::layout::Rect, _: &Ctx) {}
}

impl crate::app::App {
    /// Give the Gen page's field of tokens a key. One it passes back leaves the field and is the
    /// page's, as a key the shape panel's tokens don't bind is the panel's.
    pub(crate) fn gen_tokens_key(&mut self, key: Key, now: std::time::Instant) {
        let Some(mut tokens) = self.track_page.tokens.take() else {
            self.modes.mode = Mode::Normal;
            return;
        };
        let outcome = crate::overlays::give(
            &mut tokens,
            &self.table,
            &mut self.grammar,
            CtxMut {
                services: &mut self.services,
                modes: &self.modes,
                wheel: &mut self.wheel,
            },
            key,
            now,
        );
        self.track_page.tokens = Some(tokens);
        if matches!(outcome, Outcome::Passed) {
            self.leave_gen_tokens();
            self.handle_key(key, now);
            return;
        }
        self.settled(outcome, false);
        if self.modes.mode != Mode::Tokens {
            self.track_page.tokens = None;
        }
    }

    /// Out of the Gen page's field, back on its cell.
    pub(crate) fn leave_gen_tokens(&mut self) {
        self.track_page.tokens = None;
        self.modes.mode = Mode::Normal;
    }
}
