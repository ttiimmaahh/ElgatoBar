use std::{
    collections::{HashMap, HashSet, VecDeque},
    future::Future,
    pin::Pin,
    str::FromStr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use elgatobar_core::{
    AccessoryInfo, Brightness, DeviceEndpoint, DeviceIdentity, DeviceOperationResult,
    DeviceStorageDocument, DeviceStore, DocumentName, ElgatoTemperature, LightState,
    LightTransport, ManagerError, MultiDeviceController, OperationStatus, PersistedDevice,
    REFRESH_RETRY_DELAY, RetryClock, TransportError,
};
use tokio::sync::Barrier;
use uuid::Uuid;

#[derive(Clone)]
struct MemoryStore {
    document: Arc<Mutex<DeviceStorageDocument>>,
    fail_save: Arc<Mutex<bool>>,
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::with_devices(Vec::new())
    }
}

impl MemoryStore {
    fn with_devices(devices: Vec<PersistedDevice>) -> Self {
        Self {
            document: Arc::new(Mutex::new(DeviceStorageDocument::new(devices))),
            fail_save: Arc::new(Mutex::new(false)),
        }
    }
}

impl DeviceStore for MemoryStore {
    fn load(&self) -> Result<DeviceStorageDocument, String> {
        Ok(self.document.lock().unwrap().clone())
    }

    fn save(&self, document: &DeviceStorageDocument) -> Result<(), String> {
        if *self.fail_save.lock().unwrap() {
            return Err("injected save failure".to_string());
        }
        *self.document.lock().unwrap() = document.clone();
        Ok(())
    }
}

#[derive(Clone, Default)]
struct FakeClock(Arc<Mutex<Vec<Duration>>>);

impl RetryClock for FakeClock {
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        self.0.lock().unwrap().push(duration);
        Box::pin(async {})
    }
}

#[derive(Clone, Default)]
struct FakeTransport {
    inner: Arc<FakeTransportInner>,
}

#[derive(Default)]
struct FakeTransportInner {
    accessories: Mutex<HashMap<String, AccessoryInfo>>,
    states: Mutex<HashMap<String, LightState>>,
    reads: Mutex<HashMap<String, VecDeque<bool>>>,
    fail_sets: Mutex<HashSet<String>>,
    in_flight: AtomicUsize,
    maximum_in_flight: AtomicUsize,
    barrier: Mutex<Option<Arc<Barrier>>>,
    barrier_arrivals: AtomicUsize,
}

impl FakeTransport {
    fn configure(&self, endpoint: &DeviceEndpoint, serial: &str, state: LightState) {
        self.inner.accessories.lock().unwrap().insert(
            endpoint.to_string(),
            AccessoryInfo {
                product_name: "Key Light".to_string(),
                hardware_board_type: 53,
                firmware_build_number: 218,
                firmware_version: "1.0.3".to_string(),
                serial_number: serial.to_string(),
                display_name: Some(format!("Light {serial}")),
                features: None,
                wifi_info: None,
            },
        );
        self.inner
            .states
            .lock()
            .unwrap()
            .insert(endpoint.to_string(), state);
    }

    fn script_reads(&self, endpoint: &DeviceEndpoint, successes: impl IntoIterator<Item = bool>) {
        self.inner
            .reads
            .lock()
            .unwrap()
            .insert(endpoint.to_string(), successes.into_iter().collect());
    }

    fn fail_set(&self, endpoint: &DeviceEndpoint) {
        self.inner
            .fail_sets
            .lock()
            .unwrap()
            .insert(endpoint.to_string());
    }

    fn track_eight_way_overlap(&self) {
        *self.inner.barrier.lock().unwrap() = Some(Arc::new(Barrier::new(8)));
    }
}

fn connectivity(endpoint: &DeviceEndpoint) -> TransportError {
    TransportError::Connectivity {
        endpoint: endpoint.clone(),
        message: "injected offline".to_string(),
    }
}

#[async_trait]
impl LightTransport for FakeTransport {
    async fn accessory_info(
        &self,
        endpoint: &DeviceEndpoint,
    ) -> Result<AccessoryInfo, TransportError> {
        self.inner
            .accessories
            .lock()
            .unwrap()
            .get(&endpoint.to_string())
            .cloned()
            .ok_or_else(|| connectivity(endpoint))
    }

    async fn light_state(&self, endpoint: &DeviceEndpoint) -> Result<LightState, TransportError> {
        let current = self.inner.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.inner
            .maximum_in_flight
            .fetch_max(current, Ordering::SeqCst);
        let arrival = self.inner.barrier_arrivals.fetch_add(1, Ordering::SeqCst);
        let barrier = self.inner.barrier.lock().unwrap().clone();
        if arrival < 8
            && let Some(barrier) = barrier
        {
            barrier.wait().await;
        }
        let succeeds = self
            .inner
            .reads
            .lock()
            .unwrap()
            .get_mut(&endpoint.to_string())
            .and_then(VecDeque::pop_front)
            .unwrap_or(true);
        let state = self
            .inner
            .states
            .lock()
            .unwrap()
            .get(&endpoint.to_string())
            .copied();
        self.inner.in_flight.fetch_sub(1, Ordering::SeqCst);
        if succeeds {
            state.ok_or_else(|| connectivity(endpoint))
        } else {
            Err(connectivity(endpoint))
        }
    }

    async fn set_light_state(
        &self,
        endpoint: &DeviceEndpoint,
        state: LightState,
    ) -> Result<LightState, TransportError> {
        if self
            .inner
            .fail_sets
            .lock()
            .unwrap()
            .contains(&endpoint.to_string())
        {
            return Err(connectivity(endpoint));
        }
        self.inner
            .states
            .lock()
            .unwrap()
            .insert(endpoint.to_string(), state);
        Ok(state)
    }

    async fn identify(&self, _: &DeviceEndpoint) -> Result<(), TransportError> {
        Ok(())
    }
}

fn endpoint(index: usize) -> DeviceEndpoint {
    DeviceEndpoint::from_str(&format!("192.0.2.{}", index + 1)).unwrap()
}

fn state(is_on: bool, brightness: u8) -> LightState {
    LightState {
        is_on,
        brightness: Brightness::try_from(brightness).unwrap(),
        temperature: ElgatoTemperature::try_from(250).unwrap(),
    }
}

fn persisted(index: usize) -> PersistedDevice {
    PersistedDevice::new(
        DeviceIdentity::serial(format!("serial-{index}")).unwrap(),
        DocumentName::new(format!("Light {index}")).unwrap(),
        endpoint(index),
    )
}

fn id(index: usize) -> String {
    persisted(index).identity.canonical_id()
}

fn manager(
    devices: Vec<PersistedDevice>,
    transport: FakeTransport,
    clock: FakeClock,
) -> MultiDeviceController<FakeTransport, MemoryStore, FakeClock> {
    MultiDeviceController::load_with_clock(transport, MemoryStore::with_devices(devices), clock)
        .unwrap()
}

#[tokio::test]
async fn two_devices_persist_across_reload_with_stable_ids_and_serial_duplicates_move_metadata() {
    let transport = FakeTransport::default();
    let first = endpoint(0);
    let second = endpoint(1);
    transport.configure(&first, "ABC", state(false, 50));
    transport.configure(&second, "XYZ", state(true, 60));
    let store = MemoryStore::default();
    let controller = MultiDeviceController::load_with_clock(
        transport.clone(),
        store.clone(),
        FakeClock::default(),
    )
    .unwrap();
    let first_snapshot = controller.add(first.clone()).await.unwrap();
    let second_snapshot = controller.add(second).await.unwrap();
    assert_ne!(first_snapshot.device_id, second_snapshot.device_id);
    drop(controller);

    let reloaded = MultiDeviceController::load_with_clock(
        transport.clone(),
        store.clone(),
        FakeClock::default(),
    )
    .unwrap();
    let ids: Vec<_> = reloaded
        .snapshots()
        .await
        .into_iter()
        .map(|snapshot| snapshot.device_id)
        .collect();
    assert_eq!(
        ids,
        vec![first_snapshot.device_id.clone(), second_snapshot.device_id]
    );

    let moved = endpoint(2);
    transport.configure(&moved, "  abc  ", state(true, 70));
    let updated = reloaded.add(moved.clone()).await.unwrap();
    assert_eq!(updated.device_id, first_snapshot.device_id);
    assert_eq!(updated.endpoint, moved.to_string());
    assert_eq!(reloaded.snapshots().await.len(), 2);
}

#[test]
fn installation_local_identity_refuses_endpoint_reassociation_on_load() {
    let confirmed = endpoint(0);
    let identity = DeviceIdentity::installation_local(Uuid::new_v4(), confirmed);
    let inconsistent =
        PersistedDevice::new(identity, DocumentName::new("Local").unwrap(), endpoint(1));
    let error = MultiDeviceController::load_with_clock(
        FakeTransport::default(),
        MemoryStore::with_devices(vec![inconsistent]),
        FakeClock::default(),
    )
    .err()
    .expect("inconsistent local identity must be rejected");
    assert!(
        error
            .to_string()
            .contains("explicit removal and re-addition")
    );
}

#[tokio::test]
async fn refresh_all_caps_concurrency_at_eight_and_returns_every_device() {
    let transport = FakeTransport::default();
    let devices: Vec<_> = (0..9).map(persisted).collect();
    for index in 0..9 {
        transport.configure(
            &endpoint(index),
            &format!("serial-{index}"),
            state(false, 50),
        );
    }
    transport.track_eight_way_overlap();
    let controller = manager(devices, transport.clone(), FakeClock::default());
    let results = controller.refresh_all().await;
    assert_eq!(results.len(), 9);
    assert_eq!(transport.inner.maximum_in_flight.load(Ordering::SeqCst), 8);
}

#[tokio::test]
async fn refresh_retries_once_after_500ms_and_recovers() {
    let transport = FakeTransport::default();
    transport.configure(&endpoint(0), "serial-0", state(true, 55));
    transport.script_reads(&endpoint(0), [false, true]);
    let clock = FakeClock::default();
    let controller = manager(vec![persisted(0)], transport, clock.clone());
    let result = controller.refresh(&id(0)).await.unwrap();
    assert_eq!(result.status, OperationStatus::Succeeded);
    assert_eq!(*clock.0.lock().unwrap(), vec![REFRESH_RETRY_DELAY]);
}

#[tokio::test]
async fn offline_transition_preserves_last_known_state_and_success_resets_failures() {
    let transport = FakeTransport::default();
    transport.configure(&endpoint(0), "serial-0", state(true, 61));
    let controller = manager(vec![persisted(0)], transport.clone(), FakeClock::default());
    assert_eq!(
        controller.refresh(&id(0)).await.unwrap().status,
        OperationStatus::Succeeded
    );

    transport.script_reads(&endpoint(0), [false, false, false, false, true]);
    let first = controller.refresh(&id(0)).await.unwrap().snapshot;
    assert!(first.online);
    assert_eq!(first.consecutive_failures, 1);
    assert!(first.is_on);
    assert_eq!(first.brightness, 61);
    let second = controller.refresh(&id(0)).await.unwrap().snapshot;
    assert!(!second.online);
    assert_eq!(second.consecutive_failures, 2);
    assert!(second.is_on);
    assert_eq!(second.brightness, 61);
    let recovered = controller.refresh(&id(0)).await.unwrap().snapshot;
    assert!(recovered.online);
    assert_eq!(recovered.consecutive_failures, 0);
    assert!(recovered.last_error.is_empty());
}

async fn toggle_case(states: &[bool]) -> Vec<DeviceOperationResult> {
    let transport = FakeTransport::default();
    let devices: Vec<_> = states
        .iter()
        .enumerate()
        .map(|(index, _)| persisted(index))
        .collect();
    for (index, is_on) in states.iter().copied().enumerate() {
        transport.configure(
            &endpoint(index),
            &format!("serial-{index}"),
            state(is_on, 50),
        );
    }
    let controller = manager(devices, transport, FakeClock::default());
    controller.refresh_all().await;
    controller.toggle_all().await
}

#[tokio::test]
async fn toggle_all_uses_any_on_semantics_for_all_off_any_on_and_mixed() {
    for initial in [vec![false, false], vec![true, true], vec![false, true]] {
        let results = toggle_case(&initial).await;
        let expected = !initial.iter().any(|value| *value);
        assert!(
            results
                .iter()
                .all(|result| result.snapshot.is_on == expected)
        );
    }
}

#[tokio::test]
async fn toggle_all_skips_all_offline_devices() {
    let transport = FakeTransport::default();
    transport.configure(&endpoint(0), "serial-0", state(false, 50));
    let controller = manager(vec![persisted(0)], transport.clone(), FakeClock::default());
    transport.script_reads(&endpoint(0), [false, false, false, false]);
    controller.refresh(&id(0)).await.unwrap();
    controller.refresh(&id(0)).await.unwrap();
    let results = controller.toggle_all().await;
    assert_eq!(results[0].status, OperationStatus::SkippedOffline);
}

#[tokio::test]
async fn partial_toggle_failure_preserves_successful_device_state() {
    let transport = FakeTransport::default();
    for index in 0..2 {
        transport.configure(
            &endpoint(index),
            &format!("serial-{index}"),
            state(false, 50),
        );
    }
    let controller = manager(
        vec![persisted(0), persisted(1)],
        transport.clone(),
        FakeClock::default(),
    );
    controller.refresh_all().await;
    transport.fail_set(&endpoint(1));
    let results = controller.toggle_all().await;
    assert_eq!(
        results
            .iter()
            .filter(|result| result.status == OperationStatus::Succeeded)
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| result.status == OperationStatus::Failed)
            .count(),
        1
    );
    assert!(controller.snapshot(&id(0)).await.unwrap().is_on);
    assert!(!controller.snapshot(&id(1)).await.unwrap().is_on);
}

#[tokio::test]
async fn failed_persistence_does_not_publish_configuration_change() {
    let transport = FakeTransport::default();
    transport.configure(&endpoint(0), "serial-0", state(false, 50));
    let store = MemoryStore::default();
    *store.fail_save.lock().unwrap() = true;
    let controller =
        MultiDeviceController::load_with_clock(transport, store, FakeClock::default()).unwrap();
    assert!(matches!(
        controller.add(endpoint(0)).await,
        Err(ManagerError::Storage(_))
    ));
    assert!(controller.snapshots().await.is_empty());
}

#[tokio::test]
async fn failed_persistence_rolls_back_trusted_endpoint_update() {
    let transport = FakeTransport::default();
    let old_endpoint = endpoint(0);
    let new_endpoint = endpoint(1);
    transport.configure(&old_endpoint, "same-serial", state(false, 50));
    transport.configure(&new_endpoint, "same-serial", state(true, 70));
    let original = PersistedDevice::new(
        DeviceIdentity::serial("same-serial").unwrap(),
        DocumentName::new("Original").unwrap(),
        old_endpoint.clone(),
    );
    let store = MemoryStore::with_devices(vec![original]);
    let controller =
        MultiDeviceController::load_with_clock(transport, store.clone(), FakeClock::default())
            .unwrap();
    *store.fail_save.lock().unwrap() = true;
    assert!(matches!(
        controller.add(new_endpoint).await,
        Err(ManagerError::Storage(_))
    ));
    let snapshot = controller
        .snapshot(
            &DeviceIdentity::serial("same-serial")
                .unwrap()
                .canonical_id(),
        )
        .await
        .unwrap();
    assert_eq!(snapshot.endpoint, old_endpoint.to_string());
    assert_eq!(snapshot.name, "Original");
}

#[test]
fn canonical_ids_are_stable_and_unambiguous() {
    assert_eq!(
        DeviceIdentity::serial(" A:b ").unwrap().canonical_id(),
        "serial/613a62"
    );
    assert!(
        DeviceIdentity::mdns("Desk/Light", "Key Light", 53)
            .unwrap()
            .canonical_id()
            .starts_with("mdns/")
    );
}
