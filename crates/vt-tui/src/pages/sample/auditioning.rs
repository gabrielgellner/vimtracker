//! Hearing what's on the sample page: `;` plays the slice under the cursor, `Space c` plays from
//! the cursor to the entry's end, and the keys mode plays slices from two rows of keys, as
//! tracker-ce's Beat Slice pads do. Each goes through the cursor's track's chain on a preview
//! voice, as the explorer plays a file, from the frames the pool already holds, so nothing is
//! loaded or copied.
//!
//! A key held for [`HOLD_TIME`](crate::app::HOLD_TIME) or more plays only while it's down, and a
//! tap plays to the end. One plays at a time, as a track plays one note at a time, so the next
//! fades out the last. Where each sound is shows as a playhead over the waveform, for these and for
//! the track's own notes of the entry shown.
//!
//! What plays is the page's, through the [`Services`]: each gives back the [`Momentary`] it
//! started, which the page's key hands to `App` in [`Outcome::Hold`](crate::pages::Outcome::Hold)
//! to end when the key is let go. The keys mode itself is `App`'s, as a mode of its keys.

use vt_engine::link::Command;
use vt_model::playable::{Playable, Span};

use crate::app::{App, Momentary};
use crate::input::keys::Key;
use crate::input::mode::Mode;
use crate::pages::sample::{SamplePage, Shown};
use crate::services::Services;

/// The keys that play slices in the keys mode, by row: the home row from `a`, then the row above
/// from `q`, eight each, so the home row plays `00` to `07` and the row above `08` to `0F`. That's
/// sixteen, as an MPC's bank of pads or an Elektron's trig keys, and a hex digit's worth of slices.
pub(crate) const PAD_ROWS: [&str; 2] = ["asdfghjk", "qwertyui"];

/// How many slices the keys play at once, as one page of them.
pub(crate) const PADS: usize = 16;

/// The pad a key is, counted from the home row's first, if it's one.
pub(crate) fn pad_key(c: char) -> Option<usize> {
    PAD_ROWS.iter().enumerate().find_map(|(row, keys)| {
        let at = keys.chars().position(|key| key == c)?;
        Some(row * keys.len() + at)
    })
}

impl SamplePage {
    /// `;`: the slice under the cursor, as it plays in the song, with its own gain, pan, tune,
    /// direction and fades. Held, until it's let go.
    pub(crate) fn audition_slice(
        &mut self,
        shown: &Shown,
        services: &mut Services,
    ) -> Option<Momentary> {
        let (number, _) = shown.slice_at(self.cursor);
        self.play_slice(shown, number, services)
    }

    /// `Space c`: from the cursor to the entry's end, or on to the file's from past it, forward,
    /// at the entry's gain and tune. Held, until it's let go.
    pub(crate) fn audition_from_cursor(
        &mut self,
        shown: &Shown,
        services: &mut Services,
    ) -> Option<Momentary> {
        let (playable, rates) = playable(shown, services)?;
        let whole = playable.whole;
        let from = rates.loaded_frame(self.cursor).min(whole.frames);
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
        self.hear(shown, &playable, span, services)
    }

    /// `;` while selecting: the frames selected, forward, at the entry's gain and tune, with a
    /// declick at each end rather than the entry's fades, as they're cut out of it. Held, until
    /// it's let go.
    pub(crate) fn audition_frames(
        &mut self,
        shown: &Shown,
        frames: std::ops::Range<u64>,
        services: &mut Services,
    ) -> Option<Momentary> {
        let (playable, rates) = playable(shown, services)?;
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
        self.hear(shown, &playable, span, services)
    }

    /// Play one of the entry's slices, by its number, giving what it started if it went.
    fn play_slice(
        &mut self,
        shown: &Shown,
        number: usize,
        services: &mut Services,
    ) -> Option<Momentary> {
        let (playable, _) = playable(shown, services)?;
        let span = playable.spans[number.min(playable.spans.len() - 1)];
        self.hear(shown, &playable, span, services)
    }

    /// Play a span of the entry's frames through the cursor's track, held while its key is down.
    fn hear(
        &mut self,
        shown: &Shown,
        playable: &Playable,
        span: Span,
        services: &mut Services,
    ) -> Option<Momentary> {
        let track = services.cursor.track;
        let Some(frames) = playable.sound.loaded().map(std::sync::Arc::clone) else {
            services.status = format!("{} is still recording", shown.entry.title());
            return None;
        };
        if !services.handle.play_file(track, frames, Some(span)) {
            services.status =
                "Audio has stopped, so nothing plays. Space 9 L chooses a device".to_owned();
            return None;
        }
        self.heard = Some((shown.number, shown.analysis.hash));
        self.hearings += 1;
        // It plays through the track, so its mute and the solos silence it as they do the track's
        // notes. The page shows neither, so it says so rather than playing in silence.
        if let Some(why) = why_silent(services, track) {
            services.status = why;
        }
        Some(Momentary::SampleAudition {
            hearing: self.hearings,
        })
    }

    /// Esc on the sample page: let go of what it's playing.
    pub(crate) fn stop_audition(&self, services: &mut Services) {
        if self.heard.is_some() {
            services.handle.send(Command::StopFile);
        }
    }

    /// A pad: its slice plays, held while its key is down, and the cursor goes to it, as
    /// tracker-ce's Beat Slice pads select the slice they play.
    fn play_pad(
        &mut self,
        shown: &Shown,
        number: usize,
        services: &mut Services,
    ) -> Option<Momentary> {
        let count = shown.slice_count();
        if number >= count {
            services.status = match count {
                1 => format!("No slice {number:02X}: the sample is one slice, 00, until :slice"),
                _ => format!(
                    "No slice {number:02X}: the sample has {count}, 00 to {:02X}",
                    count - 1
                ),
            };
            return None;
        }
        let started = self.play_slice(shown, number, services)?;
        let start = shown.slice(number).start;
        self.go(start, shown.length());
        Some(started)
    }

    /// What the keys mode's keys play, and how to leave it, for under the status line while it's
    /// on. `h`, `j` and `k` are pads here, so it says that `Esc` gives them back.
    pub(crate) fn pads_hint(&self, shown: &Shown) -> String {
        let from = self.pads_from;
        let count = shown.slice_count();
        let last = (from + PADS).min(count).saturating_sub(1);
        let rows = PAD_ROWS.map(|row| row.chars().map(String::from).collect::<Vec<_>>().join(" "));
        if count <= PADS {
            format!(
                "{} play slices 00 to 07, {} 08 to 0F; Esc leaves, for h j k l to move again",
                rows[0], rows[1]
            )
        } else {
            format!(
                "{} {} play slices {from:02X} to {last:02X}; < > the next or last {PADS}, Esc \
                 leaves, for h j k l to move again",
                rows[0], rows[1]
            )
        }
    }
}

/// How the entry plays, worked out as the edit thread works it out for the engine, and the rates
/// its file was loaded at, or why it can't be played.
fn playable(
    shown: &Shown,
    services: &mut Services,
) -> Option<(Playable, vt_model::samples::Rates)> {
    let found = Playable::of(&shown.entry, &services.samples).zip(shown.frames.as_ref());
    match found {
        Some((playable, (_, rates))) => Some((playable, *rates)),
        None => {
            services.status = format!(
                "{} isn't loaded, so it can't be played",
                shown.entry.title()
            );
            None
        }
    }
}

/// Why a track plays what it's given in silence, if it does: it's muted, or another track is
/// soloed, as the engine has it.
fn why_silent(services: &Services, track: usize) -> Option<String> {
    let sounds = services
        .song()
        .sound_tracks()
        .min(services.live.levels.len());
    let live = services.live.levels.get(track)?;
    if live.mute {
        return Some(format!(
            "Track {:02} is muted, so it plays silently. Space m unmutes it",
            track + 1
        ));
    }
    let soloed = services.live.levels[..sounds]
        .iter()
        .position(|other| other.solo);
    match soloed {
        Some(other) if !live.solo => Some(format!(
            "Track {:02} is soloed, so Track {:02} plays silently. Space M clears the solos",
            other + 1,
            track + 1
        )),
        _ => None,
    }
}

impl App {
    /// A key in the sample page's keys mode. The pads play their slices, `<` and `>` go a page of
    /// them back or on, as they change octave on the track page's keyboard, `Esc` leaves, and
    /// every other key works on the page as it does, so `;` plays the slice the cursor is on.
    pub(crate) fn sample_keys_key(&mut self, key: Key, now: std::time::Instant) {
        use ratatui::crossterm::event::KeyCode;
        let plain = self.grammar.pending.is_empty() && !key.ctrl && !key.alt;
        let shown = self.sample_page_here();
        match (key.code, shown) {
            (KeyCode::Esc, _) if self.grammar.pending.is_empty() => {
                self.modes.mode = Mode::Normal;
                self.services.status.clear();
            }
            (KeyCode::Char(c @ ('<' | '>')), Ok(shown)) if plain => {
                self.services.status.clear();
                let from = self.sample_page.pads_from;
                let last = shown.slice_count().saturating_sub(1) / PADS * PADS;
                self.sample_page.pads_from = if c == '>' {
                    (from + PADS).min(last)
                } else {
                    from.saturating_sub(PADS)
                };
            }
            (KeyCode::Char(c), Ok(shown)) if plain && pad_key(c).is_some() => {
                self.modes.count = None;
                let number = self.sample_page.pads_from + pad_key(c).unwrap_or(0);
                if let Some(started) = self
                    .sample_page
                    .play_pad(&shown, number, &mut self.services)
                {
                    self.held.started = Some(started);
                }
            }
            _ => {
                let view = self.modes.view;
                self.command_key_normal(key, now);
                // The keys only play on the page they started on.
                if self.modes.mode == Mode::Keys && self.modes.view != view {
                    self.modes.mode = Mode::Normal;
                }
            }
        }
    }

    /// Where the voices playing the entry shown are, in its file's frames, for the playheads: the
    /// track's own notes of it, any of its notes on the preview voices, and what the page itself
    /// played.
    pub fn sample_playheads(&self, shown: &Shown) -> Vec<u64> {
        self.sample_page
            .playheads(shown, &self.services.reported.playheads)
    }
}
