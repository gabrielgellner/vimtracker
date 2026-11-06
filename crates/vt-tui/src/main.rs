use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// A Neovim-style terminal music tracker.
#[derive(Parser)]
#[command(name = "vimtracker", version, about)]
struct Cli {
    /// A project folder to open. It is created on the first save if it doesn't exist.
    project: Option<PathBuf>,
    /// Draw in Braille, instead of with images where the terminal can show them.
    #[arg(long)]
    no_graphics: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Print the key events your terminal delivers.
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
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Some(Command::ProbeKeys) => vt_tui::probe::run().map_err(|e| e.to_string()),
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
        None => vt_tui::run::run(cli.project, cli.no_graphics).map_err(|e| e.to_string()),
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("vimtracker: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
