# ElgatoBar feature parity roadmap

| Capability | macOS | Linux milestone 1 | Linux daemon foundation | Linux multi-device slice | Linux GTK controls |
| --- | --- | --- | --- | --- | --- |
| Read accessory/state | Shipped | Direct CLI + portable core | Daemon-owned cached snapshots and refresh | Persistent multi-device cache and selectors shipped | Complete replacement inventory with honest loading/offline/stale states |
| Set power/brightness/temperature | Shipped | Direct CLI, full-state writes | D-Bus client, serialized daemon writes | Per-device stable-ID commands shipped | Daemon-only GTK controls with coalesced range-safe sliders |
| Toggle preserving state | Shipped | Direct CLI/controller | D-Bus client/controller | Per-device and toggle-all shipped | Individual and daemon aggregate controls |
| Identify | Shipped | Direct CLI | D-Bus client | Stable-ID daemon/CLI command shipped; GTK deferred | Per-device action shipped |
| Human and structured automation output | Limited | Shipped | Shipped for one device | Per-device aggregate results and partial exit status shipped | Non-destructive categorized toast feedback |
| Stable cross-platform identities | Local UUID today | v1 serial/mDNS/installation hierarchy | Unchanged | Canonical selectors and trusted endpoint updates shipped; reassociation UI deferred | Stable IDs displayed and used for every action |
| Versioned interchange documents | Not yet | Schema and Rust model | Unchanged | macOS import/export + migrations | Unchanged |
| mDNS discovery/manual add/CIDR scan | Shipped | Deferred intentionally | One startup-configured endpoint | Explicit validated add shipped; discovery and scan deferred | Manual validated add only; discovery and scan deferred |
| Persistence/scenes | Shipped | Document shape only | Deferred intentionally | Atomic versioned XDG device/settings persistence shipped; scenes deferred | Daemon-owned persistence only; scenes deferred |
| Background polling/offline state | App lifetime | Deferred intentionally | systemd unit, polling, cached offline state, D-Bus signal | Concurrent capped polling, retry policy, and honest multi-device offline state shipped | Signal-driven replacement and daemon lifecycle recovery |
| Compact graphical controls | Menu bar | Deferred intentionally | Deferred intentionally | Reserved for next slice | GTK4/libadwaita single-instance client shipped |
| Waybar/global shortcuts/packaging | N/A | Deferred intentionally | Service unit only | Desktop integration milestones | Deferred intentionally |

Milestone 1 contained only `elgatobar-core` and `elgatobar-cli`. The daemon foundation added `elgatobar-dbus` and `elgatobar-daemon` and cut the CLI over to D-Bus. The multi-device slice persists explicitly configured devices and settings, exposes additive typed manager methods, and implements per-device and aggregate CLI workflows. The GTK slice adds the real `elgatobar-ui` client without changing the v1 D-Bus contract. No discovery, scanner, scene, settings, shortcut, Waybar, or packaging implementation is implied.

The daemon remains the sole shipped device poller and writer. CLI and GTK require D-Bus and do not silently fall back to direct access; a future Waybar client will follow the same rule.
