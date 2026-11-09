//! Live recording: armed with `Space r` while the song plays, notes played in Insert go in where
//! the cursor's track is playing, each as long as its key was held, in its gate cell.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::Transport;
use vt_model::action::Action;
use vt_model::constants::{TICKS_PER_QUARTER, TICKS_PER_STEP};
use vt_model::step::{Gate, GateKind, MAX_PULSES, Note};

use crate::app::App;
use crate::input::keys::Key;
use crate::input::mode::View;

/// The longest a recorded note can be, in steps: the most pulses a row can last.
const LONGEST_NOTE: f64 = MAX_PULSES as f64;

/// The longest a recorded note's length can be, just under all of its last pulse, so it ends
/// where it was let go rather than holding on to the next note as `FF` does.
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
    /// Where the playhead was last seen, to notice it jumping back.
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
    /// When its key went down, and when its row starts, in steps of the running count the
    /// engine reports.
    pressed: f64,
    starts: f64,
    /// How long it can last: until its row comes round again, or the most pulses a row can last.
    longest: f64,
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
                last: self.heard_steps(),
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
        let now = if self.transport == Transport::Playing {
            self.heard_steps()
        } else {
            recording.last
        };
        for take in std::mem::take(&mut recording.takes) {
            let held = take.held.unwrap_or(now - take.pressed);
            self.write_take(&take, Some(held), &mut recording.depth);
        }
    }

    /// Record a note on the row the cursor's track is playing, or the next if it's nearer.
    pub(crate) fn record_note(&mut self, key: Key, note: u8) {
        let track = self.cursor.track;
        let now = self.heard_steps();
        let Some(row) = self.row_to_record(track, now) else {
            self.status = "This track isn't playing".to_owned();
            return;
        };
        let velocity = self.note_velocity;
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
            pressed: now,
            starts: row.starts,
            longest: row.longest,
            written: false,
            held: None,
        });
        self.follow_recording();
    }

    /// A key holding a recorded note was let go, so its length is known.
    pub(crate) fn end_take(&mut self, key: KeyCode) {
        let now = self.heard_steps();
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
        let now = self.heard_steps();
        let Some(mut recording) = self.recording.take() else {
            return;
        };
        // A jump back, as playing from the start or a new section makes, ends every note where
        // it was.
        let jumped = now < recording.last - 0.5;
        let last = recording.last;
        recording.last = now;
        let mut kept = Vec::new();
        for mut take in std::mem::take(&mut recording.takes) {
            let ended = jumped || now >= take.starts + take.longest;
            if ended && take.held.is_none() {
                let at = if jumped { last } else { now };
                take.held = Some(at - take.pressed);
            }
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

    /// Where the song is as it's heard, in steps of the running count the engine reports: its
    /// last report, carried on by the clock since, less the output's latency.
    fn heard_steps(&self) -> f64 {
        let elapsed = Instant::now()
            .saturating_duration_since(self.report_at)
            .min(MOST_ELAPSED);
        let latency = Duration::from_micros(self.audio.latency_us);
        let steps_per_quarter = (TICKS_PER_QUARTER / TICKS_PER_STEP) as f64;
        let steps_per_second = f64::from(self.song().millibpm) / 60_000.0 * steps_per_quarter;
        let since = elapsed.as_secs_f64() - latency.as_secs_f64();
        self.playhead as f64 + f64::from(self.progress) + since * steps_per_second
    }

    /// The row a track is playing at this point, or the next one if its start is nearer. Each
    /// track loops at its own length, so it's found in the track's own chain.
    fn row_to_record(&self, track: usize, now: f64) -> Option<Row> {
        let jam = self.jam.get(track)?;
        let chain = jam.chain?;
        let owner = self.song().tracks.get(track)?;
        let length = owner.chain_steps(chain);
        if length == 0 {
            return None;
        }
        let into = (now - jam.started as f64).rem_euclid(length as f64);
        let position = owner.chain_position(chain, into as usize)?;
        let start = (into as usize).saturating_sub(position.pulse) as f64;
        let end = start + owner.step_at(position)?.pulses() as f64;
        let (position, start) = if end - into < into - start {
            (owner.chain_position(chain, end as usize)?, end)
        } else {
            (position, start)
        };
        let phrase = owner.phrases.get(&position.phrase)?;
        Some(Row {
            phrase: position.phrase,
            step: phrase.start() + position.step,
            starts: now + start - into,
            longest: (length as f64).min(LONGEST_NOTE),
        })
    }

    /// Write a recorded note over its row's note, velocity and gate cell, with its length in the
    /// cell. Until it's let go, it holds on the pulses the row had. Other columns and rows are left
    /// alone, and the pass is one undo step.
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
        // The note is one note, held, so its gate is a `SGL`. Until it's let go, it holds on the
        // row's pulses until something ends it, and then the row lasts as long as it was held.
        let (pulses, length) = match held {
            Some(held) => recorded_length(held.min(take.longest)),
            None => (step.gate.map_or(1, |gate| gate.pulses), u8::MAX),
        };
        step.gate = Some(Gate {
            kind: GateKind::Single,
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

/// The pulses and length a note held this many steps is written with. The row lasts as long as it
/// was held, to the nearest step, as a press goes on the nearest row, so a key let go just past a
/// step doesn't add one. The note lasts what was held of the row's last pulse, up to just under
/// all of it.
fn recorded_length(held: f64) -> (u8, u8) {
    let pulses = held.round().clamp(1.0, LONGEST_NOTE);
    let last = (held - (pulses - 1.0)).clamp(0.0, 1.0);
    let length = (last * 255.0).round().min(f64::from(LONGEST_LENGTH));
    (pulses as u8, length as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hold_gives_its_row_the_nearest_whole_steps_and_the_rest_as_its_length() {
        assert_eq!(recorded_length(0.5), (1, 0x80));
        assert_eq!(recorded_length(0.0), (1, 0x00), "00 still sounds, at 1%");
        // Let go just past a step, the row doesn't take another.
        assert_eq!(recorded_length(1.3), (1, LONGEST_LENGTH));
        assert_eq!(recorded_length(1.6), (2, 0x99));
        assert_eq!(recorded_length(2.0), (2, LONGEST_LENGTH));
        assert_eq!(recorded_length(12.0), (MAX_PULSES, LONGEST_LENGTH));
    }
}
