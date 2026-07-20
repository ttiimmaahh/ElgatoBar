use std::{path::PathBuf, process::ExitCode, time::Duration};

use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "elgatobar-daemon",
    version,
    about = "ElgatoBar user-session daemon"
)]
struct Args {
    /// Device HTTP timeout in milliseconds.
    #[arg(long, default_value_t = 5_000)]
    timeout_ms: u64,

    /// Override the XDG data root (primarily for isolated testing).
    #[arg(long, hide = true)]
    data_root: Option<PathBuf>,

    /// Override the XDG config root (primarily for isolated testing).
    #[arg(long, hide = true)]
    config_root: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = Args::parse();
    if args.timeout_ms == 0 {
        eprintln!("timeout must be greater than zero");
        return ExitCode::from(2);
    }
    let paths = match (args.data_root, args.config_root) {
        (Some(data), Some(config)) => elgatobar_daemon::StoragePaths::with_roots(data, config),
        (None, None) => match elgatobar_daemon::StoragePaths::discover() {
            Ok(paths) => paths,
            Err(error) => {
                eprintln!("elgatobar-daemon: {error}");
                return ExitCode::FAILURE;
            }
        },
        _ => {
            eprintln!("--data-root and --config-root must be supplied together");
            return ExitCode::from(2);
        }
    };
    let settings_path = paths.settings_file();
    let settings = match elgatobar_daemon::load_settings(&settings_path) {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("elgatobar-daemon: {error}");
            return ExitCode::FAILURE;
        }
    };
    if !settings_path.exists()
        && let Err(error) = elgatobar_daemon::save_settings(&settings_path, &settings)
    {
        eprintln!("elgatobar-daemon: {error}");
        return ExitCode::FAILURE;
    }
    match elgatobar_daemon::serve(paths, Duration::from_millis(args.timeout_ms), settings).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("elgatobar-daemon: {error}");
            ExitCode::FAILURE
        }
    }
}
