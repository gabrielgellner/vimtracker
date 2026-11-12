//! Changing where the current track's sample starts, ends and loops, and how it plays, from
//! `:sample` and a word, until the sample page comes. Each change is to the bank entry the track's
//! `Sample` is on, so every track playing it hears it, and is one undo step.

use vt_model::action::Action;
use vt_model::bank::{BankEntry, Loop, MOST_STEPS};
use vt_model::constants::{BAR_STEPS, MAX_MILLIBPM, MIN_MILLIBPM};
use vt_model::samples::Rates;

use crate::app::App;
use crate::banking::sample_text;
use crate::input::command::SampleChange;
use crate::views::status::format_bpm;

/// A time in microseconds as frames at a rate, to the nearest.
fn frames(micros: u64, rate: u32) -> u64 {
    ((u128::from(micros) * u128::from(rate) + 500_000) / 1_000_000) as u64
}

/// The tempo, in thousandths of a BPM, at which so many seconds last so many steps, and how many:
/// those it says it lasts, or else the power of two of steps whose tempo is nearest the tempo now.
/// None if it's outside the tempos a song can have.
pub(crate) fn grabbed_tempo(seconds: f64, steps: Option<u32>, now: u32) -> Option<(u32, u32)> {
    if seconds <= 0.0 {
        return None;
    }
    // A step is a sixteenth: four to a beat.
    let tempo = |steps: u32| (f64::from(steps) / 4.0 * 60.0 / seconds * 1000.0).round();
    let in_range =
        |millibpm: f64| (f64::from(MIN_MILLIBPM)..=f64::from(MAX_MILLIBPM)).contains(&millibpm);
    let steps = match steps {
        Some(steps) => steps,
        None => {
            let now = f64::from(now.max(1));
            (0..=MOST_STEPS.ilog2())
                .map(|power| 1 << power)
                .filter(|steps| in_range(tempo(*steps)))
                .min_by(|a, b| {
                    let off = |steps: u32| (tempo(steps) / now).ln().abs();
                    off(*a).total_cmp(&off(*b))
                })?
        }
    };
    let millibpm = tempo(steps);
    in_range(millibpm).then_some((steps, millibpm as u32))
}

/// What a sample's length in steps is now, as the status line says it.
fn length_said(steps: Option<u32>) -> String {
    match steps {
        Some(steps) => format!(
            "lasts {}, for :slice beat and Play's Tempo",
            steps_text(steps)
        ),
        None => "goes by the song's tempo for :slice beat, and loops at its own speed on Tempo"
            .to_owned(),
    }
}

/// A number of steps as it's said: in bars where it's a whole number of them.
pub(crate) fn steps_text(steps: u32) -> String {
    let bar = BAR_STEPS as u32;
    match steps {
        1 => "a step".to_owned(),
        _ if steps == bar => "a bar".to_owned(),
        _ if steps.is_multiple_of(bar) => format!("{} bars", steps / bar),
        _ => format!("{steps} steps"),
    }
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
            SampleChange::Bars(bars) => {
                let steps = bars.map(|bars| u32::from(bars) * BAR_STEPS as u32);
                entry.edit.hints.set_length_steps(steps);
                length_said(steps)
            }
            SampleChange::Steps(steps) => {
                entry.edit.hints.set_length_steps(steps);
                length_said(steps)
            }
        };
        self.apply(&Action::SetBankEntry {
            number,
            entry: Some(entry),
        });
        self.status = format!("{} {said}", sample_text(self.song(), number));
    }

    /// `:bpm sample`, grab tempo: set the song's tempo so the current track's sample, from its
    /// start to its end, lasts as many steps as it says at its own speed, as the Deluge's grab
    /// tempo does. One that doesn't say is taken to last the power of two of steps nearest the
    /// tempo now, as the Deluge takes a new loop, and from then on says so. One undo step.
    pub(crate) fn grab_tempo(&mut self) {
        let number = self
            .song()
            .tracks
            .get(self.cursor.track)
            .filter(|track| track.machine == vt_model::song::Machine::Sampler)
            .map_or(0, |track| track.sample());
        let Some(mut entry) = self.song().bank.get(number).cloned() else {
            self.status = format!(
                "Track {:02} plays no sample to take a tempo from: K on Sample chooses one",
                self.cursor.track + 1
            );
            return;
        };
        let Some((rate, length)) = self.file_of(&entry) else {
            self.status = "Its file isn't loaded, so its length isn't known".to_owned();
            return;
        };
        let points = &entry.edit.points;
        let end = points.end_in(length);
        let start = points.start.min(end);
        let seconds = (end - start) as f64 / f64::from(rate.max(1));
        let said = entry.edit.hints.length_steps();
        // One that says its own tempo, as an Octatrack file does, plays at its own speed at that.
        if said.is_none()
            && let Some(own) = entry.edit.hints.millibpm
        {
            let own = own.clamp(MIN_MILLIBPM, MAX_MILLIBPM);
            self.apply(&Action::SetMilliBpm(own));
            self.status = format!(
                "Tempo {} BPM: {}'s own, at which it plays at its own speed: u to undo",
                format_bpm(own),
                sample_text(self.song(), number)
            );
            return;
        }
        let now = self.song().millibpm;
        let Some((steps, millibpm)) = grabbed_tempo(seconds, said, now) else {
            self.status = format!(
                "{} can't set a tempo between {} and {} BPM",
                sample_text(self.song(), number),
                MIN_MILLIBPM / 1000,
                MAX_MILLIBPM / 1000
            );
            return;
        };
        let mut actions = vec![Action::SetMilliBpm(millibpm)];
        if said.is_none() {
            entry.edit.hints.set_length_steps(Some(steps));
            actions.push(Action::SetBankEntry {
                number,
                entry: Some(entry),
            });
        }
        self.apply(&Action::Batch(actions));
        let taken = match said {
            Some(_) => String::new(),
            None => ", taking it as the nearest power of two to the tempo before".to_owned(),
        };
        self.status = format!(
            "Tempo {} BPM: {} lasts {} at its own speed{taken}: u to undo",
            format_bpm(millibpm),
            sample_text(self.song(), number),
            steps_text(steps)
        );
    }

    /// The rate an entry's file was recorded at, and its length in the file's frames, if it's
    /// loaded.
    pub(crate) fn file_of(&self, entry: &BankEntry) -> Option<(u32, u64)> {
        let id = self.samples.id_of(&entry.source()?.path)?;
        let loaded = self.samples.get(id)?.frames.len() as u64;
        // One made at the program's rate, as a recording is, is at the program's rate.
        match self.samples.rates(id) {
            Rates::SAME => Some((self.rate, loaded)),
            rates => Some((rates.file.max(1), rates.file_frame(loaded))),
        }
    }
}
