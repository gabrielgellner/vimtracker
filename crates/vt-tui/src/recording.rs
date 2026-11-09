//! Live recording: armed with `Space r` while the song plays, notes played in Insert go in where
//! the cursor's track is playing, each with how long its key was held in its gate cell. Rows are
//! where the track's groove lands them, so a note goes on the row it was played nearest to as it's
//! heard, with how far from the row it was played as the row's nudge.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::{TrackPlace, Transport};
use vt_model::action::Action;
use vt_model::constants::{NUDGE_TICKS, TICKS_PER_QUARTER, TICKS_PER_STEP};
use vt_model::groove::Groove;
use vt_model::step::{Gate, GateKind, MAX_NUDGE, Note};

use crate::app::App;
use crate::input::keys::Key;
use crate::input::mode::View;

/// The longest a recorded note's length can be while it's let go within its row, just under all
/// of its last pulse, so it ends there rather than holding on to the next note as `FF` does.
const LONGEST_LENGTH: u8 = 0xFE;

/// The furthest past the engine's last report the playhead is worked out from the clock, so a
/// screen that stalls doesn't run on.
const MOST_ELAPSED: Duration = Duration::from_millis(250);

/// A pass of live recording, from arming to disarming.
#[derive(Debug, Default)]
pub struct Recording {
    /// The undo depth from before the pass's first note, so the whole pass undoes at once.
    depth: Option<usize>,
    /// Notes still held, or waiting for their row to start.
    takes: Vec<Take>,
    /// Where the song's playhead was last seen, to notice it jumping back.
    last: f64,
}

/// A note played while recording.
#[derive(Debug, Clone, Copy)]
struct Take {
    key: KeyCode,
    track: usize,
    phrase: u32,
    /// The stored step it goes on.
    step: usize,
    note: u8,
    velocity: u8,
    /// Whether the slide's key was down as it was played, so it's a `HLD`.
    slide: bool,
    /// When its key went down, and when its row starts, in steps of its track's running count,
    /// which the engine reports, and where its track was last seen.
    pressed: f64,
    starts: f64,
    seen: f64,
    /// How long it can last: until its row comes round again.
    longest: f64,
    /// How far the groove moves the row's last pulse from its first, and how long it makes that
    /// pulse, in steps, so its length is a share of the pulse as it's heard.
    moved: f64,
    last_pulse: f64,
    written: bool,
    /// How long its key was held, once it's let go or the note has run as long as it can.
    held: Option<f64>,
}

/// The row a note played now goes on.
struct Row {
    phrase: u32,
    step: usize,
    starts: f64,
    longest: f64,
    moved: f64,
    last_pulse: f64,
}

impl App {
    /// Whether live recording is armed.
    pub fn recording(&self) -> bool {
        self.recording.is_some()
    }

    /// Arm live recording, which only works while playing in the phrase view, or disarm it.
    pub(crate) fn toggle_recording(&mut self) {
        if self.recording.is_some() {
            self.stop_recording();
            self.status = "Recording off".to_owned();
        } else if self.view != View::Phrase {
            self.status = "Recording arms in the phrase view".to_owned();
        } else if self.transport != Transport::Playing {
            self.status = "Recording arms while playing".to_owned();
        } else {
            self.recording = Some(Recording {
                last: self.heard_song(),
                ..Recording::default()
            });
            self.status = "Recording: notes played in Insert go in at the playhead".to_owned();
        }
    }

    /// Disarm, writing every note still held as long as it's been held so far.
    pub(crate) fn stop_recording(&mut self) {
        let Some(mut recording) = self.recording.take() else {
            return;
        };
        let playing = self.transport == Transport::Playing;
        for take in std::mem::take(&mut recording.takes) {
            let now = if playing {
                self.heard_steps(take.track)
            } else {
                take.seen
            };
            let held = take.held.unwrap_or(now - take.pressed);
            self.write_take(&take, Some(held), &mut recording.depth);
        }
    }

    /// Record a note on the row the cursor's track is playing, or the next if it's nearer.
    pub(crate) fn record_note(&mut self, key: Key, note: u8) {
        let track = self.cursor.track;
        let now = self.heard_steps(track);
        let Some(row) = self.row_to_record(track, now) else {
            self.status = "This track isn't playing".to_owned();
            return;
        };
        let velocity = self.flagged_velocity(self.note_velocity);
        let slide = self.flagged_slide();
        self.use_flags();
        let Some(recording) = self.recording.as_mut() else {
            return;
        };
        // The last key down wins its row, as in Insert.
        recording
            .takes
            .retain(|t| (t.track, t.phrase, t.step) != (track, row.phrase, row.step));
        recording.takes.push(Take {
            key: key.physical(),
            track,
            phrase: row.phrase,
            step: row.step,
            note,
            velocity,
            slide,
            pressed: now,
            starts: row.starts,
            seen: now,
            longest: row.longest,
            moved: row.moved,
            last_pulse: row.last_pulse,
            written: false,
            held: None,
        });
        self.follow_recording();
    }

    /// A key holding a recorded note was let go, so its length is known.
    pub(crate) fn end_take(&mut self, key: KeyCode) {
        let Some(track) = self.recording.as_ref().and_then(|recording| {
            let take = recording.takes.iter().find(|t| t.key == key)?;
            Some(take.track)
        }) else {
            return;
        };
        let now = self.heard_steps(track);
        let Some(recording) = self.recording.as_mut() else {
            return;
        };
        if let Some(take) = recording.takes.iter_mut().find(|t| t.key == key) {
            take.held.get_or_insert(now - take.pressed);
        }
        self.follow_recording();
    }

    /// Write what's due as the song plays: a note once its row has started, so it isn't played a
    /// second time as it's heard, and its length once it's let go, or once its row comes round
    /// again. Stopping, or leaving the phrase view, disarms.
    pub(crate) fn follow_recording(&mut self) {
        if self.recording.is_none() {
            return;
        }
        if self.transport != Transport::Playing || self.view != View::Phrase {
            self.stop_recording();
            return;
        }
        let Some(mut recording) = self.recording.take() else {
            return;
        };
        // A jump back, as playing from the start or a new section makes, ends every note where
        // it was.
        let song_now = self.heard_song();
        let jumped = song_now < recording.last - 0.5;
        recording.last = song_now;
        let mut kept = Vec::new();
        for mut take in std::mem::take(&mut recording.takes) {
            let now = self.heard_steps(take.track);
            let ended = jumped || now >= take.starts + take.longest;
            if ended && take.held.is_none() {
                let at = if jumped { take.seen } else { now };
                take.held = Some(at - take.pressed);
            }
            take.seen = now;
            if !ended && !take.written && now < take.starts {
                kept.push(take);
                continue;
            }
            match take.held {
                Some(held) => self.write_take(&take, Some(held), &mut recording.depth),
                None if !take.written => {
                    self.write_take(&take, None, &mut recording.depth);
                    take.written = true;
                    kept.push(take);
                }
                None => kept.push(take),
            }
        }
        recording.takes = kept;
        self.recording = Some(recording);
    }

    /// Where the song is as it's heard, in its own steps: the engine's last report, carried on by
    /// the clock since, less the output's latency.
    fn heard_song(&self) -> f64 {
        let place = TrackPlace {
            count: self.playhead as i64,
            progress: self.progress,
            ticks: TICKS_PER_STEP as u32,
            skipped: false,
            accumulated: None,
        };
        self.carried_on(place)
    }

    /// Where a track is as it's heard, in steps of its own running count, which the engine
    /// reports, as long as its speed makes them.
    fn heard_steps(&self, track: usize) -> f64 {
        self.carried_on(self.places.get(track).copied().unwrap_or_default())
    }

    /// Where a place the engine last reported has got to by now, less the output's latency.
    fn carried_on(&self, place: TrackPlace) -> f64 {
        let elapsed = Instant::now()
            .saturating_duration_since(self.report_at)
            .min(MOST_ELAPSED);
        let latency = Duration::from_micros(self.audio.latency_us);
        let steps_per_quarter = TICKS_PER_QUARTER as f64 / f64::from(place.ticks.max(1));
        let steps_per_second = f64::from(self.song().millibpm) / 60_000.0 * steps_per_quarter;
        let since = elapsed.as_secs_f64() - latency.as_secs_f64();
        place.count as f64 + f64::from(place.progress) + since * steps_per_second
    }

    /// The row a track is playing at this point, or the next one if its start is nearer, with
    /// each start where the track's groove lands it. Each track loops at its own length, so it's
    /// found in the track's own chain, and the groove counts the track's own steps, as playing
    /// does. Everything here is in the track's steps, which its pushes and nudges are shares of.
    fn row_to_record(&self, track: usize, now: f64) -> Option<Row> {
        let jam = self.jam.get(track)?;
        let chain = jam.chain?;
        let song = self.song();
        let owner = song.tracks.get(track)?;
        let length = owner.chain_steps(chain);
        if length == 0 {
            return None;
        }
        let groove = song.groove(song.track_groove(track));
        let into = (now - jam.started as f64).rem_euclid(length as f64);
        let position = owner.chain_position(chain, into as usize)?;
        let start = (into as usize).saturating_sub(position.pulse) as f64;
        let end = start + owner.step_at(position)?.pulses() as f64;
        // Where a place in the chain lands as it's heard, on the running count.
        let heard = |at: f64| {
            let count = now - into + at;
            count + pushed(&groove, count)
        };
        let (position, start) = if (heard(end) - now).abs() < (now - heard(start)).abs() {
            (owner.chain_position(chain, end as usize)?, end)
        } else {
            (position, start)
        };
        let phrase = owner.phrases.get(&position.phrase)?;
        let pulses = owner.step_at(position)?.pulses().max(1) as f64;
        let first = now - into + start;
        let last = first + pulses - 1.0;
        let steps = TICKS_PER_STEP as f64;
        Some(Row {
            phrase: position.phrase,
            step: phrase.start() + position.step,
            starts: heard(start),
            longest: length as f64,
            moved: pushed(&groove, last) - pushed(&groove, first),
            last_pulse: groove.length(running(last)) as f64 / steps,
        })
    }

    /// Write a recorded note over its row's note, velocity, nudge and gate cell, with how far off
    /// the row it was played as the nudge and its length in the cell. The row keeps its pulses, so recording never changes a loop's length, and until it's
    /// let go the note holds on. Other columns and rows are left alone, and the pass is one undo
    /// step.
    fn write_take(&mut self, take: &Take, held: Option<f64>, depth: &mut Option<usize>) {
        let Some(phrase) = self
            .song()
            .tracks
            .get(take.track)
            .and_then(|track| track.phrases.get(&take.phrase))
            .map(Arc::clone)
        else {
            return;
        };
        let Some(mut step) = phrase.steps().get(take.step).cloned() else {
            return;
        };
        step.note = Note::On(take.note);
        step.velocity = Some(take.velocity);
        step.nudge = recorded_nudge(take.pressed - take.starts);
        // The note is one note, held, so its gate is a `SGL`, on the pulses the row had, or a
        // `HLD` if it was played with the slide's key down. Until it's let go, it holds on until
        // something ends it.
        let pulses = step.gate.map_or(1, |gate| gate.pulses);
        let length = held.map_or(u8::MAX, |held| {
            let held = held.min(take.longest) - take.moved;
            recorded_length(held, pulses, take.last_pulse)
        });
        step.gate = Some(Gate {
            kind: if take.slide {
                GateKind::Hold
            } else {
                GateKind::Single
            },
            pulses,
            length: Some(length),
        });
        let before = Arc::clone(self.song());
        let was = self.history.undo_depth();
        self.apply(&Action::SetStep {
            track: take.track,
            phrase: take.phrase,
            step: take.step,
            value: step,
        });
        if Arc::ptr_eq(&before, self.song()) {
            return;
        }
        match *depth {
            Some(start) if start <= was => self.history.squash_to(start),
            _ => *depth = Some(was),
        }
    }
}

/// The step of the running count a place falls in.
fn running(count: f64) -> usize {
    count.round().max(0.0) as usize
}

/// How far the groove moves the step at a place on the running count, in steps.
fn pushed(groove: &Groove, count: f64) -> f64 {
    f64::from(groove.push(running(count))) / TICKS_PER_STEP as f64
}

/// The nudge a note played this many steps from where its row lands is written with, to the
/// nearest 24th of a step and no further than a nudge goes.
fn recorded_nudge(off: f64) -> i8 {
    let most = f64::from(MAX_NUDGE);
    (off * (TICKS_PER_STEP / NUDGE_TICKS) as f64)
        .round()
        .clamp(-most, most) as i8
}

/// The length a note held this many steps is written with on a row of so many pulses, which stay
/// as they are: what was held of the row's last pulse, as long as the groove makes it, up to just
/// under all of it, or `FF` if it was held past the row's end, so it rings on until the next note
/// or something else ends it, as a held note does. One let go before the row's last pulse lasts
/// into it as little as the cell can say, `00`.
fn recorded_length(held: f64, pulses: u8, last_pulse: f64) -> u8 {
    let last = (held - f64::from(pulses.max(1) - 1)) / last_pulse.max(f64::EPSILON);
    if last > 1.0 {
        return u8::MAX;
    }
    (last.max(0.0) * 255.0)
        .round()
        .min(f64::from(LONGEST_LENGTH)) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hold_is_what_was_held_of_the_rows_last_pulse_and_past_it_holds_on() {
        assert_eq!(recorded_length(0.5, 1, 1.0), 0x80);
        assert_eq!(recorded_length(0.0, 1, 1.0), 0x00, "00 still sounds, at 1%");
        assert_eq!(
            recorded_length(1.0, 1, 1.0),
            LONGEST_LENGTH,
            "let go at the row's end"
        );
        assert_eq!(
            recorded_length(1.3, 1, 1.0),
            0xFF,
            "held past it, it rings on"
        );
        // On a row of several pulses, it's the share of the last.
        assert_eq!(recorded_length(3.5, 4, 1.0), 0x80);
        assert_eq!(
            recorded_length(0.25, 4, 1.0),
            0x00,
            "a tap lasts into the last pulse"
        );
        assert_eq!(recorded_length(4.5, 4, 1.0), 0xFF);
    }

    #[test]
    fn a_note_played_off_its_row_is_nudged_to_the_nearest_24th() {
        assert_eq!(recorded_nudge(0.0), 0);
        assert_eq!(recorded_nudge(0.1), 2);
        assert_eq!(recorded_nudge(-0.13), -3);
        assert_eq!(
            recorded_nudge(0.49),
            MAX_NUDGE,
            "half a step is as far as it goes"
        );
        assert_eq!(recorded_nudge(-0.5), -MAX_NUDGE);
    }

    #[test]
    fn a_hold_is_a_share_of_the_pulse_as_the_groove_makes_it() {
        // Swing at 66% makes the first of a pair 1.32 steps long, so half of it is 0.66.
        assert_eq!(recorded_length(0.66, 1, 1.32), 0x80);
        assert_eq!(recorded_length(0.34, 1, 0.68), 0x80);
    }
}
