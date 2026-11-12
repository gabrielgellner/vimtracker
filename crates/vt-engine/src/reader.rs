//! How a sampler's voice reads its sample: a span of it, from its start to its end, round a loop
//! forward or back and forth, reversed, with fades, and a short declick wherever it cuts into the
//! sound, so no edge clicks.
//!
//! Places are counted in play order: frames from the file's first, or from its last when it plays
//! reversed. So reversing changes only how a place becomes a frame, and a loop, a fade and an end
//! work the same either way.
//!
//! Under `Play`'s `Tempo`, the song's tempo sets how fast it moves through its loop, so its entry
//! lasts as many steps as it says: the engine tells it the tempo before every run it renders, so
//! a tempo change is heard within a run and nothing adds up to drift. With the `Beats` warp, a
//! clock moves through the loop at that speed instead, and each sound starts as the clock reaches
//! it and plays at its own speed.

use vt_model::constants::DECLICK_FRAMES;
use vt_model::playable::Span;
use vt_model::sample::SampleData;
use vt_model::settings::PlayMode;

/// The shortest a fade at a cut can be, in frames, so a cut into the sound doesn't click.
const DECLICK: f64 = DECLICK_FRAMES as f64;

/// What a track's settings make of a span as a note starts: how far into it the note starts and
/// how much of it plays, as shares of the whole, and how it plays.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cut {
    pub start: f64,
    pub length: f64,
    pub mode: PlayMode,
}

impl Default for Cut {
    fn default() -> Self {
        Self {
            start: 0.0,
            length: 1.0,
            mode: PlayMode::OneShot,
        }
    }
}

/// A loop as it's read, in play order.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Looping {
    start: f64,
    end: f64,
    /// Whether it goes back and forth rather than round.
    ping_pong: bool,
    /// How far the crossfade into its start runs, and whether it runs in from before the start, so
    /// it's over as the loop starts again, or, with no room there, on past the end.
    crossfade: f64,
    before: bool,
}

impl Looping {
    /// Where a forward loop goes back round.
    fn turn(&self) -> f64 {
        if self.before {
            self.end - self.crossfade
        } else {
            self.end
        }
    }
}

/// What a note reads, in play order.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Region {
    /// The whole sample's length, to turn a place in play order into a frame.
    frames: f64,
    reverse: bool,
    start: f64,
    /// One past the last place, after which a note with no loop ends.
    end: f64,
    looping: Option<Looping>,
    /// How far it fades in from the start and out to the end.
    fade_in: f64,
    fade_out: f64,
}

impl Region {
    /// The whole of a sample, as it is, however long it turns out to be.
    const WHOLE: Self = Self {
        frames: 0.0,
        reverse: false,
        start: 0.0,
        end: f64::INFINITY,
        looping: None,
        fade_in: 0.0,
        fade_out: 0.0,
    };

    /// What a cut makes of a span. `Start` and `Length` are shares of the span from where it starts
    /// in play order, so a reversed sample's start is its end. The loop is the span's own, cut to
    /// what plays, or all that plays if it has none, or none is left of it.
    fn new(span: Span, cut: Cut) -> Self {
        let frames = span.frames as f64;
        let (first, last) = if span.reverse {
            (frames - span.end as f64, frames - span.start as f64)
        } else {
            (span.start as f64, span.end as f64)
        };
        let whole = last - first;
        let start = first + whole * cut.start.clamp(0.0, 1.0);
        let end = (start + whole * cut.length.clamp(0.0, 1.0)).min(last);
        // An edge where the file has more sound is a cut into it.
        let fade_in = f64::from(span.fade_in);
        let fade_out = f64::from(span.fade_out);
        let looping = match cut.mode {
            // On through is once, over a span the entry has already carried on to its end.
            PlayMode::OneShot | PlayMode::Thru => None,
            // Loop to tempo goes round as a loop does, at the tempo's speed.
            mode => {
                let own = span.loop_.map(|found| {
                    let (start, end) = if span.reverse {
                        (frames - found.end as f64, frames - found.start as f64)
                    } else {
                        (found.start as f64, found.end as f64)
                    };
                    (start, end.min(last), f64::from(found.crossfade))
                });
                let (from, to, crossfade) = match own {
                    Some((from, to, crossfade)) if from < to && to > start => {
                        (from, to.min(end), crossfade)
                    }
                    _ => (start, end, 0.0),
                };
                looping(from, to, crossfade, mode == PlayMode::PingPong)
            }
        };
        Self {
            frames,
            reverse: span.reverse,
            start,
            end,
            looping,
            fade_in: if start > 0.0 {
                fade_in.max(DECLICK)
            } else {
                fade_in
            },
            fade_out: if end < frames {
                fade_out.max(DECLICK)
            } else {
                fade_out
            },
        }
    }

    /// The frame a place in play order is at, which is before the first if it's negative.
    fn frame(&self, place: f64) -> f64 {
        if self.reverse {
            self.frames - 1.0 - place
        } else {
            place
        }
    }
}

/// A loop from one place to another, with a crossfade of about so many frames, or none if it's
/// too short to loop. The crossfade is at least a declick, and at most half the loop and the room
/// before its start.
fn looping(start: f64, end: f64, crossfade: f64, ping_pong: bool) -> Option<Looping> {
    if end - start < 2.0 {
        return None;
    }
    let half = (end - start) / 2.0;
    let declick = DECLICK.min(half);
    let (crossfade, before) = if start >= declick {
        (crossfade.max(declick).min(half).min(start), true)
    } else {
        (declick, false)
    };
    Some(Looping {
        start,
        end,
        ping_pong,
        crossfade,
        before,
    })
}

/// The place a forward loop left, read on as it fades out while the loop's start fades in.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Crossing {
    place: f64,
    /// How far it has left to fade, and how far it fades in all.
    left: f64,
    length: f64,
}

/// How a note under `Tempo` keeps time.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Tempo {
    /// How many places it moves each frame at the song's tempo, which the engine sets before each
    /// run, or none until it first has.
    rate: Option<f64>,
    /// With the `Beats` warp, the clock its sounds start by.
    clock: Option<Clock>,
}

/// Where the clock `Beats` starts sounds by is in the loop, in play order, and the sound it's in:
/// the place that sound started, and the place the next one starts, where this one ends.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Clock {
    place: f64,
    from: f64,
    until: f64,
}

impl Tempo {
    /// How a note keeps time under a cut, if it does: under `Tempo`, round a loop, for an entry
    /// that says how long it is.
    fn of(span: Span, cut: Cut, region: &Region) -> Option<Self> {
        let looping = region.looping?;
        if cut.mode != PlayMode::Tempo || span.step_frames.is_none() {
            return None;
        }
        Some(Self {
            rate: None,
            clock: span.beats.then_some(Clock {
                place: region.start,
                from: region.start,
                until: looping.end,
            }),
        })
    }
}

/// Where a voice is in its sample, and how it reads on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reader {
    /// The span it was given, if it was given one, and the cut, to work out again as it changes.
    span: Option<Span>,
    cut: Cut,
    region: Region,
    place: f64,
    /// Whether a ping-pong loop has it going back.
    backwards: bool,
    crossing: Option<Crossing>,
    /// How far it has read since the note started, for its fade in.
    read: f64,
    /// What the fades multiply the sound by now, which never moves faster than a declick.
    edge: f32,
    /// How it keeps time under `Tempo`, if it does.
    tempo: Option<Tempo>,
}

impl Default for Reader {
    fn default() -> Self {
        Self::new(None, Cut::default())
    }
}

impl Reader {
    /// Start reading a span as a cut makes of it, or the whole sample as it is with none.
    pub fn new(span: Option<Span>, cut: Cut) -> Self {
        let region = span.map_or(Region::WHOLE, |span| Region::new(span, cut));
        let mut reader = Self {
            span,
            cut,
            region,
            place: region.start,
            backwards: false,
            crossing: None,
            read: 0.0,
            edge: 1.0,
            tempo: span.and_then(|span| Tempo::of(span, cut, &region)),
        };
        reader.edge = reader.edge_target();
        reader
    }

    /// The span it's reading.
    pub fn span(&self) -> Option<Span> {
        self.span
    }

    /// The frame of its sample it's at, for a playhead: counted from the first, whichever way it
    /// plays.
    pub fn frame(&self) -> u64 {
        self.region.frame(self.place).max(0.0) as u64
    }

    /// Whether it plays on past its slice's end, so the span it follows is the one that does.
    pub fn thru(&self) -> bool {
        self.cut.mode == PlayMode::Thru
    }

    /// Carry on with a span that's changed, as moving a point does, from the same frame. A note
    /// past its new end fades out, one past its loop goes round with a crossfade, and a reversed
    /// one reads on from where it is, the other way.
    pub fn respan(&mut self, span: Span) {
        if self.span == Some(span) {
            return;
        }
        let region = Region::new(span, self.cut);
        if region.reverse != self.region.reverse {
            self.place = self.region.frames - 1.0 - self.place;
            if let Some(crossing) = &mut self.crossing {
                crossing.place = self.region.frames - 1.0 - crossing.place;
            }
        }
        self.span = Some(span);
        self.region = region;
        // A new length or warp keeps time from here: a clock, if it now needs one, starts here.
        let rate = self.tempo.and_then(|tempo| tempo.rate);
        let clock = self.tempo.and_then(|tempo| tempo.clock);
        self.tempo = Tempo::of(span, self.cut, &region).map(|tempo| Tempo {
            rate,
            clock: tempo.clock.map(|fresh| {
                let place = clock.map_or(self.place, |clock| clock.place);
                Clock {
                    place,
                    from: clock.map_or(self.place, |clock| clock.from),
                    until: fresh.until,
                }
            }),
        });
    }

    /// Whether it keeps to the song's tempo, so the engine tells it the tempo before each run.
    pub fn keeps_time(&self) -> bool {
        self.tempo.is_some()
    }

    /// Keep to the song's tempo for the next run, at this many frames a step: it moves through
    /// its loop so its entry lasts as many steps as it says. With `Beats`, `around` says where the
    /// clock's sound starts and where the next does, so this one ends.
    pub fn keep_time(&mut self, step: f64, around: impl FnOnce(&Span, f64) -> (f64, f64)) {
        let (Some(tempo), Some(span)) = (&mut self.tempo, self.span) else {
            return;
        };
        let Some(step_frames) = span.step_frames else {
            return;
        };
        tempo.rate = Some(step_frames / step.max(f64::MIN_POSITIVE));
        let Some(clock) = &mut tempo.clock else {
            return;
        };
        let Some(looping) = self.region.looping else {
            return;
        };
        let (from, until) = around(&span, clock.place);
        // A sound that started since, as one does when the sounds change under the note, starts
        // now.
        if from > clock.from && from <= clock.place {
            let until = until.min(looping.end);
            self.start_beat(from, until);
            return;
        }
        if let Some(clock) = self.tempo.as_mut().and_then(|tempo| tempo.clock.as_mut()) {
            clock.until = until.min(looping.end).max(clock.from);
        }
    }

    /// Start the sound the clock has reached, from where the clock is: whatever's still sounding
    /// fades into it, as a loop's turn does.
    fn start_beat(&mut self, from: f64, until: f64) {
        let Some(clock) = self.tempo.as_mut().and_then(|tempo| tempo.clock.as_mut()) else {
            return;
        };
        let old = clock.until;
        clock.from = from;
        clock.until = until;
        let place = clock.place;
        if self.place < old && self.edge > 0.0 {
            let length = DECLICK.min((until - from) / 2.0).max(1.0);
            self.crossing = Some(Crossing {
                place: self.place,
                left: length,
                length,
            });
        }
        self.place = place;
    }

    /// Move the clock `Beats` keeps on to its next sound, round its loop.
    fn follow_beats(&mut self, looping: Looping) {
        let Some(clock) = self.tempo.and_then(|tempo| tempo.clock) else {
            return;
        };
        if clock.place >= looping.end {
            let mut place = clock.place - (looping.end - looping.start);
            if place >= looping.end || place < looping.start {
                place = looping.start;
            }
            if let Some(clock) = self.tempo.as_mut().and_then(|tempo| tempo.clock.as_mut()) {
                clock.place = place;
            }
            // Round again, the loop's start starts a sound, until the engine says where it ends.
            self.start_beat(looping.start, looping.end);
        } else if clock.place >= clock.until {
            self.start_beat(clock.until, looping.end);
        }
    }

    /// The next frame, before it moves on, or none once it's past the sample. A loop's turn
    /// comes first, so the frame read is always in the loop.
    #[inline]
    pub fn next(&mut self, data: &SampleData) -> Option<[f32; 2]> {
        let clock = self.tempo.and_then(|tempo| tempo.clock);
        if let Some(looping) = self.region.looping {
            match clock {
                Some(_) => self.follow_beats(looping),
                None => self.follow(looping),
            }
        }
        // Between sounds, in the gap one leaves ending before the next is due, it's silent.
        let clock = self.tempo.and_then(|tempo| tempo.clock);
        let mut frame = match clock {
            Some(clock) if self.place >= clock.until => [0.0; 2],
            Some(_) => self.at(data, self.place).unwrap_or([0.0; 2]),
            None => self.at(data, self.place)?,
        };
        if let Some(crossing) = &mut self.crossing {
            let left = (crossing.left / crossing.length) as f32;
            let old = at(data, &self.region, crossing.place).unwrap_or([0.0; 2]);
            frame = [
                frame[0] + (old[0] - frame[0]) * left,
                frame[1] + (old[1] - frame[1]) * left,
            ];
        }
        let target = self.edge_target();
        if target != self.edge || self.edge != 1.0 {
            let most = (1.0 / DECLICK) as f32;
            self.edge = target.clamp(self.edge - most, self.edge + most);
            frame = [frame[0] * self.edge, frame[1] * self.edge];
        }
        Some(frame)
    }

    /// Move on by so many frames, and say whether there's more to read: a note with no loop ends
    /// once it's past its end and faded out. Keeping to the tempo, it moves at the tempo's speed
    /// instead, or with `Beats`, its clock does, and its sound at the speed it's given.
    #[inline]
    pub fn advance(&mut self, by: f64) -> bool {
        let mut by = by;
        if let Some(tempo) = &mut self.tempo
            && let Some(rate) = tempo.rate
        {
            match &mut tempo.clock {
                Some(clock) => clock.place += rate,
                None => by = rate,
            }
        }
        if self.backwards {
            self.place -= by;
        } else {
            self.place += by;
        }
        self.read += by;
        if let Some(crossing) = &mut self.crossing {
            crossing.place += by;
            crossing.left -= by;
            if crossing.left <= 0.0 {
                self.crossing = None;
            }
        }
        self.region.looping.is_some() || self.place < self.region.end || self.edge > 0.0
    }

    /// The frame at a place, or none past the sample.
    #[inline]
    fn at(&self, data: &SampleData, place: f64) -> Option<[f32; 2]> {
        at(data, &self.region, place)
    }

    /// Go round or turn back at a loop's end.
    fn follow(&mut self, looping: Looping) {
        if looping.ping_pong {
            let top = looping.end - 1.0;
            if !self.backwards && self.place > top {
                self.place = (2.0 * top - self.place).max(looping.start);
                self.backwards = true;
            } else if self.backwards && self.place < looping.start {
                self.place = (2.0 * looping.start - self.place).min(top);
                self.backwards = false;
            }
            return;
        }
        self.backwards = false;
        let turn = looping.turn();
        if self.place >= turn {
            let length = if looping.before {
                looping.crossfade
            } else {
                DECLICK.min((looping.end - looping.start) / 2.0)
            };
            self.crossing = Some(Crossing {
                place: self.place,
                left: length,
                length,
            });
            self.place -= looping.end - looping.start;
            // Far past it, as a loop moved under the note leaves it, it starts the loop again.
            if self.place >= turn {
                self.place = looping.start - (looping.end - turn);
            }
        }
    }

    /// What the fades want to multiply the sound by here: in from the note's start, and out to
    /// its end if it has no loop, or none once it's past the end.
    fn edge_target(&self) -> f32 {
        let region = &self.region;
        let mut edge = 1.0;
        if self.read < region.fade_in {
            edge = self.read / region.fade_in;
        }
        if region.looping.is_none() {
            let left = region.end - self.place;
            if left <= 0.0 {
                edge = 0.0;
            } else if left < region.fade_out {
                edge *= left / region.fade_out;
            }
        }
        // A sound `Beats` started fades out as it reaches the next one's start.
        if let Some(clock) = self.tempo.and_then(|tempo| tempo.clock) {
            let left = clock.until - self.place;
            if left <= 0.0 {
                edge = 0.0;
            } else if left < DECLICK {
                edge *= left / DECLICK;
            }
        }
        edge as f32
    }
}

/// The frame at a place in a region, between the two either side of it, or none past the sample.
#[inline]
fn at(data: &SampleData, region: &Region, place: f64) -> Option<[f32; 2]> {
    let frame = region.frame(place);
    if frame < 0.0 {
        return None;
    }
    let index = frame as usize;
    let first = data.frames.get(index)?;
    let second = data.frames.get(index + 1).unwrap_or(first);
    let fraction = (frame - index as f64) as f32;
    Some([
        first[0] + (second[0] - first[0]) * fraction,
        first[1] + (second[1] - first[1]) * fraction,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use vt_model::playable::LoopSpan;

    /// A ramp, so each frame says where it is.
    fn ramp(frames: usize) -> SampleData {
        SampleData {
            frames: (0..frames).map(|at| [at as f32, at as f32]).collect(),
        }
    }

    /// Every frame a reader gives, at its own speed, up to a limit.
    fn read(mut reader: Reader, data: &SampleData, most: usize) -> Vec<f32> {
        let mut out = Vec::new();
        while out.len() < most {
            let Some(frame) = reader.next(data) else {
                break;
            };
            out.push(frame[0]);
            if !reader.advance(1.0) {
                break;
            }
        }
        out
    }

    fn span(start: u64, end: u64) -> Span {
        Span {
            start,
            end,
            ..Span::whole(1000)
        }
    }

    #[test]
    fn the_whole_sample_reads_as_it_is() {
        let data = ramp(100);
        let out = read(Reader::default(), &data, 1000);
        assert_eq!(out.len(), 100);
        assert_eq!(out[99], 99.0);
    }

    #[test]
    fn a_span_reads_from_its_start_to_its_end_with_declicks_at_the_cuts() {
        let data = ramp(1000);
        let out = read(
            Reader::new(Some(span(200, 600)), Cut::default()),
            &data,
            2000,
        );
        assert_eq!(out.len(), 401, "its frames, then one faded to nothing");
        assert_eq!(out[0], 0.0, "faded in from nothing");
        assert!((out[200] - 400.0).abs() < 1e-3);
        assert!(out[399] < 600.0 / DECLICK as f32 * 2.0, "faded out");
        assert_eq!(out[400], 0.0);
    }

    #[test]
    fn start_and_length_are_shares_of_the_span() {
        let data = ramp(1000);
        let cut = Cut {
            start: 0.25,
            length: 0.5,
            mode: PlayMode::OneShot,
        };
        let out = read(Reader::new(Some(span(200, 600)), cut), &data, 2000);
        assert_eq!(out.len(), 201, "from 300 to 500");
        assert!((out[100] - 400.0).abs() < 1e-3);
    }

    #[test]
    fn reversed_it_reads_from_its_end_back() {
        let data = ramp(1000);
        let reversed = Span {
            reverse: true,
            ..span(200, 600)
        };
        let out = read(Reader::new(Some(reversed), Cut::default()), &data, 2000);
        assert_eq!(out.len(), 401);
        assert!((out[100] - 499.0).abs() < 1e-3);
    }

    #[test]
    fn a_forward_loop_goes_round_its_own_loop() {
        let data = ramp(1000);
        let looped = Span {
            loop_: Some(LoopSpan {
                start: 300,
                end: 400,
                crossfade: 0,
            }),
            ..span(200, 600)
        };
        let cut = Cut {
            mode: PlayMode::Loop,
            ..Cut::default()
        };
        let out = read(Reader::new(Some(looped), cut), &data, 2000);
        assert_eq!(out.len(), 2000, "it doesn't end");
        // After the first pass, it's in the loop, a hundred frames apart.
        for at in [1000, 1050, 1099] {
            assert!((out[at] - out[at + 100]).abs() < 1e-3, "{at}");
            assert!(
                (300.0 - DECLICK as f32..400.0).contains(&out[at]),
                "{}",
                out[at]
            );
        }
    }

    #[test]
    fn a_ping_pong_goes_back_and_forth() {
        let data = ramp(1000);
        let cut = Cut {
            mode: PlayMode::PingPong,
            ..Cut::default()
        };
        let out = read(Reader::new(Some(span(200, 300)), cut), &data, 400);
        assert_eq!(out.len(), 400);
        let top = out.iter().fold(0.0_f32, |top, value| top.max(*value));
        assert!((top - 299.0).abs() < 1e-3);
        assert!((out[150] - 249.0).abs() < 2.0, "{}", out[150]);
        assert!(out[350] > 240.0 && out[350] < 260.0, "and up again");
    }

    #[test]
    fn an_end_moved_behind_the_note_fades_it_out() {
        let data = ramp(1000);
        let mut reader = Reader::new(Some(span(0, 1000)), Cut::default());
        for _ in 0..500 {
            reader.next(&data);
            reader.advance(1.0);
        }
        reader.respan(span(0, 400));
        let out = read(reader, &data, 2000);
        assert_eq!(out.len(), DECLICK as usize, "{out:?}");
        for pair in out.windows(2) {
            assert!(pair[1] <= pair[0] + 1.0, "it only fades");
        }
    }
}
