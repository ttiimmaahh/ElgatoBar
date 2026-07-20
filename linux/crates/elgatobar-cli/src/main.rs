use std::process::ExitCode;

use clap::{Args, Error as ClapError, Parser, Subcommand};
use elgatobar_core::{Brightness, ElgatoTemperature};
use elgatobar_dbus::{AccessorySnapshot, DeviceSnapshot, ElgatoBarProxy, OperationResult};
use serde::Serialize;

const EXIT_INPUT: u8 = 2;
const EXIT_CONNECTIVITY: u8 = 3;
const EXIT_PROTOCOL: u8 = 4;
const EXIT_PARTIAL: u8 = 5;

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
    /// Manage explicitly configured lights.
    Devices {
        #[command(subcommand)]
        command: DevicesCommand,
    },
    /// Read accessory information (legacy; requires exactly one configured light).
    Info,
    /// Read cached state for one device, or all devices when no ID is supplied.
    State { device_id: Option<String> },
    /// Poll one device, or all devices when no ID is supplied.
    Refresh { device_id: Option<String> },
    /// Change one or more fields on a selected device.
    Set(SetArgs),
    /// Toggle one selected device.
    Toggle { device_id: String },
    /// Toggle all currently online devices to a common target state.
    ToggleAll,
    /// Flash one selected physical light for identification.
    Identify { device_id: String },
}

#[derive(Debug, Subcommand)]
enum DevicesCommand {
    /// List configured devices and cached state.
    List,
    /// Validate and persist a device endpoint.
    Add { endpoint: String },
    /// Remove local configuration without changing the physical light.
    Remove { device_id: String },
}

#[derive(Debug, Args)]
struct SetArgs {
    /// Stable device ID shown by `devices list`.
    device_id: String,

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
    Storage,
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
            } else if name.as_str().ends_with(".Protocol") {
                ErrorKind::Protocol
            } else if name.as_str().ends_with(".Storage") {
                ErrorKind::Storage
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

    fn exit_code(&self) -> u8 {
        match self.kind {
            ErrorKind::InvalidInput => EXIT_INPUT,
            ErrorKind::Connectivity | ErrorKind::Storage => EXIT_CONNECTIVITY,
            ErrorKind::Protocol => EXIT_PROTOCOL,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum Output {
    AccessoryInfo {
        accessory: AccessorySnapshot,
    },
    Devices {
        devices: Vec<DeviceSnapshot>,
    },
    Device {
        device: DeviceSnapshot,
    },
    Operation {
        result: OperationResult,
    },
    Aggregate {
        results: Vec<OperationResult>,
        succeeded: usize,
        failed: usize,
        skipped: usize,
    },
    Removed {
        device: DeviceSnapshot,
    },
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
        Ok((result, exit_code)) => {
            print_result(&result, json);
            ExitCode::from(exit_code)
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

async fn run(cli: Cli) -> Result<(Output, u8), AppError> {
    let validated_set = match &cli.command {
        Command::Set(args) => Some(validate_set(args)?),
        _ => None,
    };
    let connection = zbus::Connection::session().await.map_err(AppError::dbus)?;
    let proxy = ElgatoBarProxy::new(&connection)
        .await
        .map_err(AppError::dbus)?;
    let output = match cli.command {
        Command::Devices {
            command: DevicesCommand::List,
        } => Output::Devices {
            devices: proxy.list_devices().await.map_err(AppError::dbus)?,
        },
        Command::Devices {
            command: DevicesCommand::Add { endpoint },
        } => Output::Device {
            device: proxy.add_device(&endpoint).await.map_err(AppError::dbus)?,
        },
        Command::Devices {
            command: DevicesCommand::Remove { device_id },
        } => Output::Removed {
            device: proxy
                .remove_device(&device_id)
                .await
                .map_err(AppError::dbus)?,
        },
        Command::Info => Output::AccessoryInfo {
            accessory: proxy.accessory_info().await.map_err(AppError::dbus)?,
        },
        Command::State {
            device_id: Some(device_id),
        } => Output::Device {
            device: proxy
                .device_snapshot(&device_id)
                .await
                .map_err(AppError::dbus)?,
        },
        Command::State { device_id: None } => Output::Devices {
            devices: proxy.list_devices().await.map_err(AppError::dbus)?,
        },
        Command::Refresh {
            device_id: Some(device_id),
        } => Output::Operation {
            result: proxy
                .refresh_device(&device_id)
                .await
                .map_err(AppError::dbus)?,
        },
        Command::Refresh { device_id: None } => {
            aggregate_output(proxy.refresh_all().await.map_err(AppError::dbus)?)
        }
        Command::Toggle { device_id } => Output::Operation {
            result: proxy
                .toggle_device(&device_id)
                .await
                .map_err(AppError::dbus)?,
        },
        Command::ToggleAll => aggregate_output(proxy.toggle_all().await.map_err(AppError::dbus)?),
        Command::Identify { device_id } => Output::Operation {
            result: proxy
                .identify_device(&device_id)
                .await
                .map_err(AppError::dbus)?,
        },
        Command::Set(args) => {
            let (has_power, power, brightness, temperature) =
                validated_set.expect("set arguments were validated before D-Bus connection");
            Output::Operation {
                result: proxy
                    .set_device_state(&args.device_id, has_power, power, brightness, temperature)
                    .await
                    .map_err(AppError::dbus)?,
            }
        }
    };
    let exit = output_exit_code(&output);
    Ok((output, exit))
}

fn validate_set(args: &SetArgs) -> Result<(bool, bool, u8, u16), AppError> {
    let has_power = args.on || args.off;
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
    Ok((has_power, args.on, brightness, temperature))
}

fn aggregate_output(results: Vec<OperationResult>) -> Output {
    let succeeded = results
        .iter()
        .filter(|result| result.status == "succeeded")
        .count();
    let failed = results
        .iter()
        .filter(|result| result.status == "failed")
        .count();
    let skipped = results
        .iter()
        .filter(|result| result.status == "skipped-offline")
        .count();
    Output::Aggregate {
        results,
        succeeded,
        failed,
        skipped,
    }
}

fn output_exit_code(output: &Output) -> u8 {
    match output {
        Output::Operation { result } if result.status == "failed" => result_exit_code(result),
        Output::Aggregate {
            results,
            succeeded,
            failed,
            skipped,
        } if *failed + *skipped > 0 => {
            if *succeeded > 0 {
                EXIT_PARTIAL
            } else {
                results
                    .iter()
                    .filter(|result| result.status == "failed")
                    .map(result_exit_code)
                    .max()
                    .unwrap_or(EXIT_CONNECTIVITY)
            }
        }
        _ => 0,
    }
}

fn result_exit_code(result: &OperationResult) -> u8 {
    if result.error_kind == "protocol" {
        EXIT_PROTOCOL
    } else {
        EXIT_CONNECTIVITY
    }
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
        Output::Devices { devices } => {
            if devices.is_empty() {
                println!("No devices configured.");
            }
            for device in devices {
                print_device(device);
            }
        }
        Output::Device { device } => print_device(device),
        Output::Removed { device } => println!(
            "Removed {} ({}) from local configuration.",
            device.name, device.device_id
        ),
        Output::Operation { result } => print_operation(result),
        Output::Aggregate {
            results,
            succeeded,
            failed,
            skipped,
        } => {
            for result in results {
                print_operation(result);
            }
            println!("Summary: {succeeded} succeeded, {failed} failed, {skipped} skipped offline");
        }
    }
}

fn print_device(device: &DeviceSnapshot) {
    println!("{} ({})", device.name, device.device_id);
    println!("  Endpoint: {}", device.endpoint);
    println!(
        "  Status: {}",
        if device.online { "online" } else { "offline" }
    );
    if device.has_state {
        println!("  Power: {}", if device.is_on { "on" } else { "off" });
        println!("  Brightness: {}%", device.brightness);
        println!(
            "  Temperature: {} K (Elgato {})",
            1_000_000 / u32::from(device.temperature),
            device.temperature
        );
    }
    if device.consecutive_failures > 0 {
        println!("  Consecutive failures: {}", device.consecutive_failures);
    }
    if !device.last_error.is_empty() {
        println!("  Last error: {}", device.last_error);
    }
}

fn print_operation(result: &OperationResult) {
    println!("{}: {}", result.device_id, result.status);
    if result.status == "succeeded" {
        print_device(&result.snapshot);
    }
    if !result.error.is_empty() {
        println!("  Error: {}", result.error);
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
