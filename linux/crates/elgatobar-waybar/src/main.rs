use std::{io, process::ExitCode};

use clap::{Parser, Subcommand};
use elgatobar_waybar::{Action, run_action, watch};

#[derive(Debug, Parser)]
#[command(
    name = "elgatobar-waybar",
    about = "Stateful Waybar integration for the ElgatoBar daemon"
)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Clone, Copy, Debug, Subcommand)]
enum Command {
    /// Continuously emit Waybar custom-module JSON (the default).
    Watch,
    /// Ask the daemon to toggle every online light.
    ToggleAll,
    /// Ask the daemon to refresh every configured light.
    RefreshAll,
}

#[tokio::main]
async fn main() -> ExitCode {
    let command = Args::parse().command.unwrap_or(Command::Watch);
    match command {
        Command::Watch => {
            let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
            tokio::spawn(async move {
                if tokio::signal::ctrl_c().await.is_ok() {
                    let _ = shutdown_tx.send(true);
                }
            });
            match watch(io::stdout().lock(), shutdown_rx).await {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("elgatobar-waybar: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::ToggleAll | Command::RefreshAll => {
            let action = match command {
                Command::ToggleAll => Action::ToggleAll,
                Command::RefreshAll => Action::RefreshAll,
                Command::Watch => unreachable!(),
            };
            match run_action(action).await {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("elgatobar-waybar: {error}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}
