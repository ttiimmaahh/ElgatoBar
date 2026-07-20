use std::{process::ExitCode, str::FromStr, time::Duration};

use clap::{Args, Error as ClapError, Parser, Subcommand};
use elgatobar_core::{
    ApplicationController, Brightness, CommandResult, DeviceCommand, DeviceEndpoint,
    ElgatoTemperature, ReqwestLightTransport, SetLightState, TransportError,
};
use serde::Serialize;

const EXIT_INPUT: u8 = 2;
const EXIT_CONNECTIVITY: u8 = 3;
const EXIT_PROTOCOL: u8 = 4;

#[derive(Debug, Parser)]
#[command(
    name = "elgatobar",
    version,
    about = "Control an Elgato light directly over the trusted local network",
    after_help = "ENDPOINT accepts host, host:port, or http://host:port (default port: 9123)."
)]
struct Cli {
    /// Emit one JSON document on stdout (and structured errors on stderr).
    #[arg(long, global = true)]
    json: bool,

    /// Per-device request timeout in milliseconds.
    #[arg(long, global = true, default_value_t = 5_000)]
    timeout_ms: u64,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Read accessory information.
    Info { endpoint: String },
    /// Read the current light state.
    State { endpoint: String },
    /// Change one or more state fields while preserving the others.
    Set(SetArgs),
    /// Toggle power while preserving brightness and temperature.
    Toggle { endpoint: String },
    /// Flash the physical light for identification.
    Identify { endpoint: String },
}

#[derive(Debug, Args)]
struct SetArgs {
    endpoint: String,

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

    fn exit_code(&self) -> u8 {
        match self.kind {
            ErrorKind::InvalidInput => EXIT_INPUT,
            ErrorKind::Connectivity => EXIT_CONNECTIVITY,
            ErrorKind::Protocol => EXIT_PROTOCOL,
        }
    }
}

impl From<TransportError> for AppError {
    fn from(error: TransportError) -> Self {
        let kind = if error.is_connectivity() {
            ErrorKind::Connectivity
        } else {
            ErrorKind::Protocol
        };
        Self {
            kind,
            message: error.to_string(),
        }
    }
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

async fn run(cli: Cli) -> Result<CommandResult, AppError> {
    if cli.timeout_ms == 0 {
        return Err(AppError::input("timeout must be greater than zero"));
    }
    let (endpoint_text, command) = command_from_cli(cli.command)?;
    let endpoint = parse_endpoint(&endpoint_text)?;
    let transport = ReqwestLightTransport::with_timeout(Duration::from_millis(cli.timeout_ms))
        .map_err(AppError::from)?;
    ApplicationController::new(transport)
        .execute(&endpoint, command)
        .await
        .map_err(AppError::from)
}

fn parse_endpoint(value: &str) -> Result<DeviceEndpoint, AppError> {
    DeviceEndpoint::from_str(value).map_err(|error| AppError::input(error.to_string()))
}

fn command_from_cli(command: Command) -> Result<(String, DeviceCommand), AppError> {
    match command {
        Command::Info { endpoint } => Ok((endpoint, DeviceCommand::AccessoryInfo)),
        Command::State { endpoint } => Ok((endpoint, DeviceCommand::State)),
        Command::Toggle { endpoint } => Ok((endpoint, DeviceCommand::Toggle)),
        Command::Identify { endpoint } => Ok((endpoint, DeviceCommand::Identify)),
        Command::Set(args) => {
            let power = if args.on {
                Some(true)
            } else if args.off {
                Some(false)
            } else {
                None
            };
            let brightness = args
                .brightness
                .map(Brightness::try_from)
                .transpose()
                .map_err(|error| AppError::input(error.to_string()))?;
            let temperature = args
                .temperature
                .map(ElgatoTemperature::try_from)
                .transpose()
                .map_err(|error| AppError::input(error.to_string()))?
                .or_else(|| args.kelvin.map(ElgatoTemperature::from_kelvin));
            if power.is_none() && brightness.is_none() && temperature.is_none() {
                return Err(AppError::input(
                    "set requires at least one of --on, --off, --brightness, --temperature, or --kelvin",
                ));
            }
            Ok((
                args.endpoint,
                DeviceCommand::Set(SetLightState {
                    power,
                    brightness,
                    temperature,
                }),
            ))
        }
    }
}

fn print_result(result: &CommandResult, json: bool) {
    if json {
        match serde_json::to_string(result) {
            Ok(document) => println!("{document}"),
            Err(error) => eprintln!("could not encode command result: {error}"),
        }
        return;
    }
    match result {
        CommandResult::AccessoryInfo { accessory } => {
            println!("Name: {}", accessory.best_name());
            println!("Product: {}", accessory.product_name);
            println!("Serial: {}", accessory.serial_number);
            println!("Firmware: {}", accessory.firmware_version);
            println!("Hardware board: {}", accessory.hardware_board_type);
        }
        CommandResult::State { state } => {
            println!("Power: {}", if state.is_on { "on" } else { "off" });
            println!("Brightness: {}%", state.brightness.get());
            println!(
                "Temperature: {} K (Elgato {})",
                state.temperature.to_kelvin(),
                state.temperature.get()
            );
        }
        CommandResult::Identified => println!("Identify request sent."),
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

#[cfg(test)]
mod tests {
    use super::{AppError, parse_endpoint};

    #[test]
    fn cli_input_accepts_standard_bracketed_ipv6_forms() -> Result<(), AppError> {
        for value in ["http://[::1]:9123", "[::1]:9123"] {
            let endpoint = parse_endpoint(value)?;
            assert_eq!(endpoint.host(), "::1");
            assert_eq!(endpoint.port(), 9123);
        }
        Ok(())
    }
}
