use std::{process::ExitCode, time::Duration};

use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "elgatobar-daemon",
    version,
    about = "ElgatoBar user-session daemon"
)]
struct Args {
    /// The single light endpoint managed by this milestone.
    #[arg(long, env = "ELGATOBAR_ENDPOINT")]
    endpoint: String,

    /// Device HTTP timeout in milliseconds.
    #[arg(long, default_value_t = 5_000)]
    timeout_ms: u64,

    /// Device polling interval in seconds.
    #[arg(long, default_value_t = 5)]
    poll_interval_seconds: u64,
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = Args::parse();
    if args.timeout_ms == 0 || args.poll_interval_seconds == 0 {
        eprintln!("timeout and poll interval must be greater than zero");
        return ExitCode::from(2);
    }
    match elgatobar_daemon::serve(
        &args.endpoint,
        Duration::from_millis(args.timeout_ms),
        Duration::from_secs(args.poll_interval_seconds),
    )
    .await
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("elgatobar-daemon: {error}");
            ExitCode::FAILURE
        }
    }
}
