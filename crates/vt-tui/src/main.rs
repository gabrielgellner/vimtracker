use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// A Neovim-style terminal music tracker.
#[derive(Parser)]
#[command(name = "vimtracker", version, about)]
struct Cli {
    /// A project folder to open. It is created on the first save if it doesn't exist.
    project: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Print the key events your terminal delivers, with the kitty keyboard protocol on as the
    /// program has it, and whether the terminal offers it and the kitty graphics protocol.
    ProbeKeys,
    /// List the MIDI and serial ports, and if a Roto-Control is there, connect to it and read its
    /// firmware and API versions, its mode and the setups it stores, changing none of them.
    RotoProbe {
        /// Instead, remove vimtracker's own setups from the Roto, asking first, and nothing else.
        #[arg(long)]
        clear_own: bool,
    },
    /// Render a saved project to a WAV file.
    Render {
        project: PathBuf,
        out: PathBuf,
        /// The sample rate.
        #[arg(long, default_value_t = 48_000)]
        rate: u32,
        /// The length in steps, instead of the section's.
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
        rows: Option<u64>,
        /// A random seed, instead of the project's.
        #[arg(long)]
        seed: Option<u64>,
        /// Let sound ring out after the end, up to 10 seconds.
        #[arg(long)]
        tail: bool,
        /// Write one file per group, and per track in none, with the output as a folder.
        #[arg(long)]
        stems: bool,
        /// Write stems of each track instead, splitting the groups.
        #[arg(long)]
        split: bool,
    },
    /// Play one voice or one effect with no song, offline, from a spec that the reference side in
    /// tools/compare reads too, to a WAV file.
    Probe {
        spec: PathBuf,
        out: PathBuf,
        /// Which of the spec's cases to render, if it has them.
        #[arg(long)]
        case: Option<String>,
        /// Also write an effect's input as the probe made it, to check the reference side's against.
        #[arg(long)]
        write_input: Option<PathBuf>,
        /// Also write the track filter's response as the Sound page draws it, as CSV.
        #[arg(long)]
        response: Option<PathBuf>,
    },
    /// Write one track's notes from a saved project, once through its chain, with the tempo and
    /// its machine's settings, as TOML for tools/compare to play on every side. The project is
    /// only read.
    Notes {
        project: PathBuf,
        out: PathBuf,
        /// The track by its name, instead of the first sound track.
        #[arg(long)]
        track: Option<String>,
        /// The first sound track on this machine, `sampler` or a synth voice's key such as `acid`.
        #[arg(long)]
        machine: Option<String>,
    },
    /// Convert a project saved in format 10 to format 11, once: a nudge on a track not at 1/16
    /// was a 24th of its step, and is now a 24th of a sixteenth. The old files are kept beside
    /// the new as song.format-10.ron and song.autosave.format-10.ron. Run it on a copy.
    #[command(hide = true)]
    ConvertNudges { project: PathBuf },
    /// Make and manage kits.
    Kits {
        #[command(subcommand)]
        command: KitsCommand,
    },
    /// Measure how much of a core a saved project takes to play, as the status line's CPU figure.
    Bench {
        project: PathBuf,
        /// The sample rate.
        #[arg(long, default_value_t = 48_000)]
        rate: u32,
        /// The frames in each block, as the audio device asks for them.
        #[arg(long, default_value_t = 512)]
        block: usize,
        /// How long to play, in seconds of audio.
        #[arg(long, default_value_t = 60)]
        seconds: u32,
        /// Also say how long each track, slot, group, return and the master take while playing.
        #[arg(long)]
        tracks: bool,
        /// Play each block when its time comes rather than flat out, as the audio thread does.
        #[arg(long, conflicts_with = "device")]
        paced: bool,
        /// Play through the output device's callback, with its rate and buffer, sending it only
        /// silence, and split the callback's time into the engine's and the rest.
        #[arg(long)]
        device: bool,
        /// Through the device, the buffer in frames to ask it for, in place of the audio page's.
        #[arg(long, requires = "device")]
        buffer: Option<u32>,
    },
}

#[derive(Subcommand)]
enum KitsCommand {
    /// Write a kit for each of Ableton Live's drum racks, its sounds pointing into Live's
    /// library. Running it again replaces the kits it wrote, and nothing else.
    ImportAbleton {
        /// Live's Core Library folder. The newest Live in /Applications if not given.
        library: Option<PathBuf>,
        /// Where to write the kits, a folder for each category. The first kit folder in
        /// settings.ron, under Ableton, if not given.
        out: Option<PathBuf>,
    },
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Some(Command::ProbeKeys) => vt_tui::probe::run().map_err(|e| e.to_string()),
        Some(Command::RotoProbe { clear_own: true }) => vt_tui::roto::clear_own_setups(
            &mut std::io::stdout().lock(),
            &mut std::io::stdin().lock(),
        )
        .map_err(|e| e.to_string()),
        Some(Command::RotoProbe { clear_own: false }) => vt_roto::probe::run(
            &mut std::io::stdout().lock(),
            vt_roto::link::Options::default().daw,
        )
        .map_err(|e| e.to_string()),
        Some(Command::Kits {
            command: KitsCommand::ImportAbleton { library, out },
        }) => vt_tui::ableton::run(library, out).map_err(|e| e.to_string()),
        Some(Command::Bench {
            project,
            rate,
            block,
            seconds,
            tracks,
            paced,
            device,
            buffer,
        }) => vt_tui::bench::run(&vt_tui::bench::BenchRequest {
            project,
            rate,
            block,
            seconds,
            pace: if device {
                vt_tui::bench::Pace::Device
            } else if paced {
                vt_tui::bench::Pace::Paced
            } else {
                vt_tui::bench::Pace::FlatOut
            },
            tracks,
            buffer,
        })
        .map_err(|e| e.to_string()),
        Some(Command::Render {
            project,
            out,
            rate,
            rows,
            seed,
            tail,
            stems,
            split,
        }) => vt_tui::run::render(&vt_tui::run::RenderRequest {
            project,
            out,
            rate,
            rows: rows.map(|rows| rows as usize),
            seed,
            tail,
            stems,
            split,
        })
        .map_err(|e| e.to_string()),
        Some(Command::Probe {
            spec,
            out,
            case,
            write_input,
            response,
        }) => vt_tui::dsp_probe::run(&vt_tui::dsp_probe::ProbeRequest {
            spec,
            out,
            case,
            write_input,
            response,
        })
        .map_err(|e| e.to_string()),
        Some(Command::Notes {
            project,
            out,
            track,
            machine,
        }) => vt_tui::track_notes::run(&vt_tui::track_notes::NotesRequest {
            project,
            out,
            choice: vt_tui::track_notes::Choice { track, machine },
        })
        .map_err(|e| e.to_string()),
        Some(Command::ConvertNudges { project }) => vt_model::convert::convert_project(&project)
            .map(|done| {
                println!(
                    "{}: {} nudges converted, {} of them kept to their track's range, and {} \
                     live Nudge generators",
                    project.display(),
                    done.nudges,
                    done.clamped,
                    done.generators
                );
            })
            .map_err(|e| e.to_string()),
        None => vt_tui::run::run(cli.project).map_err(|e| e.to_string()),
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("vimtracker: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
