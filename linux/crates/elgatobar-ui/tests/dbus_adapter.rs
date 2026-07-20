use std::{
    process::Command as ProcessCommand,
    sync::{Arc, Mutex},
    time::Duration,
};

use elgatobar_dbus::{DeviceSnapshot, OBJECT_PATH, OperationResult, SERVICE_NAME};
use elgatobar_ui::{
    client::{self, ClientEvent},
    model::Command,
};
use zbus::{interface, object_server::SignalEmitter};

const CHILD: &str = "ELGATOBAR_UI_DBUS_TEST_CHILD";

fn snapshot(id: &str, brightness: u8) -> DeviceSnapshot {
    DeviceSnapshot {
        device_id: id.into(),
        name: format!("Light {id}"),
        endpoint: format!("{id}.local"),
        online: true,
        has_state: true,
        is_on: true,
        brightness,
        temperature: 250,
        consecutive_failures: 0,
        last_error: String::new(),
    }
}
fn result(snapshot: DeviceSnapshot) -> OperationResult {
    OperationResult {
        device_id: snapshot.device_id.clone(),
        status: "succeeded".into(),
        snapshot,
        error_kind: String::new(),
        error: String::new(),
    }
}

#[derive(Clone)]
struct Fake {
    initial: Vec<DeviceSnapshot>,
    replacement: Vec<DeviceSnapshot>,
    calls: Arc<Mutex<Vec<String>>>,
}

#[interface(name = "io.github.ttiimmaahh.ElgatoBar1")]
impl Fake {
    async fn list_devices(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Vec<DeviceSnapshot> {
        let _ = Self::devices_changed(&emitter, self.replacement.clone()).await;
        self.initial.clone()
    }
    async fn toggle_device(&self, id: &str) -> OperationResult {
        self.calls.lock().unwrap().push(format!("toggle:{id}"));
        result(snapshot(id, 60))
    }
    async fn set_device_state(
        &self,
        id: &str,
        _has_power: bool,
        _power: bool,
        brightness: u8,
        temperature: u16,
    ) -> OperationResult {
        self.calls
            .lock()
            .unwrap()
            .push(format!("set:{id}:{brightness}:{temperature}"));
        result(snapshot(id, brightness.max(50)))
    }
    async fn identify_device(&self, id: &str) -> OperationResult {
        self.calls.lock().unwrap().push(format!("identify:{id}"));
        result(snapshot(id, 50))
    }
    async fn refresh_all(&self) -> Vec<OperationResult> {
        self.calls.lock().unwrap().push("refresh-all".into());
        vec![result(snapshot("b", 50))]
    }
    async fn toggle_all(&self) -> Vec<OperationResult> {
        self.calls.lock().unwrap().push("toggle-all".into());
        vec![result(snapshot("b", 50))]
    }
    async fn add_device(&self, endpoint: &str) -> DeviceSnapshot {
        self.calls.lock().unwrap().push(format!("add:{endpoint}"));
        snapshot("added", 50)
    }
    async fn remove_device(&self, id: &str) -> DeviceSnapshot {
        self.calls.lock().unwrap().push(format!("remove:{id}"));
        snapshot(id, 50)
    }
    #[zbus(signal)]
    async fn devices_changed(
        emitter: &SignalEmitter<'_>,
        snapshots: Vec<DeviceSnapshot>,
    ) -> zbus::Result<()>;
}

fn next(handle: &client::ClientHandle) -> ClientEvent {
    handle
        .events
        .recv_timeout(Duration::from_secs(5))
        .expect("client event")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn adapter_subscribes_before_list_calls_typed_methods_and_reconnects() {
    if std::env::var_os(CHILD).is_none() {
        let status = ProcessCommand::new("dbus-run-session")
            .arg("--")
            .arg("env")
            .arg(format!("{CHILD}=1"))
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "adapter_subscribes_before_list_calls_typed_methods_and_reconnects",
                "--nocapture",
            ])
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    let calls = Arc::new(Mutex::new(Vec::new()));
    let connection = zbus::connection::Builder::session()
        .unwrap()
        .name(SERVICE_NAME)
        .unwrap()
        .serve_at(
            OBJECT_PATH,
            Fake {
                initial: vec![snapshot("a", 40)],
                replacement: vec![snapshot("b", 50)],
                calls: calls.clone(),
            },
        )
        .unwrap()
        .build()
        .await
        .unwrap();
    let handle = client::spawn();
    assert!(matches!(next(&handle),ClientEvent::Connected(devices) if devices[0].device_id=="a"));
    assert!(
        matches!(next(&handle),ClientEvent::Replaced(devices) if devices.len()==1 && devices[0].device_id=="b")
    );
    for command in [
        Command::Toggle {
            id: "b".into(),
            generation: 1,
        },
        Command::SetBrightness {
            id: "b".into(),
            value: 63,
            generation: 2,
        },
        Command::SetTemperature {
            id: "b".into(),
            value: 260,
            generation: 3,
        },
        Command::Identify {
            id: "b".into(),
            generation: 4,
        },
        Command::RefreshAll { generation: 5 },
        Command::ToggleAll { generation: 6 },
        Command::Add {
            endpoint: "new.local".into(),
            generation: 7,
        },
        Command::Remove {
            id: "b".into(),
            generation: 8,
        },
    ] {
        handle.commands.send(command).unwrap();
        let _ = next(&handle);
    }
    let recorded = calls.lock().unwrap().clone();
    assert_eq!(
        recorded,
        vec![
            "toggle:b",
            "set:b:63:0",
            "set:b:0:260",
            "identify:b",
            "refresh-all",
            "toggle-all",
            "add:new.local",
            "remove:b"
        ]
    );
    drop(connection);
    assert!(matches!(next(&handle), ClientEvent::Unavailable(_)));
    handle
        .commands
        .send(Command::Toggle {
            id: "b".into(),
            generation: 99,
        })
        .unwrap();
    assert!(
        matches!(next(&handle), ClientEvent::Failed { id: Some(id), generation: 99, .. } if id=="b")
    );
    let _restarted = zbus::connection::Builder::session()
        .unwrap()
        .name(SERVICE_NAME)
        .unwrap()
        .serve_at(
            OBJECT_PATH,
            Fake {
                initial: vec![snapshot("restart", 70)],
                replacement: vec![snapshot("restart", 71)],
                calls,
            },
        )
        .unwrap()
        .build()
        .await
        .unwrap();
    let mut recovered = false;
    for _ in 0..5 {
        if matches!(next(&handle),ClientEvent::Connected(devices) if devices[0].device_id=="restart")
        {
            recovered = true;
            break;
        }
    }
    assert!(recovered);
}
