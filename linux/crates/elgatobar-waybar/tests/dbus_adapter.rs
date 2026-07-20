use std::{
    io::{self, Write},
    process::Command as ProcessCommand,
    sync::{Arc, Mutex},
    time::Duration,
};

use elgatobar_dbus::{DeviceSnapshot, OBJECT_PATH, OperationResult, SERVICE_NAME};
use elgatobar_waybar::{Action, WaybarOutput, run_action, watch};
use tokio::sync::watch as shutdown;
use zbus::{interface, object_server::SignalEmitter};

const CHILD: &str = "ELGATOBAR_WAYBAR_DBUS_TEST_CHILD";

fn snapshot(id: &str, on: bool, brightness: u8) -> DeviceSnapshot {
    DeviceSnapshot {
        device_id: id.into(),
        name: format!("Light {id}"),
        endpoint: format!("{id}.local"),
        online: true,
        has_state: true,
        is_on: on,
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

    async fn toggle_all(&self) -> Vec<OperationResult> {
        self.calls.lock().unwrap().push("toggle-all".into());
        vec![result(snapshot("action", true, 50))]
    }

    async fn refresh_all(&self) -> Vec<OperationResult> {
        self.calls.lock().unwrap().push("refresh-all".into());
        vec![result(snapshot("action", true, 50))]
    }

    #[zbus(signal)]
    async fn devices_changed(
        emitter: &SignalEmitter<'_>,
        snapshots: Vec<DeviceSnapshot>,
    ) -> zbus::Result<()>;
}

#[derive(Clone, Default)]
struct SharedWriter(Arc<Mutex<Vec<u8>>>);

impl Write for SharedWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl SharedWriter {
    fn outputs(&self) -> Vec<WaybarOutput> {
        String::from_utf8(self.0.lock().unwrap().clone())
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}

async fn wait_for(
    writer: &SharedWriter,
    predicate: impl Fn(&[WaybarOutput]) -> bool,
) -> Vec<WaybarOutput> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let outputs = writer.outputs();
            if predicate(&outputs) {
                break outputs;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("Waybar output")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn watcher_uses_typed_replacements_actions_owner_loss_and_reconnect() {
    if std::env::var_os(CHILD).is_none() {
        let status = ProcessCommand::new("dbus-run-session")
            .arg("--")
            .arg("env")
            .arg(format!("{CHILD}=1"))
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "watcher_uses_typed_replacements_actions_owner_loss_and_reconnect",
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
                initial: vec![snapshot("initial", false, 30)],
                replacement: vec![snapshot("replacement", true, 60)],
                calls: calls.clone(),
            },
        )
        .unwrap()
        .build()
        .await
        .unwrap();
    let writer = SharedWriter::default();
    let (shutdown_tx, shutdown_rx) = shutdown::channel(false);
    let task = tokio::spawn(watch(writer.clone(), shutdown_rx));

    let outputs = wait_for(&writer, |outputs| {
        outputs.iter().any(|output| output.text == "Lights 1/1")
    })
    .await;
    assert!(outputs.iter().any(|output| output.text == "Lights off"));
    assert!(outputs.iter().any(|output| output.text == "Lights 1/1"));

    run_action(Action::ToggleAll).await.unwrap();
    run_action(Action::RefreshAll).await.unwrap();
    assert_eq!(*calls.lock().unwrap(), ["toggle-all", "refresh-all"]);

    drop(connection);
    wait_for(&writer, |outputs| {
        outputs
            .last()
            .is_some_and(|output| output.class.contains(&"stale".into()))
    })
    .await;

    let _restarted = zbus::connection::Builder::session()
        .unwrap()
        .name(SERVICE_NAME)
        .unwrap()
        .serve_at(
            OBJECT_PATH,
            Fake {
                initial: vec![snapshot("restart", true, 70)],
                replacement: vec![snapshot("restart", true, 71)],
                calls,
            },
        )
        .unwrap()
        .build()
        .await
        .unwrap();
    wait_for(&writer, |outputs| {
        outputs.last().is_some_and(|output| {
            output.text == "Lights 1/1" && !output.class.contains(&"stale".into())
        })
    })
    .await;

    shutdown_tx.send(true).unwrap();
    task.await.unwrap().unwrap();
}
