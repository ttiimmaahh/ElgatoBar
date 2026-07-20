use std::process::ExitCode;

use clap::{Args, Error as ClapError, Parser, Subcommand};
use elgatobar_core::{Brightness, ElgatoTemperature};
use elgatobar_dbus::{AccessorySnapshot, ElgatoBarProxy, LightSnapshot};
use serde::Serialize;

const EXIT_INPUT: u8 = 2;
const EXIT_CONNECTIVITY: u8 = 3;
const EXIT_PROTOCOL: u8 = 4;

#[derive(Debug, Parser)]
#[command(
    name = "elgatobar",
    version,
    about = "Control the ElgatoBar user daemon",
    after_help = "The daemon must own io.github.ttiimmaahh.ElgatoBar1 on the user session bus."
)]
struct Cli {
    /// Emit one JSON document on stdout (and structured errors on stderr).
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Read accessory information from the configured light.
    Info,
    /// Read the daemon's latest snapshot without polling the light.
    State,
    /// Poll the configured light now and return the new snapshot.
    Refresh,
    /// Change one or more state fields while preserving the others.
    Set(SetArgs),
    /// Toggle power while preserving brightness and temperature.
    Toggle,
    /// Flash the configured physical light for identification.
    Identify,
}

#[derive(Debug, Args)]
struct SetArgs {
    /// Turn the light on.
    #[arg(long, conflicts_with = "off")]
    on: bool,

    /// Turn the light off.
    #[arg(long, conflicts_with = "on")]
    off: bool,

    /// Brightness percentage (3 through 100).
    #[arg(long)]
    brightness: Option<u8>,

    /// Native Elgato temperature value (143 through 344).
    #[arg(long, conflicts_with = "kelvin")]
    temperature: Option<u16>,

    /// Color temperature in Kelvin (clamped to 2900 through 7000).
    #[arg(long, conflicts_with = "temperature")]
    kelvin: Option<u32>,
}

#[derive(Debug)]
struct AppError {
    kind: ErrorKind,
    message: String,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
enum ErrorKind {
    InvalidInput,
    Connectivity,
    Protocol,
}

impl AppError {
    fn input(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::InvalidInput,
            message: message.into(),
        }
    }

    fn dbus(error: zbus::Error) -> Self {
        if let zbus::Error::MethodError(name, detail, _) = &error {
            let kind = if name.as_str().ends_with(".InvalidInput") {
                ErrorKind::InvalidInput
            } else if name.as_str().ends_with(".Connectivity") {
                ErrorKind::Connectivity
            } else if name.as_str().ends_with(".Protocol") {
                ErrorKind::Protocol
            } else {
                ErrorKind::Connectivity
            };
            return Self {
                kind,
                message: detail.clone().unwrap_or_else(|| error.to_string()),
            };
        }
        Self {
            kind: ErrorKind::Connectivity,
            message: format!("ElgatoBar daemon is unavailable: {error}"),
        }
    }

    fn protocol(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Protocol,
            message: message.into(),
        }
    }

    fn exit_code(&self) -> u8 {
        match self.kind {
            ErrorKind::InvalidInput => EXIT_INPUT,
            ErrorKind::Connectivity => EXIT_CONNECTIVITY,
            ErrorKind::Protocol => EXIT_PROTOCOL,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum Output {
    AccessoryInfo { accessory: AccessorySnapshot },
    State { state: LightSnapshot },
    Identified,
}

#[derive(Serialize)]
struct ErrorEnvelope<'a> {
    error: ErrorBody<'a>,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    kind: ErrorKind,
    message: &'a str,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = match parse_cli() {
        Ok(cli) => cli,
        Err(exit_code) => return exit_code,
    };
    let json = cli.json;
    match run(cli).await {
        Ok(result) => {
            print_result(&result, json);
            ExitCode::SUCCESS
        }
        Err(error) => {
            print_error(&error, json);
            ExitCode::from(error.exit_code())
        }
    }
}

fn parse_cli() -> Result<Cli, ExitCode> {
    let json_requested = std::env::args_os().any(|argument| argument == "--json");
    match Cli::try_parse() {
        Ok(cli) => Ok(cli),
        Err(error) => Err(report_parse_error(error, json_requested)),
    }
}

fn report_parse_error(error: ClapError, json_requested: bool) -> ExitCode {
    if error.exit_code() == 0 {
        let _ = error.print();
        return ExitCode::SUCCESS;
    }
    if json_requested {
        print_error(&AppError::input(error.to_string()), true);
    } else {
        let _ = error.print();
    }
    ExitCode::from(EXIT_INPUT)
}

async fn run(cli: Cli) -> Result<Output, AppError> {
    let connection = zbus::Connection::session().await.map_err(AppError::dbus)?;
    let proxy = ElgatoBarProxy::new(&connection)
        .await
        .map_err(AppError::dbus)?;
    match cli.command {
        Command::Info => proxy
            .accessory_info()
            .await
            .map(|accessory| Output::AccessoryInfo { accessory })
            .map_err(AppError::dbus),
        Command::State => proxy
            .snapshot()
            .await
            .map_err(AppError::dbus)
            .and_then(state_output),
        Command::Refresh => proxy
            .refresh()
            .await
            .map_err(AppError::dbus)
            .and_then(state_output),
        Command::Toggle => proxy
            .toggle()
            .await
            .map_err(AppError::dbus)
            .and_then(state_output),
        Command::Identify => proxy
            .identify()
            .await
            .map(|()| Output::Identified)
            .map_err(AppError::dbus),
        Command::Set(args) => {
            let has_power = args.on || args.off;
            let power = args.on;
            let brightness = args
                .brightness
                .map(Brightness::try_from)
                .transpose()
                .map_err(|error| AppError::input(error.to_string()))?
                .map_or(0, Brightness::get);
            let temperature = args
                .temperature
                .map(ElgatoTemperature::try_from)
                .transpose()
                .map_err(|error| AppError::input(error.to_string()))?
                .or_else(|| args.kelvin.map(ElgatoTemperature::from_kelvin))
                .map_or(0, ElgatoTemperature::get);
            if !has_power && brightness == 0 && temperature == 0 {
                return Err(AppError::input(
                    "set requires at least one of --on, --off, --brightness, --temperature, or --kelvin",
                ));
            }
            proxy
                .set_state(has_power, power, brightness, temperature)
                .await
                .map_err(AppError::dbus)
                .and_then(state_output)
        }
    }
}

fn state_output(state: LightSnapshot) -> Result<Output, AppError> {
    if state.online {
        Brightness::try_from(state.brightness)
            .map_err(|error| AppError::protocol(format!("daemon returned {error}")))?;
        ElgatoTemperature::try_from(state.temperature)
            .map_err(|error| AppError::protocol(format!("daemon returned {error}")))?;
    }
    Ok(Output::State { state })
}

fn print_result(result: &Output, json: bool) {
    if json {
        match serde_json::to_string(result) {
            Ok(document) => println!("{document}"),
            Err(error) => eprintln!("could not encode command result: {error}"),
        }
        return;
    }
    match result {
        Output::AccessoryInfo { accessory } => {
            println!("Name: {}", accessory.display_name);
            println!("Product: {}", accessory.product_name);
            println!("Serial: {}", accessory.serial_number);
            println!("Firmware: {}", accessory.firmware_version);
            println!("Hardware board: {}", accessory.hardware_board_type);
        }
        Output::State { state } => {
            println!("Endpoint: {}", state.endpoint);
            println!(
                "Status: {}",
                if state.online { "online" } else { "offline" }
            );
            if state.online {
                println!("Power: {}", if state.is_on { "on" } else { "off" });
                println!("Brightness: {}%", state.brightness);
                let kelvin = 1_000_000 / u32::from(state.temperature);
                println!("Temperature: {kelvin} K (Elgato {})", state.temperature);
            }
            if !state.last_error.is_empty() {
                println!("Last error: {}", state.last_error);
            }
        }
        Output::Identified => println!("Identify request sent."),
    }
}

fn print_error(error: &AppError, json: bool) {
    if json {
        let envelope = ErrorEnvelope {
            error: ErrorBody {
                kind: error.kind,
                message: &error.message,
            },
        };
        match serde_json::to_string(&envelope) {
            Ok(document) => eprintln!("{document}"),
            Err(encode_error) => eprintln!("{} ({encode_error})", error.message),
        }
    } else {
        eprintln!("Error: {}", error.message);
    }
}
