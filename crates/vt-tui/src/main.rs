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
        /// Write one file per track, with the output as a folder.
        #[arg(long)]
        stems: bool,
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
    },
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
        Some(Command::Kits {
            command: KitsCommand::ImportAbleton { library, out },
        }) => vt_tui::ableton::run(library, out).map_err(|e| e.to_string()),
        Some(Command::Bench {
            project,
            rate,
            block,
            seconds,
        }) => vt_tui::run::bench(&project, rate, block, seconds).map_err(|e| e.to_string()),
        Some(Command::Render {
            project,
            out,
            rate,
            rows,
            seed,
            tail,
            stems,
        }) => vt_tui::run::render(&vt_tui::run::RenderRequest {
            project,
            out,
            rate,
            rows: rows.map(|rows| rows as usize),
            seed,
            tail,
            stems,
        })
        .map_err(|e| e.to_string()),
        Some(Command::Probe {
            spec,
            out,
            case,
            write_input,
        }) => vt_tui::dsp_probe::run(&vt_tui::dsp_probe::ProbeRequest {
            spec,
            out,
            case,
            write_input,
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
