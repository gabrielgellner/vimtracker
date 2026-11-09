//! Changing where the current track's sample starts, ends and loops, and how it plays, from
//! `:sample` and a word, until the sample page comes. Each change is to the bank entry the track's
//! `Sample` is on, so every track playing it hears it, and is one undo step.

use vt_model::action::Action;
use vt_model::bank::{BankEntry, Loop};
use vt_model::samples::Rates;

use crate::app::App;
use crate::banking::sample_text;
use crate::input::command::SampleChange;

/// A time in microseconds as frames at a rate, to the nearest.
fn frames(micros: u64, rate: u32) -> u64 {
    ((u128::from(micros) * u128::from(rate) + 500_000) / 1_000_000) as u64
}

/// Frames at a rate as seconds, as the status line shows a point.
fn seconds(frames: u64, rate: u32) -> String {
    format!("{:.3} s", frames as f64 / f64::from(rate.max(1)))
}

impl App {
    /// Change the sample the current track plays, as one undo step, and say what it is now.
    pub(crate) fn change_sample(&mut self, change: SampleChange) {
        let number = self
            .song()
            .tracks
            .get(self.cursor.track)
            .filter(|track| track.machine == vt_model::song::Machine::Sampler)
            .map_or(0, |track| track.sample());
        let Some(mut entry) = self.song().bank.get(number).cloned() else {
            self.status = format!(
                "Track {:02} plays no sample to change: K on Sample chooses one",
                self.cursor.track + 1
            );
            return;
        };
        let Some((rate, length)) = self.file_of(&entry) else {
            self.status = "Its file isn't loaded, so its length isn't known".to_owned();
            return;
        };
        let at = |micros: u64| frames(micros, rate).min(length);
        let points = &mut entry.edit.points;
        let play = &mut entry.edit.play;
        let said = match change {
            SampleChange::Start(micros) => {
                points.start = at(micros);
                if points.end.is_some_and(|end| end <= points.start) {
                    self.status = "The start would be at or after the end".to_owned();
                    return;
                }
                format!("starts at {}", seconds(points.start, rate))
            }
            SampleChange::End(micros) => {
                points.end = micros.map(at).filter(|end| *end < length);
                let end = points.end.unwrap_or(length);
                if end <= points.start {
                    self.status = "The end would be at or before the start".to_owned();
                    return;
                }
                format!("ends at {}", seconds(end, rate))
            }
            SampleChange::Loop(None) => {
                points.loop_ = None;
                "has no loop of its own".to_owned()
            }
            SampleChange::Loop(Some((from, to, crossfade))) => {
                let (start, end) = (at(from), at(to));
                if end <= start {
                    self.status = "A loop ends after it starts".to_owned();
                    return;
                }
                let crossfade = frames(crossfade, rate).min(u64::from(u32::MAX)) as u32;
                points.loop_ = Some(Loop {
                    start,
                    end,
                    crossfade,
                });
                format!(
                    "loops from {} to {}, with Play on Loop or Ping-pong",
                    seconds(start, rate),
                    seconds(end, rate)
                )
            }
            SampleChange::Reverse(on) => {
                play.reverse = on.unwrap_or(!play.reverse);
                if play.reverse {
                    "plays reversed".to_owned()
                } else {
                    "plays forward".to_owned()
                }
            }
            SampleChange::Fade(fade_in, fade_out) => {
                let fade = |micros| frames(micros, rate).min(length) as u32;
                (play.fade_in, play.fade_out) = (fade(fade_in), fade(fade_out));
                format!(
                    "fades in over {} and out over {}",
                    seconds(u64::from(play.fade_in), rate),
                    seconds(u64::from(play.fade_out), rate)
                )
            }
            SampleChange::Gain(hundredths) => {
                play.gain_db = hundredths as f32 / 100.0;
                format!("plays at {:+.1} dB", play.gain_db)
            }
            SampleChange::Tune(semitones, cents) => {
                (play.tune, play.cents) = (semitones, cents);
                format!("is tuned {semitones:+} st {cents:+} ct")
            }
        };
        self.apply(&Action::SetBankEntry {
            number,
            entry: Some(entry),
        });
        self.status = format!("{} {said}", sample_text(self.song(), number));
    }

    /// The rate an entry's file was recorded at, and its length in the file's frames, if it's
    /// loaded.
    fn file_of(&self, entry: &BankEntry) -> Option<(u32, u64)> {
        let id = self.samples.id_of(&entry.source()?.path)?;
        let loaded = self.samples.get(id)?.frames.len() as u64;
        // One made at the program's rate, as a recording is, is at the program's rate.
        match self.samples.rates(id) {
            Rates::SAME => Some((self.rate, loaded)),
            rates => Some((rates.file.max(1), rates.file_frame(loaded))),
        }
    }
}
