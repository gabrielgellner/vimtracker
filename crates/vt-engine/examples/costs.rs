//! What each machine, filter type, slot effect and mod slot type costs, one track at a time, at
//! 48 kHz, as nanoseconds a frame and shares of a core: `just costs` (#187).
//!
//! Each figure is the song's time with each block at its fastest over several runs, less the same
//! song's without the part, so it's what the part adds while it plays. A thing over about 1% of a core an instance, 208 ns a frame, is
//! marked. `just costs worst` plays only a 16-track song of the heaviest parts, and
//! `just costs save folder` saves that song as a project, to bench or open, and
//! `just costs save folder large` the large song an edit is timed on. `just costs blocks`
//! times the slowest block of the effects that do their work a part at a time, against a block's
//! budget at 192 kHz. `just costs edits` times one edit of a large song on the edit thread, and the
//! engine taking its version (#871).

use std::sync::Arc;
use std::time::Instant;

use vt_engine::engine::Engine;
use vt_engine::published::{Handover, Published};
use vt_model::action::{Action, edit};
use vt_model::bank::{BankEntry, ContentHash};
use vt_model::history::History;
use vt_model::insert::{
    BEADS_QUALITY_NAMES, Beads, BeadsMode, Chorus, ChorusType, Clouds, CloudsMode, Compressor,
    CompressorType, Distort, DistortType, Echo, Exciter, FreqShift, Grains, INSERTS, Insert, Pitch,
    Plate, QUALITY_NAMES, Rotary, Space, Spring, Stutter, Warps,
};
use vt_model::modulation::{MOD_SLOTS, ModKind};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{Chain, ChainEntry, FilterType, Machine, Phrase, Song, SynthVoice, Track};
use vt_model::step::Note;

const RATE: u32 = 48_000;
const BLOCK: usize = 512;
/// The audio each run plays, after a second to settle.
const SECONDS: f64 = 3.0;
const RUNS: usize = 7;
/// 1% of a core at 48 kHz.
const ONE_PERCENT_NS: f64 = 1e9 / RATE as f64 * 0.01;

/// Four notes a bar, a beat apart, so the voice always sounds.
const NOTES: [&str; 4] = [
    "C-3 7F --- --- ---",
    "G-3 70 --- --- ---",
    "D#3 78 --- --- ---",
    "C-4 6F --- --- ---",
];

fn pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    let mut phase = 0.0_f32;
    let tone = (0..RATE as usize * 2)
        .map(|_| {
            phase = (phase + 220.0 / RATE as f32).fract();
            let value = (phase * std::f32::consts::TAU).sin() * 0.5;
            [value, value * 0.9]
        })
        .collect();
    pool.insert("tone", SampleData { frames: tone });
    Arc::new(pool)
}

/// Put a machine on a track, playing the notes, or none.
fn set_up(song: &mut Song, track: usize, machine: Machine, notes: bool) {
    if machine == Machine::Sampler && song.bank.get(1).is_none() {
        song.bank
            .set(1, Some(BankEntry::of_file("tone", ContentHash::UNKNOWN)));
    }
    let track = &mut song.tracks[track];
    track.machine = machine;
    if machine == Machine::Sampler {
        track.set_setting(SettingId::SAMPLE, 1);
    }
    if notes {
        let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
        for (at, text) in NOTES.iter().enumerate() {
            *phrase.step_mut(at * 4).unwrap() = text.parse().unwrap();
        }
    }
}

/// A one-track song of a machine, playing or not.
fn one(machine: Machine, notes: bool, edit: impl FnOnce(&mut Track)) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    set_up(&mut song, 0, machine, notes);
    edit(&mut song.tracks[0]);
    song
}

/// What a song takes playing, in nanoseconds a frame.
fn cost(song: &Song, pool: &Arc<SamplePool>) -> f64 {
    cost_of(song, pool, true)
}

/// What a song takes, playing or stopped, in nanoseconds a frame. Every run plays the same blocks,
/// so each block's fastest over the runs is kept and they're added up: a block the system put
/// aside for another thread in one run is fast in another, which keeps the figure steady on a busy
/// machine.
fn cost_of(song: &Song, pool: &Arc<SamplePool>, play: bool) -> f64 {
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(pool)));
    let mut out = vec![0.0_f32; BLOCK * 2];
    let blocks = (SECONDS * f64::from(RATE)) as usize / BLOCK;
    let mut fastest = vec![u128::MAX; blocks];
    for _ in 0..RUNS {
        let mut engine = Engine::new(RATE, Arc::clone(&version));
        if play {
            engine.play_from(0);
        }
        for _ in 0..RATE as usize / BLOCK {
            engine.process(&mut out);
        }
        for fastest in &mut fastest {
            let started = Instant::now();
            engine.process(&mut out);
            *fastest = (*fastest).min(started.elapsed().as_nanos());
        }
    }
    std::hint::black_box(&out);
    fastest.iter().sum::<u128>() as f64 / (blocks * BLOCK) as f64
}

/// A table's row: a name and what it adds.
struct Row {
    name: String,
    ns: f64,
}

fn print_table(title: &str, base: f64, mut rows: Vec<Row>) {
    rows.sort_by(|a, b| b.ns.total_cmp(&a.ns));
    println!();
    println!("{title} (base {base:.0} ns a frame)");
    println!("{:<22} {:>9} {:>8}", "", "ns/frame", "% core");
    for row in rows {
        let percent = row.ns / ONE_PERCENT_NS;
        let mark = if percent > 1.0 { "  *" } else { "" };
        println!("{:<22} {:>9.0} {:>7.2}%{mark}", row.name, row.ns, percent);
    }
}

fn machines(pool: &Arc<SamplePool>) {
    let base = cost(&one(Machine::Sampler, false, |_| {}), pool);
    let rows = Machine::ALL
        .into_iter()
        .map(|machine| Row {
            name: match machine {
                Machine::Sampler => "Sampler".to_owned(),
                Machine::Plaits(voice) => voice.name().to_owned(),
            },
            ns: cost(&one(machine, true, |_| {}), pool) - base,
        })
        .collect();
    print_table("Machines, each playing on a track of its own", base, rows);
}

/// The effects to time: each in `Insert::ALL`, and each type of those that have types.
fn effects() -> Vec<(String, Insert)> {
    let mut all: Vec<(String, Insert)> = Insert::ALL
        .into_iter()
        .filter(|insert| *insert != Insert::Empty)
        .map(|insert| (insert.name().to_owned(), insert))
        .collect();
    for kind in DistortType::ALL.into_iter().skip(1) {
        let distort = Distort {
            kind,
            ..Distort::DEFAULT
        };
        all.push((format!("Distort {kind:?}"), Insert::Distort(distort)));
    }
    for kind in ChorusType::ALL.into_iter().skip(1) {
        let chorus = Chorus {
            kind,
            ..Chorus::DEFAULT
        };
        all.push((format!("Chorus {kind:?}"), Insert::Chorus(chorus)));
    }
    for kind in CompressorType::ALL.into_iter().skip(1) {
        let compressor = Compressor {
            kind,
            ..Compressor::DEFAULT
        };
        all.push((format!("Comp {kind:?}"), Insert::Compressor(compressor)));
    }
    // Clouds' other modes, its other qualities, and its densest grains.
    for mode in CloudsMode::ALL.into_iter().skip(1) {
        let clouds = Clouds {
            mode,
            ..Clouds::DEFAULT
        };
        all.push((format!("Clouds {mode:?}"), Insert::Clouds(clouds)));
    }
    for (quality, name) in QUALITY_NAMES.into_iter().enumerate().skip(1) {
        let clouds = Clouds {
            quality: quality as u8,
            ..Clouds::DEFAULT
        };
        all.push((format!("Clouds {name}"), Insert::Clouds(clouds)));
    }
    for (density, name) in [(0x00, "regular"), (0xFF, "random")] {
        let clouds = Clouds {
            density,
            size: 0xFF,
            quality: 3,
            ..Clouds::DEFAULT
        };
        all.push((
            format!("Clouds densest {name}, Mono 8"),
            Insert::Clouds(clouds),
        ));
    }
    // Its phase vocoder in mono, with every bin's phase scattered, and quantised and flattened.
    for (name, clouds) in [
        (
            "Clouds Spectral, Mono 16",
            Clouds {
                mode: CloudsMode::Spectral,
                quality: 1,
                ..Clouds::DEFAULT
            },
        ),
        (
            "Clouds Spectral, all on",
            Clouds {
                mode: CloudsMode::Spectral,
                density: 0xFF,
                texture: 0xFF,
                size: 0x20,
                pitch: vt_model::insert::MOST_SHIFT + 7,
                ..Clouds::DEFAULT
            },
        ),
    ] {
        all.push((name.to_owned(), Insert::Clouds(clouds)));
    }
    // Beads' delay, its other qualities, its densest longest grains in the dirtiest, and its
    // delay at its shortest with its pitch shifted, which reads each tap twice.
    let delay = Beads {
        mode: BeadsMode::Delay,
        ..Beads::DEFAULT
    };
    all.push(("Beads Delay".to_owned(), Insert::Beads(delay)));
    for (quality, name) in BEADS_QUALITY_NAMES.into_iter().enumerate().skip(1) {
        let beads = Beads {
            quality: quality as u8,
            ..Beads::DEFAULT
        };
        all.push((format!("Beads {name}"), Insert::Beads(beads)));
    }
    for (density, name) in [(0x00, "regular"), (0xFF, "random")] {
        let beads = Beads {
            density,
            size: 0xFF,
            quality: 3,
            ..Beads::DEFAULT
        };
        all.push((
            format!("Beads densest {name}, Scorched cassette"),
            Insert::Beads(beads),
        ));
    }
    let shifted = Beads {
        density: 0x00,
        pitch: vt_model::insert::MOST_SHIFT + 7,
        ..delay
    };
    all.push(("Beads Delay shifted".to_owned(), Insert::Beads(shifted)));
    // The Space Echo with all three heads and its spring, wobbling, as fed back as it goes before
    // it runs away, its tone moving: the most it does at once.
    let echo = Echo {
        mode: 10,
        intensity: 0xD0,
        wow: 0xFF,
        reverb: 0xFF,
        bass: 0xC0,
        treble: 0x40,
        ..Echo::DEFAULT
    };
    all.push((
        "Space Echo, three heads, wow, spring".to_owned(),
        Insert::Echo(echo),
    ));
    all
}

fn slot_effects(pool: &Arc<SamplePool>) {
    let voice = Machine::Plaits(SynthVoice::VirtualAnalog);
    let base = cost(&one(voice, true, |_| {}), pool);
    let rows = effects()
        .into_iter()
        .map(|(name, insert)| Row {
            name,
            ns: cost(&one(voice, true, |track| track.inserts[0] = insert), pool) - base,
        })
        .collect();
    print_table("Slot effects, on a playing VA", base, rows);
}

fn filters(pool: &Arc<SamplePool>) {
    let voice = Machine::Plaits(SynthVoice::VirtualAnalog);
    let base = cost(&one(voice, true, |_| {}), pool);
    let mut rows: Vec<Row> = FilterType::ALL
        .into_iter()
        .map(|kind| Row {
            name: format!("{kind:?}"),
            ns: cost(
                &one(voice, true, |track| {
                    track.filter = kind;
                    track.cutoff = 0x80;
                    track.resonance = 0x40;
                }),
                pool,
            ) - base,
        })
        .collect();
    let driven = |kind: FilterType, name: &str, edit: fn(&mut Track)| Row {
        name: name.to_owned(),
        ns: cost(
            &one(voice, true, |track| {
                track.filter = kind;
                track.cutoff = 0x80;
                track.resonance = 0x40;
                edit(track);
            }),
            pool,
        ) - base,
    };
    rows.push(driven(FilterType::LowPass, "LowPass driven", |track| {
        track.drive = 0x80;
    }));
    rows.push(driven(FilterType::LowPass, "LowPass punch", |track| {
        track.punch = 0x80;
    }));
    rows.push(driven(FilterType::LowPass, "LowPass safe", |track| {
        track.safe = true;
    }));
    rows.push(driven(
        FilterType::LowPass,
        "LowPass + cut/hi-cut",
        |track| {
            track.low_cut = 0x40;
            track.high_cut = 0xC0;
        },
    ));
    rows.push(driven(FilterType::LowPass, "LowPass, Cut moved", |track| {
        // The second slot's envelope, on `Cut` already, given depth, so the filter moves.
        track.mods[1].depth = 0x40;
    }));
    print_table(
        "Track filter types, Cut 80 Res 40, on a playing VA",
        base,
        rows,
    );
}

fn mod_slots(pool: &Arc<SamplePool>) {
    let voice = Machine::Plaits(SynthVoice::VirtualAnalog);
    let base = cost(&one(voice, true, |_| {}), pool);
    let mut rows: Vec<Row> = ModKind::ALL
        .into_iter()
        .map(|kind| Row {
            name: format!("{} on Pitch", kind.name()),
            ns: cost(
                &one(voice, true, |track| {
                    track.mods[2].kind = kind;
                    track.mods[2].destination = Some(SettingId::PITCH);
                    track.mods[2].depth = 2;
                }),
                pool,
            ) - base,
        })
        .collect();
    // What the default slots cost, the note's shape and a filter envelope with no depth, against
    // no slot going anywhere.
    let none = cost(
        &one(voice, true, |track| {
            for slot in &mut track.mods {
                slot.destination = None;
            }
        }),
        pool,
    );
    rows.push(Row {
        name: "the defaults".to_owned(),
        ns: base - none,
    });
    rows.push(Row {
        name: "all 6 going somewhere".to_owned(),
        ns: cost(&one(voice, true, every_slot_moving), pool) - base,
    });
    print_table(
        "Mod slots, the third slot (or all), on a playing VA",
        base,
        rows,
    );
}

/// Every slot moving something: the note's shape, `Cut` and `Res` by envelopes, and the rest by
/// an LFO, Steps and Track.
fn every_slot_moving(track: &mut Track) {
    let kinds = [
        ModKind::Adsr,
        ModKind::Ahd,
        ModKind::Drum,
        ModKind::Lfo,
        ModKind::Steps,
        ModKind::Track,
    ];
    let to = [
        SettingId::VOL,
        SettingId::CUT,
        SettingId::RES,
        SettingId::PITCH,
        SettingId::PAN,
        SettingId::CUT,
    ];
    for slot in 0..MOD_SLOTS {
        track.mods[slot].kind = kinds[slot];
        track.mods[slot].destination = Some(to[slot]);
        track.mods[slot].depth = if slot == 0 { 0 } else { 0x10 };
    }
}

/// Sixteen tracks of the heaviest parts: each its voice, a moving filter, every mod slot going
/// somewhere, and four effects.
fn worst_song() -> Song {
    use SynthVoice::*;
    let voices = [
        SixOp,
        FourOp,
        Supersaw,
        Swarm,
        Chord,
        Speech,
        Modal,
        String,
        Additive,
        Bowed,
        Blown,
        Fluted,
        Bell,
        WaveTerrain,
        Wavetable,
        GrainCloud,
    ];
    let filters = [
        FilterType::ObX,
        FilterType::Ladder,
        FilterType::Diode,
        FilterType::Dirty,
        FilterType::Ms20LowPass,
    ];
    let heavy = [
        Insert::Space(Space::DEFAULT),
        Insert::Spring(Spring::DEFAULT),
        Insert::Rotary(Rotary::DEFAULT),
        Insert::Plate(Plate::DEFAULT),
        Insert::Grains(Grains::DEFAULT),
        Insert::Warps(Warps::DEFAULT),
    ];
    let light = [
        Insert::Exciter(Exciter::DEFAULT),
        Insert::FreqShift(FreqShift::DEFAULT),
        Insert::Stutter(Stutter::DEFAULT),
        Insert::Pitch(Pitch::DEFAULT),
        Insert::Chorus(Chorus {
            kind: ChorusType::JunoBoth,
            ..Chorus::DEFAULT
        }),
    ];
    let mut song = Song::new(16);
    song.limiter = true;
    for (index, voice) in voices.into_iter().enumerate() {
        set_up(&mut song, index, Machine::Plaits(voice), true);
        let track = &mut song.tracks[index];
        track.filter = filters[index % filters.len()];
        track.cutoff = 0x90;
        track.resonance = 0x50;
        track.drive = 0x40;
        every_slot_moving(track);
        for slot in 0..INSERTS {
            track.inserts[slot] = if slot % 2 == 0 {
                heavy[(index + slot) % heavy.len()]
            } else {
                light[(index + slot) % light.len()]
            };
        }
        track.send_a = 0x40;
        track.send_b = 0x40;
    }
    song
}

fn worst(pool: &Arc<SamplePool>) {
    let ns = cost(&worst_song(), pool);
    println!();
    println!(
        "16 tracks, each a heavy voice, a moving driven filter, six mod slots and four effects, \
         sends to both returns and the limiter: {ns:.0} ns a frame, {:.1}% of a core",
        ns / ONE_PERCENT_NS
    );
    let stopped = cost_of(&worst_song(), pool, false);
    println!(
        "The same, stopped: {stopped:.0} ns a frame, {:.2}% of a core",
        stopped / ONE_PERCENT_NS
    );
    let empty = cost(&Song::new(16), pool);
    println!(
        "16 empty tracks, playing: {empty:.0} ns a frame, {:.2}% of a core",
        empty / ONE_PERCENT_NS
    );
}

/// The slowest block of a song at a rate, in microseconds, each block at its fastest over the runs,
/// and its average in nanoseconds a frame.
fn slowest_block(song: &Song, pool: &Arc<SamplePool>, rate: u32, block: usize) -> (f64, f64) {
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(pool)));
    let mut out = vec![0.0_f32; block * 2];
    let blocks = (SECONDS * f64::from(rate)) as usize / block;
    let mut fastest = vec![u128::MAX; blocks];
    for _ in 0..RUNS {
        let mut engine = Engine::new(rate, Arc::clone(&version));
        engine.play_from(0);
        for _ in 0..rate as usize / block {
            engine.process(&mut out);
        }
        for fastest in &mut fastest {
            let started = Instant::now();
            engine.process(&mut out);
            *fastest = (*fastest).min(started.elapsed().as_nanos());
        }
    }
    std::hint::black_box(&out);
    let slowest = fastest.iter().max().copied().unwrap_or(0) as f64 / 1000.0;
    let average = fastest.iter().sum::<u128>() as f64 / (blocks * block) as f64;
    (slowest, average)
}

/// The effects that do their work a part at a time, against a block's budget at 192 kHz: Clouds'
/// phase vocoder, whose FFTs are the most any one frame does.
fn blocks(pool: &Arc<SamplePool>) {
    let rate = 192_000;
    let block = 256;
    let budget = block as f64 / f64::from(rate) * 1e6;
    let voice = Machine::Plaits(SynthVoice::VirtualAnalog);
    let (base_slowest, base_average) = slowest_block(&one(voice, true, |_| {}), pool, rate, block);
    println!();
    println!(
        "The slowest block at {rate} Hz in blocks of {block}, a budget of {budget:.0} us, on a \
         playing VA ({base_slowest:.0} us without, {base_average:.0} ns a frame)"
    );
    println!(
        "{:<26} {:>8} {:>8} {:>8} {:>9}",
        "", "slowest", "added", "% block", "ns/frame"
    );
    for (name, mode, quality) in [
        ("Clouds Granular", CloudsMode::Granular, 0),
        ("Clouds Spectral", CloudsMode::Spectral, 0),
        ("Clouds Spectral, Mono 16", CloudsMode::Spectral, 1),
        ("Clouds Spectral, Stereo 8", CloudsMode::Spectral, 2),
    ] {
        let clouds = Clouds {
            mode,
            quality,
            density: 0xFF,
            ..Clouds::DEFAULT
        };
        let song = one(voice, true, |track| {
            track.inserts[0] = Insert::Clouds(clouds)
        });
        let (slowest, average) = slowest_block(&song, pool, rate, block);
        let added = slowest - base_slowest;
        println!(
            "{name:<26} {slowest:>6.0}us {added:>6.0}us {:>7.1}% {:>9.0}",
            added / budget * 100.0,
            average - base_average
        );
    }
}

/// A large song for [`edits`]: 32 sound tracks, each as [`worst_song`]'s, with a chain of 32
/// phrases of 64 rows, a note on every other row.
fn large_song() -> Song {
    const TRACKS: usize = 32;
    const PHRASES: u32 = 32;
    const ROWS: usize = 64;
    let worst = worst_song();
    let mut song = Song::new(TRACKS);
    for index in 0..TRACKS {
        let track = &mut song.tracks[index];
        let from = &worst.tracks[index % 16];
        track.machine = from.machine;
        track.filter = from.filter;
        track.inserts = from.inserts;
        track.mods = from.mods;
        track.settings = from.settings.clone();
        // A setting of its own on every track, as a song worked on has.
        track.set_setting(SettingId::CUT, 0x90);
        track.set_setting(SettingId::RES, 0x50);
        track.phrases.clear();
        let mut entries = Vec::new();
        for id in 0..PHRASES {
            let mut phrase = Phrase::new(ROWS);
            for row in (0..ROWS).step_by(2) {
                *phrase.step_mut(row).unwrap() =
                    NOTES[(row / 2 + id as usize) % 4].parse().unwrap();
            }
            track.phrases.insert(id, Arc::new(phrase));
            entries.push(ChainEntry {
                phrase: id,
                transpose: 0,
            });
        }
        let label = String::new();
        track.chains.insert(0, Arc::new(Chain { entries, label }));
    }
    song
}

/// The times of some runs, in microseconds, reported as their fastest, their median and their
/// worst: the median, as a run the system put aside for another thread would move an average, and
/// the fastest for a busy machine, where even the median moves.
#[derive(Default, Clone)]
struct Took(Vec<f64>);

impl Took {
    fn add(&mut self, started: Instant) {
        self.0.push(started.elapsed().as_secs_f64() * 1e6);
    }

    fn line(&self, name: &str) -> String {
        let mut sorted = self.0.clone();
        sorted.sort_by(f64::total_cmp);
        let median = sorted.get(sorted.len() / 2).copied().unwrap_or(0.0);
        let fastest = sorted.first().copied().unwrap_or(0.0);
        let worst = sorted.last().copied().unwrap_or(0.0);
        format!("  {name:<30} {fastest:>8.1} us {median:>8.1} us {worst:>8.1} us")
    }
}

/// A kind of edit to time: its name, and the action that's the edit of a number.
type Kind = (&'static str, fn(u32) -> Action);

/// What one edit of a large song takes: on the edit thread, the edit itself, committing it to the
/// undo history, building its version and the room for its generators; then, while playing, the
/// engine taking the version, as the callback does. A turn of the Roto or the wheel makes one of
/// these a frame. The edits are to the last sound track, so whatever compares the tracks in order
/// passes over all the others first.
fn edits(pool: &Arc<SamplePool>) {
    const EDITS: u32 = 400;
    let song = Arc::new(large_song());
    let steps: usize = song
        .tracks
        .iter()
        .flat_map(|track| track.phrases.values())
        .map(|phrase| phrase.len())
        .sum();
    println!();
    println!(
        "One edit of {} sound tracks and {steps} rows, the fastest, the median and the worst of {EDITS}",
        song.sound_tracks()
    );
    const LAST: usize = 31;
    let kinds: [Kind; 2] = [
        ("a turn of track 32's Cut", |edit| Action::SetTrackSetting {
            track: LAST,
            setting: SettingId::CUT,
            value: 0x40 + (edit % 2) as u8,
        }),
        ("a note on track 32", |edit| Action::SetNote {
            track: LAST,
            phrase: 0,
            step: 1,
            note: Note::On(60 + (edit % 2) as u8),
        }),
    ];
    for (name, action) in kinds {
        let mut history = History::new(Arc::clone(&song));
        let mut last = Arc::new(Published::new(Arc::clone(&song), Arc::clone(pool)));
        let mut engine = Engine::new(RATE, Arc::clone(&last));
        engine.play_from(0);
        let mut out = vec![0.0_f32; BLOCK * 2];
        let [
            mut edited,
            mut committed,
            mut published,
            mut room,
            mut whole,
            mut taken,
        ] = std::array::from_fn(|_| Took::default());
        for count in 0..EDITS {
            let action = action(count);
            let started = Instant::now();
            let next = edit(history.current(), &action).unwrap();
            edited.add(started);
            let commit = Instant::now();
            assert!(history.commit(Arc::clone(&next)));
            committed.add(commit);
            let publish = Instant::now();
            let version = Arc::new(Published::after(next, Arc::clone(pool), Some(&last)));
            published.add(publish);
            let scratch = Instant::now();
            let handover = Handover::new(Arc::clone(&version));
            room.add(scratch);
            whole.add(started);
            let take = Instant::now();
            let old = engine.hand_over(handover);
            taken.add(take);
            drop(old);
            last = version;
            engine.process(&mut out);
        }
        std::hint::black_box(&out);
        println!("{name}");
        println!("{}", edited.line("edit"));
        println!("{}", committed.line("History::commit"));
        println!("{}", published.line("Published::after"));
        println!("{}", room.line("Handover::new"));
        println!("{}", whole.line("all four, on the edit thread"));
        println!("{}", taken.line("Engine::hand_over, playing"));
    }
}

fn main() {
    let pool = pool();
    let only: Vec<String> = std::env::args().skip(1).collect();
    if let [save, folder, which @ ..] = only.as_slice()
        && save == "save"
    {
        // The worst case as a project, for `vimtracker bench` and the program itself, or with
        // `large` after the folder, the large song an edit is timed on.
        let large = which.first().is_some_and(|which| which == "large");
        let mut song = if large { large_song() } else { worst_song() };
        song.name = if large { "large" } else { "worst case" }.to_owned();
        vt_model::project::save_song(std::path::Path::new(folder), &song).unwrap();
        return;
    }
    let wanted = |part: &str| only.is_empty() || only.iter().any(|name| name == part);
    println!(
        "At {RATE} Hz in blocks of {BLOCK}, each block's fastest of {RUNS} runs of {SECONDS} s; * \
         marks over 1% of a core ({ONE_PERCENT_NS:.0} ns a frame)"
    );
    if wanted("machines") {
        machines(&pool);
    }
    if wanted("filters") {
        filters(&pool);
    }
    if wanted("effects") {
        slot_effects(&pool);
    }
    if wanted("mods") {
        mod_slots(&pool);
    }
    if wanted("worst") {
        worst(&pool);
    }
    if wanted("blocks") {
        blocks(&pool);
    }
    if wanted("edits") {
        edits(&pool);
    }
}
