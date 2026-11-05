use clap::{Parser, Subcommand};

/// A Neovim-style terminal music tracker.
#[derive(Parser)]
#[command(name = "vimtracker", version, about)]
struct Cli {
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
        Some(Command::ProbeKeys) => vt_tui::probe::run(),
        None => Ok(()),
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("vimtracker: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
