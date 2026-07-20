use std::{
    sync::mpsc::{Receiver, Sender},
    time::Duration,
};

use elgatobar_dbus::{DeviceSnapshot, ElgatoBarProxy, OperationResult};
use futures_util::StreamExt;
use tokio::runtime::Runtime;

use crate::model::{Command, ReconnectBackoff};

#[derive(Clone, Debug)]
pub enum ClientEvent {
    Connected(Vec<DeviceSnapshot>),
    Replaced(Vec<DeviceSnapshot>),
    Unavailable(String),
    Completed {
        id: Option<String>,
        generation: u64,
        results: Vec<OperationResult>,
    },
    Added {
        generation: u64,
    },
    Removed {
        id: String,
        generation: u64,
    },
    Failed {
        id: Option<String>,
        generation: u64,
        message: String,
    },
}

pub struct ClientHandle {
    pub commands: Sender<Command>,
    pub events: Receiver<ClientEvent>,
    cancel: Sender<()>,
}

impl Drop for ClientHandle {
    fn drop(&mut self) {
        let _ = self.cancel.send(());
    }
}

pub fn spawn() -> ClientHandle {
    let (command_tx, command_rx) = std::sync::mpsc::channel();
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    let (cancel_tx, cancel_rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("elgatobar-dbus".into())
        .spawn(move || {
            Runtime::new()
                .expect("create D-Bus runtime")
                .block_on(run(command_rx, event_tx, cancel_rx));
        })
        .expect("start D-Bus thread");
    ClientHandle {
        commands: command_tx,
        events: event_rx,
        cancel: cancel_tx,
    }
}

async fn run(commands: Receiver<Command>, events: Sender<ClientEvent>, cancel: Receiver<()>) {
    let mut backoff = ReconnectBackoff::default();
    loop {
        if cancel.try_recv().is_ok() {
            return;
        }
        match connected(&commands, &events, &cancel, &mut backoff).await {
            Ok(()) => backoff.reset(),
            Err(error) => {
                let _ = events.send(ClientEvent::Unavailable(error));
            }
        }
        let started = tokio::time::Instant::now();
        loop {
            if cancel.try_recv().is_ok() {
                return;
            }
            match commands.try_recv() {
                Ok(Command::Retry) => break,
                Ok(command) => {
                    let _ = events.send(ClientEvent::Failed {
                        id: command_id(&command),
                        generation: command_generation(&command),
                        message: "ElgatoBar daemon is unavailable".into(),
                    });
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
            if started.elapsed() >= backoff.current() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        backoff.advance();
    }
}

async fn connected(
    commands: &Receiver<Command>,
    events: &Sender<ClientEvent>,
    cancel: &Receiver<()>,
    backoff: &mut ReconnectBackoff,
) -> Result<(), String> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|e| e.to_string())?;
    let proxy = ElgatoBarProxy::new(&connection)
        .await
        .map_err(|e| e.to_string())?;
    let mut changes = proxy
        .receive_devices_changed()
        .await
        .map_err(|e| e.to_string())?;
    let mut owners = proxy
        .inner()
        .receive_owner_changed()
        .await
        .map_err(|e| e.to_string())?;
    let initial = proxy.list_devices().await.map_err(|e| e.to_string())?;
    events
        .send(ClientEvent::Connected(initial))
        .map_err(|e| e.to_string())?;
    backoff.reset();
    loop {
        if cancel.try_recv().is_ok() {
            return Ok(());
        }
        tokio::select! {
            signal = changes.next() => match signal {
                Some(signal) => events.send(ClientEvent::Replaced(signal.args().map_err(|e| e.to_string())?.snapshots().to_vec())).map_err(|e| e.to_string())?,
                None => return Err("daemon signal subscription ended".into()),
            },
            owner = owners.next() => if owner.flatten().is_none() { return Err("daemon stopped".into()); },
            _ = tokio::time::sleep(Duration::from_millis(25)) => {
                while let Ok(command) = commands.try_recv() {
                    if matches!(command, Command::Retry) { continue; }
                    execute(&proxy, command, events).await;
                }
            }
        }
    }
}

async fn execute(proxy: &ElgatoBarProxy<'_>, command: Command, events: &Sender<ClientEvent>) {
    let failed_id = command_id(&command);
    let failed_generation = command_generation(&command);
    let outcome: Result<ClientEvent, zbus::Error> = match command {
        Command::RefreshAll { generation } => {
            proxy
                .refresh_all()
                .await
                .map(|results| ClientEvent::Completed {
                    id: None,
                    generation,
                    results,
                })
        }
        Command::ToggleAll { generation } => {
            proxy
                .toggle_all()
                .await
                .map(|results| ClientEvent::Completed {
                    id: None,
                    generation,
                    results,
                })
        }
        Command::Toggle { id, generation } => {
            proxy
                .toggle_device(&id)
                .await
                .map(|r| ClientEvent::Completed {
                    id: Some(id),
                    generation,
                    results: vec![r],
                })
        }
        Command::SetBrightness {
            id,
            value,
            generation,
        } => proxy
            .set_device_state(&id, false, false, value, 0)
            .await
            .map(|r| ClientEvent::Completed {
                id: Some(id),
                generation,
                results: vec![r],
            }),
        Command::SetTemperature {
            id,
            value,
            generation,
        } => proxy
            .set_device_state(&id, false, false, 0, value)
            .await
            .map(|r| ClientEvent::Completed {
                id: Some(id),
                generation,
                results: vec![r],
            }),
        Command::Identify { id, generation } => {
            proxy
                .identify_device(&id)
                .await
                .map(|r| ClientEvent::Completed {
                    id: Some(id),
                    generation,
                    results: vec![r],
                })
        }
        Command::Add {
            endpoint,
            generation,
        } => proxy
            .add_device(&endpoint)
            .await
            .map(|_| ClientEvent::Added { generation }),
        Command::Remove { id, generation } => proxy
            .remove_device(&id)
            .await
            .map(|_| ClientEvent::Removed { id, generation }),
        Command::Retry => return,
    };
    let event = outcome.unwrap_or_else(|error| ClientEvent::Failed {
        id: failed_id,
        generation: failed_generation,
        message: error.to_string(),
    });
    let _ = events.send(event);
}

fn command_id(command: &Command) -> Option<String> {
    match command {
        Command::Toggle { id, .. }
        | Command::SetBrightness { id, .. }
        | Command::SetTemperature { id, .. }
        | Command::Identify { id, .. }
        | Command::Remove { id, .. } => Some(id.clone()),
        _ => None,
    }
}
fn command_generation(command: &Command) -> u64 {
    match command {
        Command::RefreshAll { generation }
        | Command::ToggleAll { generation }
        | Command::Toggle { generation, .. }
        | Command::SetBrightness { generation, .. }
        | Command::SetTemperature { generation, .. }
        | Command::Identify { generation, .. }
        | Command::Add { generation, .. }
        | Command::Remove { generation, .. } => *generation,
        Command::Retry => 0,
    }
}
