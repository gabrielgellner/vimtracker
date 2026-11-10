//! Hearing what's on the sample page: `;` plays the slice under the cursor, `Space c` plays from
//! the cursor to the entry's end, and the keys mode plays slices from two rows of keys, as
//! tracker-ce's Beat Slice pads do. Each goes through the cursor's track's chain on a preview
//! voice, as the explorer plays a file, from the frames the pool already holds, so nothing is
//! loaded or copied.
//!
//! As decision 53 has it, a key held for [`HOLD_TIME`](crate::app::HOLD_TIME) or more plays only
//! while it's down, and a tap plays to the end. One plays at a time, as a track plays one note at a
//! time, so the next fades out the last. Where each sound is shows as a playhead over the waveform,
//! for these and for the track's own notes of the entry shown.

use vt_engine::link::Command;
use vt_model::playable::{Playable, Span};

use crate::app::{App, Momentary};
use crate::input::keys::Key;
use crate::input::mode::Mode;
use crate::sample_page::Shown;

/// The keys that play slices in the keys mode, by row: the home row from `a`, then the row above
/// from `q`, eight each, so the home row plays `00` to `07` and the row above `08` to `0F`. That's
/// sixteen, as an MPC's bank of pads or an Elektron's trig keys, and a hex digit's worth of slices.
pub const PAD_ROWS: [&str; 2] = ["asdfghjk", "qwertyui"];

/// How many slices the keys play at once, as one page of them.
pub const PADS: usize = 16;

/// The pad a key is, counted from the home row's first, if it's one.
pub fn pad_key(c: char) -> Option<usize> {
    PAD_ROWS.iter().enumerate().find_map(|(row, keys)| {
        let at = keys.chars().position(|key| key == c)?;
        Some(row * keys.len() + at)
    })
}

impl App {
    /// `;`: the slice under the cursor, as it plays in the song, with its own gain, pan, tune,
    /// direction and fades. Held, until it's let go.
    pub(crate) fn audition_slice(&mut self, shown: &Shown) {
        let (number, _) = shown.slice_at(self.sample_page.cursor);
        self.play_slice(shown, number);
    }

    /// `Space c`: from the cursor to the entry's end, or on to the file's from past it, forward,
    /// at the entry's gain and tune. Held, until it's let go.
    pub(crate) fn audition_from_cursor(&mut self, shown: &Shown) {
        let Some((playable, rates)) = self.playable(shown) else {
            return;
        };
        let whole = playable.whole;
        let from = rates
            .loaded_frame(self.sample_page.cursor)
            .min(whole.frames);
        let end = if from < whole.end {
            whole.end
        } else {
            whole.frames
        };
        let span = Span {
            start: from,
            end,
            loop_: None,
            reverse: false,
            // Cut into, it starts with a declick rather than the entry's fade in.
            fade_in: 0,
            pan: [1.0, 1.0],
            ..whole
        };
        self.hear(shown, &playable, span);
    }

    /// `;` while selecting: the frames selected, forward, at the entry's gain and tune, with a
    /// declick at each end rather than the entry's fades, as they're cut out of it. Held, until
    /// it's let go.
    pub(crate) fn audition_frames(&mut self, shown: &Shown, frames: std::ops::Range<u64>) {
        let Some((playable, rates)) = self.playable(shown) else {
            return;
        };
        let whole = playable.whole;
        let start = rates.loaded_frame(frames.start).min(whole.frames);
        let end = rates.loaded_frame(frames.end).clamp(start, whole.frames);
        let span = Span {
            start,
            end,
            loop_: None,
            reverse: false,
            fade_in: 0,
            fade_out: 0,
            pan: [1.0, 1.0],
            ..whole
        };
        self.hear(shown, &playable, span);
    }

    /// Play one of the entry's slices, by its number, and say whether it went.
    fn play_slice(&mut self, shown: &Shown, number: usize) -> bool {
        let Some((playable, _)) = self.playable(shown) else {
            return false;
        };
        let span = playable.spans[number.min(playable.spans.len() - 1)];
        self.hear(shown, &playable, span)
    }

    /// How the entry plays, worked out as the edit thread works it out for the engine, and the
    /// rates its file was loaded at, or why it can't be played.
    fn playable(&mut self, shown: &Shown) -> Option<(Playable, vt_model::samples::Rates)> {
        let found = Playable::of(&shown.entry, &self.samples).zip(shown.frames.as_ref());
        match found {
            Some((playable, (_, rates))) => Some((playable, *rates)),
            None => {
                self.status = format!(
                    "{} isn't loaded, so it can't be played",
                    shown.entry.title()
                );
                None
            }
        }
    }

    /// Play a span of the entry's frames through the cursor's track, held while its key is down.
    fn hear(&mut self, shown: &Shown, playable: &Playable, span: Span) -> bool {
        let track = self.cursor.track;
        let frames = std::sync::Arc::clone(&playable.frames);
        if !self.handle.play_file(track, frames, Some(span)) {
            self.status =
                "Audio has stopped, so nothing plays. Space 7 chooses a device".to_owned();
            return false;
        }
        let page = &mut self.sample_page;
        page.heard = Some((shown.number, shown.analysis.hash));
        page.hearings += 1;
        self.started = Some(Momentary::SampleAudition {
            hearing: page.hearings,
        });
        true
    }

    /// Esc on the sample page: let go of what it's playing.
    pub(crate) fn stop_sample_audition(&mut self) {
        if self.sample_page.heard.is_some() {
            self.handle.send(Command::StopFile);
        }
    }

    /// `i` on the sample page: its keys mode, where two rows of keys play the slices.
    pub(crate) fn start_pads(&mut self, shown: &Shown) {
        self.mode = Mode::Keys;
        self.say_pads(shown);
    }

    /// What the keys play, on the status line.
    fn say_pads(&mut self, shown: &Shown) {
        let from = self.sample_page.pads_from;
        let count = shown.slice_count();
        let last = (from + PADS).min(count).saturating_sub(1);
        let rows = PAD_ROWS.map(|row| row.chars().map(String::from).collect::<Vec<_>>().join(" "));
        self.status = if count <= PADS {
            format!(
                "{} play slices 00 to 07, {} 08 to 0F; Esc leaves",
                rows[0], rows[1]
            )
        } else {
            format!(
                "{} {} play slices {from:02X} to {last:02X}; < > the next or last {PADS}, Esc leaves",
                rows[0], rows[1]
            )
        };
    }

    /// A key in the sample page's keys mode. The pads play their slices, `<` and `>` go a page of
    /// them back or on, as they change octave on the track page's keyboard, `Esc` leaves, and
    /// every other key works on the page as it does, so `;` plays the slice the cursor is on.
    pub(crate) fn sample_keys_key(&mut self, key: Key, now: std::time::Instant) {
        use ratatui::crossterm::event::KeyCode;
        let plain = self.pending.is_empty() && !key.ctrl && !key.alt;
        let shown = self.sample_page_here();
        match (key.code, shown) {
            (KeyCode::Esc, _) if self.pending.is_empty() => {
                self.mode = Mode::Normal;
                self.status.clear();
            }
            (KeyCode::Char(c @ ('<' | '>')), Ok(shown)) if plain => {
                let from = self.sample_page.pads_from;
                let last = shown.slice_count().saturating_sub(1) / PADS * PADS;
                self.sample_page.pads_from = if c == '>' {
                    (from + PADS).min(last)
                } else {
                    from.saturating_sub(PADS)
                };
                self.say_pads(&shown);
            }
            (KeyCode::Char(c), Ok(shown)) if plain && pad_key(c).is_some() => {
                self.count = None;
                let number = self.sample_page.pads_from + pad_key(c).unwrap_or(0);
                self.play_pad(&shown, number);
            }
            _ => {
                let view = self.view;
                self.command_key_normal(key, now);
                // The keys only play on the page they started on.
                if self.mode == Mode::Keys && self.view != view {
                    self.mode = Mode::Normal;
                }
            }
        }
    }

    /// A pad: its slice plays, held while its key is down, and the cursor goes to it, as
    /// tracker-ce's Beat Slice pads select the slice they play.
    fn play_pad(&mut self, shown: &Shown, number: usize) {
        let count = shown.slice_count();
        if number >= count {
            self.status = match count {
                1 => format!("No slice {number:02X}: the sample is one slice, 00, until :slice"),
                _ => format!(
                    "No slice {number:02X}: the sample has {count}, 00 to {:02X}",
                    count - 1
                ),
            };
            return;
        }
        if self.play_slice(shown, number) {
            let start = shown.slice(number).start;
            self.sample_page.go(start, shown.length());
        }
    }

    /// Where the voices playing the entry shown are, in its file's frames, for the playheads: the
    /// track's own notes of it, any of its notes on the preview voices, and what the page itself
    /// played.
    pub fn sample_playheads(&self, shown: &Shown) -> Vec<u64> {
        let Some((_, rates)) = &shown.frames else {
            return Vec::new();
        };
        let heard = self.sample_page.heard == Some((shown.number, shown.analysis.hash));
        self.playheads
            .iter()
            .flatten()
            .filter(|head| match head.entry {
                Some(number) => number == shown.number,
                None => heard,
            })
            .map(|head| rates.file_frame(head.frame))
            .collect()
    }
}
