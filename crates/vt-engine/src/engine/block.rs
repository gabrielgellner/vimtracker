//! A block of audio: the levels it reports, timing where its time goes, the sequencing
//! through it, and the render of every track, group, return and the master.

use std::sync::Arc;

use vt_model::constants::{MAX_GROUPS, MAX_TRACKS, TICKS_PER_QUARTER, TICKS_PER_STEP};

use crate::mixer::{Reading, master};
use crate::published::Bank;
use crate::timing::{Part as TimedPart, PartTimes, SlotTimes};
use crate::track_state::ShapedFrame;
use crate::voice::Voice;

use super::{Engine, RestingBuses, Transport};

impl Engine {
    /// Fill interleaved stereo output. This doesn't allocate, lock or print.
    /// The master's levels since this was last asked.
    pub fn take_reading(&mut self) -> Reading {
        std::mem::take(&mut self.reading)
    }

    /// Each track's levels over the last so many frames. A silent track skips adding to its reading,
    /// so the frames are given here, and an average counts the silence.
    pub fn take_track_readings(&mut self, frames: u32) -> [Reading; MAX_TRACKS] {
        let mut readings = [Reading::default(); MAX_TRACKS];
        for (reading, track) in readings.iter_mut().zip(self.tracks.iter_mut()) {
            *reading = std::mem::take(&mut track.reading);
            reading.frames = frames;
        }
        readings
    }

    /// The highest level of a track's sides since this was last asked, right after its trim and
    /// before its filter, for the Output device's meter. Every track's starts again from silence,
    /// so one watched next doesn't begin with what it played while it wasn't.
    pub fn take_after_trim(&mut self, index: usize) -> [f32; 2] {
        let taken = self
            .tracks
            .get(index)
            .map_or([0.0; 2], |track| track.after_trim);
        for track in &mut self.tracks {
            track.after_trim = [0.0; 2];
        }
        taken
    }

    pub fn process(&mut self, out: &mut [f32]) {
        self.stopwatch.restart_lap();
        let mut written = 0;
        let total = out.len() / 2;
        while written < total {
            let playing = self.transport == Transport::Playing;
            // A track's own step that falls before the song's next is worked out first, so every
            // step is worked out in the order it falls.
            let track_step = if playing {
                self.next_track_step()
            } else {
                None
            };
            if let Some((index, 0)) = track_step {
                let at = self.tracks[index].clock.beat;
                self.track_step(index, at, false);
                continue;
            }
            if playing && self.frames_until_next_step() == 0 {
                self.fire_step();
                self.next_step += 1;
                self.place += 1;
                self.section_place += 1;
                continue;
            }
            if playing {
                self.fire_due_events();
            }
            self.track_new_notes();
            if self.lfo_countdown == 0 {
                self.update_lfos();
                self.lfo_countdown = LFO_UPDATE_FRAMES;
            }
            let until_event = if playing {
                self.frames_until_next_step()
                    .min(self.frames_until_track_event())
                    .min(track_step.map_or(u64::MAX, |(_, frames)| frames))
            } else {
                u64::MAX
            };
            let run = until_event
                .min((total - written) as u64)
                .min(u64::from(self.lfo_countdown))
                .max(1) as usize;
            self.lfo_countdown -= run as u32;
            self.render(&mut out[written * 2..(written + run) * 2]);
            self.rendered += run as u64;
            if playing {
                self.frames_since_origin += run as u64;
                self.clock += run as u64;
            }
            written += run;
        }
        self.stopwatch.lap(TimedPart::Sequencing);
    }

    /// Start timing where the engine's time goes, a track and a slot at a time, from now. This
    /// allocates, so it's done before audio starts.
    pub fn time_parts(&mut self) {
        self.stopwatch.start();
        for track in self.tracks.iter_mut() {
            track.slot_times = SlotTimes::running();
        }
    }

    /// Where the engine's time has gone since [`Engine::time_parts`], if it was asked.
    pub fn part_times(&mut self) -> Option<PartTimes> {
        for (index, track) in self.tracks.iter_mut().enumerate() {
            self.stopwatch.add_slots(index, &mut track.slot_times);
        }
        self.stopwatch.times().cloned()
    }

    /// Which of the groups, returns and master can be left out of a run, as working them out would
    /// give the same frame every frame and change nothing (#558). A group rests while none of its tracks plays, given
    /// as each playing track's group, and the returns and master while no track plays at all and
    /// every group rests; the master too once each of its slots has settled. A note, a preview or
    /// an audition plays a track, and an edit, a lock or the transport changes what a bus is set
    /// to, so each run asks again. Done once, a resting group or return hears its silence, which
    /// is all that changes about it while it rests.
    fn resting_buses(
        &mut self,
        routes: &[Option<usize>],
        smoothing: f32,
        fast: f32,
        rate: u32,
    ) -> RestingBuses {
        let mut resting = RestingBuses::default();
        if !self.resting {
            return resting;
        }
        let groups = self.sounds();
        let return_a = self.returns();
        let master = return_a + 2;
        let mut fed = [false; MAX_GROUPS];
        for &group in routes.iter().flatten() {
            fed[group] = true;
        }
        for (group, rests) in resting.groups.iter_mut().enumerate() {
            let index = groups + group;
            *rests = !fed[group] && self.tracks[index].group_rests(smoothing, fast, rate);
            if *rests {
                self.tracks[index].rest_bus();
                self.keys.frames[index] = [0.0; 2];
            }
        }
        if !routes.is_empty() || !resting.groups.iter().all(|&rests| rests) {
            return resting;
        }
        for (place, rests) in resting.returns.iter_mut().enumerate() {
            let index = return_a + place;
            *rests = !self.tracks[index].ringing(rate);
            if *rests {
                self.tracks[index].rest_bus();
                self.keys.frames[index] = [0.0; 2];
            }
        }
        // Every other track is silent now, so whatever a slot of the master hears is.
        if resting.returns.iter().all(|&rests| rests) {
            resting.master = self.tracks[master].master_rests(smoothing, fast, rate, &self.keys);
        }
        resting
    }

    /// Render voices and effects into a run of frames with no events in it.
    fn render(&mut self, out: &mut [f32]) {
        self.stopwatch.lap(TimedPart::Sequencing);
        self.keep_time();
        let samples = Arc::clone(&self.current.samples);
        let limiter = self.current.song.limiter;
        let groups = self.sounds();
        let return_a = self.returns();
        let (return_b, master_track) = (return_a + 1, return_a + 2);
        let wet_only = self.return_soloed();
        self.keys.sounds = groups;
        // A note or a preview only starts between runs, and a skipped track doesn't change, so a
        // track that's silent at the start of this one stays silent through it and is only asked
        // once. The others are asked each frame, as one can fall silent part way.
        // The fixed tracks have their turn after the rest: the groups with what their tracks play,
        // then the returns with what's sent to them.
        // The preview voices sounding are found once too, so with none there's nothing to ask.
        let previews = self.previews.sounding();
        let mut playing = [0_usize; MAX_TRACKS];
        // Where each playing track goes: its group's place among the groups, or the master.
        let mut routes = [None; MAX_TRACKS];
        // While a return is soloed, a group is heard only if it or one of its tracks is soloed.
        let mut lit = [false; MAX_GROUPS];
        for (group, lit) in lit.iter_mut().enumerate() {
            *lit = self.tracks[groups + group].solo;
        }
        let mut count = 0;
        for index in 0..self.tracks.len() {
            if (groups..=master_track).contains(&index) {
                continue;
            }
            let route = self.group_of(index).map(|group| group - groups);
            let track = &self.tracks[index];
            if let Some(group) = route
                && track.solo
            {
                lit[group] = true;
            }
            let previewing = self.previews.on_track(&previews, index);
            if !track.sounding() && !previewing && !track.ringing(self.rate) {
                self.keys.frames[index] = [0.0; 2];
                // Silent, a mute's fade has nothing to fade, so a note it plays later while a
                // compressor hears it doesn't start with the end of one.
                let track = &mut self.tracks[index];
                track.heard.settle();
                track.feeds.settle();
            } else {
                playing[count] = index;
                routes[count] = route;
                count += 1;
            }
        }
        let (smoothing, fast, rate) = (self.smoothing, self.fast_smoothing, self.rate);
        let resting = self.resting_buses(&routes[..count], smoothing, fast, rate);
        self.rested = resting;
        if let Some(rest) = resting.master {
            // Nothing in the mix moves, so every frame of the run is the same, nearly always
            // silence.
            let frames = (out.len() / 2) as u32;
            let finished = master(rest.out, limiter);
            let master_track = &mut self.tracks[master_track];
            master_track.rest_master(&rest, frames);
            master_track.reading.add_steady(rest.out, finished, frames);
            self.reading.add_steady(rest.out, finished, frames);
            for frame in out.as_chunks_mut::<2>().0 {
                *frame = finished;
            }
            self.stopwatch.lap(TimedPart::Mixing);
            return;
        }
        for frame in out.as_chunks_mut::<2>().0 {
            self.stopwatch.lap(TimedPart::Mixing);
            let mut mix = [0.0_f32; 2];
            let mut buses = [[0.0_f32; 2]; MAX_GROUPS];
            let mut send_a = [0.0_f32; 2];
            let mut send_b = [0.0_f32; 2];
            for (&index, &route) in playing[..count].iter().zip(&routes[..count]) {
                let track = &mut self.tracks[index];
                let preview = self.previews.next(&previews, index, &samples, &self.files);
                if !track.sounding() && preview.is_none() && !track.ringing(self.rate) {
                    self.keys.frames[index] = [0.0; 2];
                    self.stopwatch.lap(TimedPart::Voice(index));
                    continue;
                }
                let main = track.voice.next(samples.get(track.voice.sample()));
                let tail = track.tail.next(samples.get(track.tail.sample()));
                self.stopwatch.lap(TimedPart::Voice(index));
                let preview = preview.unwrap_or([0.0; 2]);
                let shaped = track.shape(
                    [
                        main[0] + tail[0] + preview[0],
                        main[1] + tail[1] + preview[1],
                    ],
                    self.smoothing,
                    self.fast_smoothing,
                    self.rate,
                    &self.keys,
                );
                self.keys.frames[index] = track.key;
                // Muted but heard by a compressor or Warps, it plays on, faded out of the mix and
                // sends.
                let heard = track.heard.next(self.smoothing);
                let feeds = track.feeds.next(self.smoothing);
                let dry = shaped.dry.map(|sample| sample * heard);
                track.reading.add(dry, dry);
                // A track in a group always goes to it, and the group decides what's heard. What
                // it gives the group may be more than what's heard of it, while the group's heard
                // by a compressor and is silent itself.
                match route {
                    Some(group) => {
                        buses[group][0] += shaped.dry[0] * feeds;
                        buses[group][1] += shaped.dry[1] * feeds;
                    }
                    None if !wet_only || track.solo => {
                        mix[0] += dry[0];
                        mix[1] += dry[1];
                    }
                    None => {}
                }
                send_a[0] += shaped.send_a[0] * heard;
                send_a[1] += shaped.send_a[1] * heard;
                send_b[0] += shaped.send_b[0] * heard;
                send_b[1] += shaped.send_b[1] * heard;
                self.stopwatch.lap(TimedPart::Shaping(index));
            }
            let keys = &mut self.keys;
            for (group, bus) in buses.into_iter().enumerate() {
                let index = groups + group;
                // A group at rest gives exactly what it would working out its silence. The zeros
                // are still added, as adding one turns a `-0.0` into a `0.0`.
                let shaped = if resting.groups[group] {
                    ShapedFrame::SILENT
                } else {
                    let shaped = self.tracks[index].process_group(bus, smoothing, fast, rate, keys);
                    keys.frames[index] = self.tracks[index].key;
                    shaped
                };
                if !wet_only || lit[group] {
                    mix[0] += shaped.dry[0];
                    mix[1] += shaped.dry[1];
                }
                send_a[0] += shaped.send_a[0];
                send_a[1] += shaped.send_a[1];
                send_b[0] += shaped.send_b[0];
                send_b[1] += shaped.send_b[1];
                self.stopwatch.lap(TimedPart::Shaping(index));
            }
            let mut from = [[0.0_f32; 2]; 2];
            for ((index, send), (from, rests)) in [(return_a, send_a), (return_b, send_b)]
                .into_iter()
                .zip(from.iter_mut().zip(resting.returns))
            {
                if !rests {
                    *from = self.tracks[index].process_return(send, smoothing, fast, rate, keys);
                    keys.frames[index] = self.tracks[index].key;
                }
                self.stopwatch.lap(TimedPart::Shaping(index));
            }
            let [from_a, from_b] = from;
            let mixed = [
                mix[0] + from_a[0] + from_b[0],
                mix[1] + from_a[1] + from_b[1],
            ];
            let mastered =
                self.tracks[master_track].process_master(mixed, smoothing, fast, rate, &self.keys);
            self.stopwatch.lap(TimedPart::Shaping(master_track));
            let out = master(mastered, limiter);
            self.tracks[master_track].reading.add(mastered, out);
            self.reading.add(mastered, out);
            *frame = out;
            self.stopwatch.lap(TimedPart::Limiting);
        }
        self.stopwatch.lap(TimedPart::Mixing);
    }

    /// Tell every note keeping to the song's tempo, under `Play`'s `Tempo`, how long a step is at
    /// the tempo now, before a run: from the same tempo the sequencer times its steps by, so a
    /// loop lasting 16 steps comes round again as the 17th step does, however long it plays.
    fn keep_time(&mut self) {
        let step = TICKS_PER_STEP as f64 * 60.0 * f64::from(self.rate) * 1000.0
            / (TICKS_PER_QUARTER as f64 * f64::from(self.millibpm));
        let bank = &self.current.bank;
        for track in self.tracks.iter_mut() {
            keep_voice_time(&mut track.voice, bank, step);
            keep_voice_time(&mut track.tail, bank, step);
        }
        self.previews
            .follow(|voice| keep_voice_time(voice, bank, step), |_| {});
    }
}

/// How often LFOs are worked out, in frames. Their values are smoothed in between.
const LFO_UPDATE_FRAMES: u32 = 32;

/// Tell a voice keeping to the tempo how long a step is, and with `Beats`, where its entry's sounds
/// are around its clock.
fn keep_voice_time(voice: &mut Voice, bank: &Bank, step: f64) {
    if !voice.keeps_time() {
        return;
    }
    let playable = voice
        .entry()
        .and_then(|entry| bank.get(usize::from(entry))?.as_deref())
        .filter(|playable| playable.sample == voice.sample());
    voice.keep_time(step, |span, place| match playable {
        Some(playable) => playable.beat_around(span, place),
        None => (f64::NEG_INFINITY, f64::INFINITY),
    });
}
