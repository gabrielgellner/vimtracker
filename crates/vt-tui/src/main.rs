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
    /// Print the key events your terminal delivers.
    ProbeKeys,
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Some(Command::ProbeKeys) => vt_tui::probe::run().map_err(|e| e.to_string()),
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
