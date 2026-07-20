use std::{
    io::{self, Write},
    time::Duration,
};

use elgatobar_dbus::{DeviceSnapshot, ElgatoBarProxy, OperationResult};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::watch;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WaybarOutput {
    pub text: String,
    pub alt: String,
    pub tooltip: String,
    pub class: Vec<String>,
    pub percentage: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Availability<'a> {
    Available,
    Unavailable(&'a str),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    ToggleAll,
    RefreshAll,
}

#[derive(Debug, Error)]
pub enum ActionError {
    #[error("cannot connect to the ElgatoBar daemon: {0}")]
    Dbus(#[from] zbus::Error),
    #[error("daemon action did not fully succeed: {0}")]
    Incomplete(String),
}

#[must_use]
pub fn render(devices: &[DeviceSnapshot], availability: Availability<'_>) -> WaybarOutput {
    if devices.is_empty() {
        return match availability {
            Availability::Available => WaybarOutput {
                text: "Lights —".into(),
                alt: "unconfigured".into(),
                tooltip: "No lights are configured.".into(),
                class: vec!["unconfigured".into()],
                percentage: 0,
            },
            Availability::Unavailable(error) => WaybarOutput {
                text: "Lights unavailable".into(),
                alt: "unavailable".into(),
                tooltip: format!("ElgatoBar daemon unavailable: {error}"),
                class: vec!["unavailable".into()],
                percentage: 0,
            },
        };
    }

    let online = devices.iter().filter(|device| device.online).count();
    let on = devices
        .iter()
        .filter(|device| device.online && device.has_state && device.is_on)
        .count();
    let known_online = devices
        .iter()
        .filter(|device| device.online && device.has_state)
        .count();
    let primary = if online == 0 {
        "offline"
    } else if on == 0 {
        "off"
    } else if on == known_online && known_online == online {
        "on"
    } else {
        "mixed"
    };
    let mut class = vec![primary.into()];
    if online > 0 && online < devices.len() {
        class.push("partial-offline".into());
    }
    let stale = matches!(availability, Availability::Unavailable(_));
    if stale {
        class.push("stale".into());
    }
    let text = if stale {
        "Lights stale".into()
    } else if online == 0 {
        "Lights offline".into()
    } else if on == 0 {
        "Lights off".into()
    } else {
        format!("Lights {on}/{}", devices.len())
    };
    let mut tooltip = Vec::with_capacity(devices.len() + usize::from(stale));
    if let Availability::Unavailable(error) = availability {
        tooltip.push(format!("Daemon unavailable; showing stale values: {error}"));
    }
    tooltip.extend(devices.iter().map(device_tooltip));
    WaybarOutput {
        text,
        alt: if stale {
            "stale".into()
        } else {
            primary.into()
        },
        tooltip: tooltip.join("\r"),
        class,
        percentage: aggregate_percentage(devices),
    }
}

fn aggregate_percentage(devices: &[DeviceSnapshot]) -> u8 {
    let values: Vec<_> = devices
        .iter()
        .filter(|device| device.online && device.has_state && device.is_on)
        .map(|device| u32::from(device.brightness))
        .collect();
    if values.is_empty() {
        0
    } else {
        (values.iter().sum::<u32>() / values.len() as u32) as u8
    }
}

fn device_tooltip(device: &DeviceSnapshot) -> String {
    if !device.has_state {
        return format!(
            "{}: {} · no state received",
            device.name,
            if device.online { "Online" } else { "Offline" }
        );
    }
    let kelvin = if (143..=344).contains(&device.temperature) {
        format!("{} K", 1_000_000 / u32::from(device.temperature))
    } else {
        format!("native {}", device.temperature)
    };
    format!(
        "{}: {}{} · {}% · {}",
        device.name,
        if device.is_on { "On" } else { "Off" },
        if device.online { "" } else { " (offline)" },
        device.brightness,
        kelvin
    )
}

pub async fn run_action(action: Action) -> Result<(), ActionError> {
    let connection = zbus::Connection::session().await?;
    let proxy = ElgatoBarProxy::new(&connection).await?;
    let results = match action {
        Action::ToggleAll => proxy.toggle_all().await?,
        Action::RefreshAll => proxy.refresh_all().await?,
    };
    ensure_complete(&results)
}

fn ensure_complete(results: &[OperationResult]) -> Result<(), ActionError> {
    let incomplete = results
        .iter()
        .filter(|result| result.status != "succeeded")
        .count();
    if incomplete == 0 {
        Ok(())
    } else {
        Err(ActionError::Incomplete(format!(
            "{incomplete} of {} device operations failed or were skipped",
            results.len()
        )))
    }
}

pub async fn watch<W: Write>(mut writer: W, mut shutdown: watch::Receiver<bool>) -> io::Result<()> {
    let mut cached = Vec::new();
    let mut last = None;
    let mut backoff = Duration::from_secs(1);
    loop {
        if *shutdown.borrow() {
            return Ok(());
        }
        let connection = match zbus::Connection::session().await {
            Ok(connection) => connection,
            Err(error) => {
                emit_if_changed(
                    &mut writer,
                    &mut last,
                    render(&cached, Availability::Unavailable(&error.to_string())),
                )?;
                if wait_or_shutdown(backoff, &mut shutdown).await {
                    return Ok(());
                }
                backoff = (backoff * 2).min(Duration::from_secs(30));
                continue;
            }
        };
        let proxy = match ElgatoBarProxy::new(&connection).await {
            Ok(proxy) => proxy,
            Err(error) => {
                emit_if_changed(
                    &mut writer,
                    &mut last,
                    render(&cached, Availability::Unavailable(&error.to_string())),
                )?;
                if wait_or_shutdown(backoff, &mut shutdown).await {
                    return Ok(());
                }
                backoff = (backoff * 2).min(Duration::from_secs(30));
                continue;
            }
        };
        let mut changes = match proxy.receive_devices_changed().await {
            Ok(changes) => changes,
            Err(error) => {
                emit_if_changed(
                    &mut writer,
                    &mut last,
                    render(&cached, Availability::Unavailable(&error.to_string())),
                )?;
                if wait_or_shutdown(backoff, &mut shutdown).await {
                    return Ok(());
                }
                backoff = (backoff * 2).min(Duration::from_secs(30));
                continue;
            }
        };
        let mut owners = match proxy.inner().receive_owner_changed().await {
            Ok(owners) => owners,
            Err(error) => {
                emit_if_changed(
                    &mut writer,
                    &mut last,
                    render(&cached, Availability::Unavailable(&error.to_string())),
                )?;
                if wait_or_shutdown(backoff, &mut shutdown).await {
                    return Ok(());
                }
                backoff = (backoff * 2).min(Duration::from_secs(30));
                continue;
            }
        };
        match proxy.list_devices().await {
            Ok(devices) => cached = devices,
            Err(error) => {
                emit_if_changed(
                    &mut writer,
                    &mut last,
                    render(&cached, Availability::Unavailable(&error.to_string())),
                )?;
                if wait_or_shutdown(backoff, &mut shutdown).await {
                    return Ok(());
                }
                backoff = (backoff * 2).min(Duration::from_secs(30));
                continue;
            }
        }
        emit_if_changed(
            &mut writer,
            &mut last,
            render(&cached, Availability::Available),
        )?;
        backoff = Duration::from_secs(1);

        let disconnected = loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        return Ok(());
                    }
                }
                signal = changes.next() => match signal {
                    Some(signal) => match signal.args() {
                        Ok(args) => {
                            cached = args.snapshots().to_vec();
                            emit_if_changed(&mut writer, &mut last, render(&cached, Availability::Available))?;
                        }
                        Err(error) => break error.to_string(),
                    },
                    None => break "daemon signal subscription ended".into(),
                },
                owner = owners.next() => {
                    if owner.flatten().is_none() {
                        break "daemon stopped".into();
                    }
                }
            }
        };
        emit_if_changed(
            &mut writer,
            &mut last,
            render(&cached, Availability::Unavailable(&disconnected)),
        )?;
        if wait_or_shutdown(backoff, &mut shutdown).await {
            return Ok(());
        }
        backoff = (backoff * 2).min(Duration::from_secs(30));
    }
}

fn emit_if_changed<W: Write>(
    writer: &mut W,
    last: &mut Option<WaybarOutput>,
    output: WaybarOutput,
) -> io::Result<()> {
    if last.as_ref() == Some(&output) {
        return Ok(());
    }
    serde_json::to_writer(&mut *writer, &output).map_err(io::Error::other)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    *last = Some(output);
    Ok(())
}

async fn wait_or_shutdown(duration: Duration, shutdown: &mut watch::Receiver<bool>) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(duration) => false,
        changed = shutdown.changed() => changed.is_err() || *shutdown.borrow(),
    }
}
