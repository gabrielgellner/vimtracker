//! `vimtracker kits import-ableton`: a kit file for each of Ableton Live's drum racks, its sounds
//! pointing into Live's library where they are, so nothing is copied.
//!
//! A rack, an `.adg` file, is gzipped XML. Each pad is a `DrumBranchPreset`, with the note it
//! answers to, its choke group, and the instruments that play it: Simpler (`OriginalSimpler`),
//! Sampler (`MultiSampler`) or Drum Sampler (`DrumCell`), often inside an instrument rack whose
//! chains split it by key, velocity or a chain selector.

use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use roxmltree::{Document, Node};
use thiserror::Error;
use vt_model::kit::{CHOKE_GROUPS, KitFile, MAX_TUNE, SoundFile};

/// Where Live keeps its racks, inside its library.
const RACKS: &str = "Racks/Drum Racks";
/// What every file the importer writes starts with, so that it only ever replaces its own.
const MARK: &str = "// Made by vimtracker kits import-ableton";
/// The library the kits say they came from.
const SOURCE: &str = "Ableton Core Library";
/// Simpler's and Drum Sampler's volume as they start, which a kit's gain counts from.
const DEFAULT_VOLUME_DB: f64 = -12.0;
/// The note a pad sends its instrument unless it says otherwise, Live's C3.
const DEFAULT_SENDING_NOTE: i32 = 60;

/// Why the import couldn't run at all. A rack that won't read is reported, and the rest go on.
#[derive(Debug, Error)]
pub enum ImportError {
    #[error("no Ableton Live library found in /Applications: give its Core Library folder")]
    NoLibrary,
    #[error("{0} has no Racks/Drum Racks folder: is it Live's Core Library?")]
    NotALibrary(String),
    #[error("no kit folder in settings.ron: give a folder to write to")]
    NoKitFolder,
    #[error("{path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
}

/// What an import did.
#[derive(Debug, Default, PartialEq)]
pub struct Report {
    /// Kit files written.
    pub kits: usize,
    /// Sounds in them.
    pub sounds: usize,
    /// Pads with no sample, such as a synth's, which are left out.
    pub skipped_pads: usize,
    /// Samples a rack names that aren't in the library, by rack and path.
    pub missing: Vec<String>,
    /// Racks that wouldn't read, and why.
    pub failed: Vec<String>,
    /// Racks none of whose pads play a sample, such as those of synths, which are left out.
    pub empty: Vec<PathBuf>,
    /// Files in the way that the importer didn't make, so it left them.
    pub kept: Vec<PathBuf>,
    /// Files it made last time for racks that are no longer there.
    pub removed: Vec<PathBuf>,
}

/// The newest Live in `/Applications`, by its version and then Suite before Standard and the
/// rest, as its Core Library folder.
pub fn default_library() -> Option<PathBuf> {
    let entries = fs::read_dir("/Applications").ok()?;
    let mut lives: Vec<(u32, u32, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let rest = name.strip_prefix("Ableton Live ")?.strip_suffix(".app")?;
            let version: u32 = rest.split_whitespace().next()?.parse().ok()?;
            let edition = match rest.split_whitespace().nth(1) {
                Some("Suite") => 3,
                Some("Standard") => 2,
                Some(_) => 1,
                None => 0,
            };
            let library = entry.path().join("Contents/App-Resources/Core Library");
            library.is_dir().then_some((version, edition, library))
        })
        .collect();
    lives.sort();
    lives.pop().map(|(_, _, library)| library)
}

/// The `Samples` folder of the Live that `default_library` finds, with its `Loops`, `One Shots`
/// and `Multisamples`, which the imported kits play from.
pub fn default_samples() -> Option<PathBuf> {
    default_library()
        .map(|library| library.join("Samples"))
        .filter(|samples| samples.is_dir())
}

/// The import as the command runs it: Live's library and the first kit folder's `Ableton`
/// unless they're given, and what it did, printed.
pub fn run(library: Option<PathBuf>, out: Option<PathBuf>) -> Result<(), ImportError> {
    let library = library
        .or_else(default_library)
        .ok_or(ImportError::NoLibrary)?;
    let out = match out {
        Some(out) => out,
        None => {
            let settings = crate::config::Settings::path()
                .map(|path| crate::config::Settings::load(&path).0)
                .unwrap_or_default();
            let first = settings.kit_folders().into_iter().next();
            first.ok_or(ImportError::NoKitFolder)?.join("Ableton")
        }
    };
    let report = import(&library, &out)?;
    println!(
        "{} kits, {} sounds, written to {}",
        report.kits,
        report.sounds,
        out.display()
    );
    println!(
        "{} pads left out as they play no sample",
        report.skipped_pads
    );
    for (what, list) in [
        ("missing from the library", &report.missing),
        ("couldn't be read", &report.failed),
    ] {
        if !list.is_empty() {
            println!("{} {what}:", list.len());
            for item in list {
                println!("  {item}");
            }
        }
    }
    for (what, list) in [
        (
            "racks left out, as none of their pads play a sample",
            &report.empty,
        ),
        (
            "files left alone, as the importer didn't make them",
            &report.kept,
        ),
        ("files removed, as their racks have gone", &report.removed),
    ] {
        if !list.is_empty() {
            println!("{} {what}:", list.len());
            for path in list {
                println!("  {}", path.display());
            }
        }
    }
    Ok(())
}

/// Read every rack under the library's `Racks/Drum Racks` and write a kit for each into `out`,
/// in a folder for its category: `out/Drum Machines/909 Core Kit.ron`. A file the importer
/// made before is replaced, or removed if its rack has gone, and any other file is left alone.
pub fn import(library: &Path, out: &Path) -> Result<Report, ImportError> {
    let racks_folder = library.join(RACKS);
    if !racks_folder.is_dir() {
        return Err(ImportError::NotALibrary(library.display().to_string()));
    }
    let mut racks = Vec::new();
    find_racks(&racks_folder, &mut racks);
    racks.sort();
    let mut report = Report::default();
    let mut written = BTreeSet::new();
    for rack in &racks {
        let relative = rack.strip_prefix(&racks_folder).unwrap_or(rack);
        let category = relative
            .parent()
            .map(|folder| folder.to_string_lossy().into_owned())
            .unwrap_or_default();
        let kit = match read_rack_file(rack, library, &category, &mut report) {
            Ok(kit) => kit,
            Err(detail) => {
                report.failed.push(format!("{}: {detail}", rack.display()));
                continue;
            }
        };
        if kit.sounds.is_empty() {
            report.empty.push(rack.clone());
            continue;
        }
        let target = out.join(relative).with_extension("ron");
        if target.exists() && !made_here(&target) {
            report.kept.push(target);
            continue;
        }
        write_kit(&target, &kit, relative)?;
        report.kits += 1;
        report.sounds += kit.sounds.len();
        written.insert(target);
    }
    remove_stale(out, &written, &mut report)?;
    Ok(report)
}

fn find_racks(folder: &Path, racks: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for path in entries.flatten().map(|entry| entry.path()) {
        if path.is_dir() {
            find_racks(&path, racks);
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("adg"))
        {
            racks.push(path);
        }
    }
}

fn io_at(path: &Path) -> impl FnOnce(std::io::Error) -> ImportError + '_ {
    move |source| ImportError::Io {
        path: path.display().to_string(),
        source,
    }
}

/// Whether a file starts with the importer's mark.
fn made_here(path: &Path) -> bool {
    fs::read_to_string(path).is_ok_and(|text| text.starts_with(MARK))
}

fn write_kit(target: &Path, kit: &KitFile, rack: &Path) -> Result<(), ImportError> {
    if let Some(folder) = target.parent() {
        fs::create_dir_all(folder).map_err(io_at(folder))?;
    }
    let pretty = ron::ser::PrettyConfig::new()
        .depth_limit(4)
        .indentor("    ");
    let body = ron::ser::to_string_pretty(kit, pretty).map_err(|error| ImportError::Io {
        path: target.display().to_string(),
        source: std::io::Error::other(error),
    })?;
    let text = format!(
        "{MARK} from {}.\n// Running it again replaces this file, so copy it elsewhere to change it.\n{body}\n",
        rack.display()
    );
    fs::write(target, text).map_err(io_at(target))
}

/// Remove what the importer made before that it didn't make this time.
fn remove_stale(
    folder: &Path,
    written: &BTreeSet<PathBuf>,
    report: &mut Report,
) -> Result<(), ImportError> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Ok(());
    };
    for path in entries.flatten().map(|entry| entry.path()) {
        if path.is_dir() {
            remove_stale(&path, written, report)?;
        } else if path.extension().is_some_and(|extension| extension == "ron")
            && !written.contains(&path)
            && made_here(&path)
        {
            fs::remove_file(&path).map_err(io_at(&path))?;
            report.removed.push(path);
        }
    }
    Ok(())
}

fn read_rack_file(
    rack: &Path,
    library: &Path,
    category: &str,
    report: &mut Report,
) -> Result<KitFile, String> {
    let bytes = fs::read(rack).map_err(|error| error.to_string())?;
    let mut text = String::new();
    GzDecoder::new(&bytes[..])
        .read_to_string(&mut text)
        .map_err(|error| format!("not a gzipped rack: {error}"))?;
    let name = rack
        .file_stem()
        .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
    let (kit, rack_report) = read_rack(&text, library, &name, category)?;
    report.skipped_pads += rack_report.skipped_pads;
    report.missing.extend(
        rack_report
            .missing
            .into_iter()
            .map(|path| format!("{}: {path}", rack.display())),
    );
    Ok(kit)
}

/// What reading one rack passed over.
#[derive(Debug, Default, PartialEq)]
pub struct RackReport {
    pub skipped_pads: usize,
    /// Samples it names that aren't in the library, as it names them.
    pub missing: Vec<String>,
}

/// Make a kit from a rack's XML, its sounds in the order of their pads' notes.
pub fn read_rack(
    xml: &str,
    library: &Path,
    name: &str,
    category: &str,
) -> Result<(KitFile, RackReport), String> {
    let document = Document::parse(xml).map_err(|error| error.to_string())?;
    let mut pads = Vec::new();
    find_pads(document.root(), &mut pads);
    let mut report = RackReport::default();
    let mut sounds: Vec<(i32, SoundFile)> = Vec::new();
    for pad in pads {
        match read_pad(pad, library, &mut report.missing) {
            Some(sound) => sounds.push(sound),
            None => report.skipped_pads += 1,
        }
    }
    sounds.sort_by_key(|(note, _)| *note);
    let mut taken = BTreeSet::new();
    let sounds = sounds
        .into_iter()
        .map(|(_, mut sound)| {
            // Each label once, on its lowest pad, so a second kick is known by its name alone.
            if !taken.insert(sound.label.clone()) {
                sound.label.clear();
            }
            sound
        })
        .collect();
    let kit = KitFile {
        name: name.to_owned(),
        source: SOURCE.to_owned(),
        category: category.to_owned(),
        sounds,
    };
    Ok((kit, report))
}

/// The pads of the outermost drum rack, wherever it is.
fn find_pads<'a, 'input>(node: Node<'a, 'input>, pads: &mut Vec<Node<'a, 'input>>) {
    for child in node.children().filter(Node::is_element) {
        if child.has_tag_name("DrumBranchPreset") {
            pads.push(child);
        } else {
            find_pads(child, pads);
        }
    }
}

fn child<'a, 'input>(node: Node<'a, 'input>, tag: &str) -> Option<Node<'a, 'input>> {
    node.children().find(|child| child.has_tag_name(tag))
}

/// An element's `Value`, or its `Manual` child's, as a parameter keeps it.
fn value<'a>(node: Node<'a, '_>) -> Option<&'a str> {
    node.attribute("Value")
        .or_else(|| child(node, "Manual").and_then(|manual| manual.attribute("Value")))
}

fn number(node: Node, tag: &str) -> Option<f64> {
    child(node, tag)
        .and_then(value)
        .and_then(|v| v.parse().ok())
}

/// A key, velocity or selector range, from its `Min` and `Max`.
type Range = (i32, i32);
const FULL: Range = (0, 127);

fn range(node: Node, tag: &str) -> Option<Range> {
    let range = child(node, tag)?;
    let end = |tag| number(range, tag).map(|v| v as i32);
    Some((end("Min")?, end("Max")?))
}

fn meet(a: Range, b: Range) -> Range {
    (a.0.max(b.0), a.1.min(b.1))
}

/// One sample a pad can play, and when.
#[derive(Debug, Clone)]
struct Part {
    path: PathBuf,
    keys: Range,
    velocities: Range,
    /// The chain selectors and the sample selector it's under, outermost first.
    selectors: Vec<Range>,
    /// The note it plays at as recorded.
    root: i32,
    volume_db: f64,
    transpose: f64,
}

/// The chain ranges above a part.
#[derive(Debug, Clone, Default)]
struct Chain {
    keys: Option<Range>,
    velocities: Option<Range>,
    selectors: Vec<Range>,
}

fn find_parts(
    node: Node,
    chain: &Chain,
    library: &Path,
    parts: &mut Vec<Part>,
    missing: &mut Vec<String>,
) {
    for element in node.children().filter(Node::is_element) {
        match element.tag_name().name() {
            // A drum rack inside a pad is a kit of its own, and isn't this pad's sound.
            "DrumBranchPreset" => {}
            "InstrumentBranchPreset" => {
                let zone = child(element, "ZoneSettings");
                let within = |outer: Option<Range>, inner: Option<Range>| match (outer, inner) {
                    (Some(a), Some(b)) => Some(meet(a, b)),
                    (a, b) => a.or(b),
                };
                let mut inner = chain.clone();
                inner.keys = within(chain.keys, zone.and_then(|z| range(z, "KeyRange")));
                inner.velocities = within(
                    chain.velocities,
                    zone.and_then(|z| range(z, "VelocityRange")),
                );
                inner
                    .selectors
                    .push(range(element, "BranchSelectorRange").unwrap_or(FULL));
                find_parts(element, &inner, library, parts, missing);
            }
            "OriginalSimpler" | "MultiSampler" => {
                let volume = descendant(element, &["VolumeAndPan", "Volume"])
                    .and_then(value)
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(DEFAULT_VOLUME_DB);
                let pitch = child(element, "Pitch");
                let transpose = pitch.and_then(|p| number(p, "TransposeKey")).unwrap_or(0.0)
                    + pitch
                        .and_then(|p| number(p, "TransposeFine"))
                        .unwrap_or(0.0)
                        / 100.0;
                for part in element
                    .descendants()
                    .filter(|node| node.has_tag_name("MultiSamplePart"))
                {
                    let Some(path) = sample(part, library, missing) else {
                        continue;
                    };
                    let mut selectors = chain.selectors.clone();
                    selectors.push(range(part, "SelectorRange").unwrap_or(FULL));
                    parts.push(Part {
                        path,
                        keys: meet(
                            chain.keys.unwrap_or(FULL),
                            range(part, "KeyRange").unwrap_or(FULL),
                        ),
                        velocities: meet(
                            chain.velocities.unwrap_or(FULL),
                            range(part, "VelocityRange").unwrap_or(FULL),
                        ),
                        selectors,
                        root: number(part, "RootKey").map_or(DEFAULT_SENDING_NOTE, |v| v as i32),
                        volume_db: volume,
                        transpose,
                    });
                }
            }
            "DrumCell" => {
                let Some(path) = child(element, "UserSample")
                    .and_then(|sample_node| sample(sample_node, library, missing))
                else {
                    continue;
                };
                parts.push(Part {
                    path,
                    keys: chain.keys.unwrap_or(FULL),
                    velocities: chain.velocities.unwrap_or(FULL),
                    selectors: chain.selectors.clone(),
                    root: DEFAULT_SENDING_NOTE,
                    volume_db: number(element, "Volume").unwrap_or(DEFAULT_VOLUME_DB),
                    transpose: number(element, "Voice_Transpose").unwrap_or(0.0),
                });
            }
            _ => find_parts(element, chain, library, parts, missing),
        }
    }
}

fn descendant<'a, 'input>(node: Node<'a, 'input>, tags: &[&str]) -> Option<Node<'a, 'input>> {
    tags.iter().try_fold(node, |node, tag| child(node, tag))
}

/// The sample a part or Drum Sampler names, if it's in the library.
fn sample(node: Node, library: &Path, missing: &mut Vec<String>) -> Option<PathBuf> {
    let relative = node
        .descendants()
        .find(|n| n.has_tag_name("SampleRef"))
        .and_then(|reference| descendant(reference, &["FileRef", "RelativePath"]))
        .and_then(value)
        .filter(|relative| !relative.is_empty())?;
    let path = library.join(relative);
    if path.is_file() {
        Some(path)
    } else {
        missing.push(relative.to_owned());
        None
    }
}

/// A pad's sound and its note, or nothing if it plays no sample.
fn read_pad(pad: Node, library: &Path, missing: &mut Vec<String>) -> Option<(i32, SoundFile)> {
    let zone = child(pad, "ZoneSettings");
    let setting = |tag| zone.and_then(|zone| number(zone, tag));
    // Live keeps the pad's note counted down from 128.
    let note = setting("ReceivingNote").map_or(0, |v| 128 - v as i32);
    let sending = setting("SendingNote").map_or(DEFAULT_SENDING_NOTE, |v| v as i32);
    let choke = setting("ChokeGroup")
        .map(|v| v as i64)
        .filter(|group| (1..=i64::from(CHOKE_GROUPS)).contains(group))
        .map(|group| group as u8);
    let mut parts = Vec::new();
    if let Some(devices) = child(pad, "DevicePresets") {
        find_parts(devices, &Chain::default(), library, &mut parts, missing);
    }
    if parts.is_empty() {
        return None;
    }
    let sorted = sort_parts(&parts, sending);
    let main = sorted.main;
    let named = child(pad, "Name")
        .and_then(value)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .or_else(|| instrument_name(pad));
    let text = |path: &Path| path.to_string_lossy().into_owned();
    let stem = main
        .path
        .file_stem()
        .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
    let name = named.unwrap_or(stem);
    let folder = main
        .path
        .strip_prefix(library)
        .unwrap_or(&main.path)
        .to_string_lossy()
        .into_owned();
    let label = guess_label(&name)
        .or_else(|| guess_label(&folder))
        .unwrap_or_default();
    let mixer = descendant(pad, &["MixerPreset"])
        .and_then(|mixer| {
            mixer
                .descendants()
                .find(|n| n.has_tag_name("AudioBranchMixerDevice"))
        })
        .and_then(|device| number(device, "Volume"))
        .unwrap_or(1.0);
    let gain = main.volume_db - DEFAULT_VOLUME_DB + 20.0 * mixer.max(0.001).log10();
    let tune = (main.transpose + f64::from(sending - main.root)).round();
    let tune = tune.clamp(-f64::from(MAX_TUNE), f64::from(MAX_TUNE)) as i8;
    let sound = SoundFile {
        label,
        name,
        path: text(&main.path),
        variants: sorted.variants.iter().map(|path| text(path)).collect(),
        layers: sorted.layers.iter().map(|path| text(path)).collect(),
        choke,
        gain: ((gain * 10.0).round() / 10.0) as f32,
        tune,
    };
    Some((note, sound))
}

/// The name of the first instrument in a pad, as Live shows it on the pad.
fn instrument_name(pad: Node) -> Option<String> {
    let devices = child(pad, "DevicePresets")?;
    devices
        .descendants()
        .filter(|node| {
            [
                "InstrumentGroupDevice",
                "OriginalSimpler",
                "MultiSampler",
                "DrumCell",
            ]
            .iter()
            .any(|tag| node.has_tag_name(*tag))
        })
        .find_map(|device| {
            child(device, "UserName")
                .and_then(value)
                .filter(|name| !name.is_empty())
        })
        .map(str::to_owned)
}

/// A pad's samples sorted into the one it plays, its velocity layers and its other tones.
struct Sorted<'a> {
    main: &'a Part,
    layers: Vec<PathBuf>,
    variants: Vec<PathBuf>,
}

/// The pad plays the zone of its samples that covers the note it sends, at the lowest of each
/// selector: the hardest of that zone's velocity layers is its sound, and the layers, soft to
/// hard, are kept when there's more than one. Every other zone, another key or selector, gives
/// its hardest layer as a variant, as do samples stacked on the main one.
fn sort_parts(parts: &[Part], sending: i32) -> Sorted<'_> {
    type Zone<'a> = (Range, &'a [Range]);
    let mut zones: Vec<(Zone, Vec<&Part>)> = Vec::new();
    for part in parts {
        let zone = (part.keys, part.selectors.as_slice());
        match zones.iter_mut().find(|(z, _)| *z == zone) {
            Some((_, members)) => members.push(part),
            None => zones.push((zone, vec![part])),
        }
    }
    let lowest = |selectors: &[Range]| selectors.iter().map(|s| s.0).collect::<Vec<_>>();
    let plays = |keys: Range| keys.0 <= sending && sending <= keys.1;
    let main_zone = zones
        .iter()
        .enumerate()
        .filter(|(_, ((keys, _), _))| plays(*keys))
        .min_by_key(|(index, ((_, selectors), _))| (lowest(selectors), *index))
        .map_or(0, |(index, _)| index);
    let hardest = |members: &[&Part]| -> usize {
        members
            .iter()
            .enumerate()
            .max_by_key(|(index, part)| (part.velocities.1, part.velocities.0, usize::MAX - index))
            .map_or(0, |(index, _)| index)
    };
    let (_, members) = &zones[main_zone];
    let main = members[hardest(members)];
    let mut by_velocity: Vec<&Part> = members.clone();
    by_velocity.sort_by_key(|part| (part.velocities.0, part.velocities.1));
    let velocities: BTreeSet<Range> = members.iter().map(|part| part.velocities).collect();
    let mut layers: Vec<PathBuf> = Vec::new();
    let mut variants: Vec<PathBuf> = Vec::new();
    let mut seen: BTreeSet<&Path> = BTreeSet::from([main.path.as_path()]);
    if velocities.len() > 1 {
        for part in &by_velocity {
            if !layers.contains(&part.path) {
                layers.push(part.path.clone());
            }
        }
        if layers.len() < 2 {
            layers.clear();
        }
        seen.extend(layers.iter().map(PathBuf::as_path));
    }
    // Samples sharing the main one's velocities sound together with it, so they're other tones.
    for part in members
        .iter()
        .filter(|part| part.velocities == main.velocities)
    {
        if seen.insert(part.path.as_path()) {
            variants.push(part.path.clone());
        }
    }
    for (index, (_, others)) in zones.iter().enumerate() {
        if index == main_zone {
            continue;
        }
        let part = others[hardest(others)];
        if seen.insert(part.path.as_path()) {
            variants.push(part.path.clone());
        }
    }
    Sorted {
        main,
        layers,
        variants,
    }
}

/// The Rytm's name for a drum, guessed from words in a pad's name or its sample's path.
pub fn guess_label(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    let has = |options: &[&str]| words.iter().any(|word| options.contains(word));
    let label = if has(&["hihat", "hihats", "hat", "hats", "hh"]) || lower.contains("hi-hat") {
        if has(&["open", "oh"]) { "OH" } else { "CH" }
    } else if has(&["ride"]) {
        "RD"
    } else if has(&["crash", "cymbal", "cymbals", "cym", "china", "splash"]) {
        "CY"
    } else if has(&["cowbell", "cowbells"]) {
        "CB"
    } else if has(&["clap", "claps", "handclap"]) {
        "CP"
    } else if has(&["rim", "rimshot", "rims"]) {
        "RS"
    } else if has(&["snare", "snares", "sd"]) {
        "SD"
    } else if has(&["tom", "toms"]) {
        if has(&["low", "lo", "floor"]) {
            "LT"
        } else if has(&["mid", "md"]) {
            "MT"
        } else if has(&["hi", "high"]) {
            "HT"
        } else {
            return None;
        }
    } else if has(&["kick", "kicks", "bd"]) || lower.contains("bass drum") {
        "BD"
    } else {
        return None;
    };
    Some(label.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_guessed_from_drum_words() {
        for (text, label) in [
            ("Bass Drum", Some("BD")),
            ("Kick 909 Tune11.wav", Some("BD")),
            ("Snare Drum", Some("SD")),
            ("Rim Shot", Some("RS")),
            ("Hand Clap", Some("CP")),
            (
                "Samples/One Shots/Drums/Hihat/Hihat Closed 808.aif",
                Some("CH"),
            ),
            ("Open Hat", Some("OH")),
            ("Hi-Hat Open", Some("OH")),
            ("Low Tom", Some("LT")),
            ("Tom 909 Mid", Some("MT")),
            ("Hi Tom", Some("HT")),
            ("Crash 909", Some("CY")),
            ("Ride", Some("RD")),
            ("Cowbell 808", Some("CB")),
            ("Tom", None),
            ("Shaker", None),
            ("Samples/One Shots/Drums/Perc/Perc Thing.wav", None),
            ("That Feel", None),
        ] {
            assert_eq!(guess_label(text).as_deref(), label, "{text}");
        }
    }
}
