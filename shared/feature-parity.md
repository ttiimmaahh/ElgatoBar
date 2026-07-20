# ElgatoBar feature parity roadmap

| Capability | macOS | Linux milestone 1 | Linux daemon foundation | Linux multi-device slice |
| --- | --- | --- | --- | --- |
| Read accessory/state | Shipped | Direct CLI + portable core | Daemon-owned cached snapshots and refresh | Persistent multi-device cache and selectors shipped |
| Set power/brightness/temperature | Shipped | Direct CLI, full-state writes | D-Bus client, serialized daemon writes | Per-device stable-ID commands shipped |
| Toggle preserving state | Shipped | Direct CLI/controller | D-Bus client/controller | Per-device and toggle-all shipped |
| Identify | Shipped | Direct CLI | D-Bus client | Stable-ID daemon/CLI command shipped; GTK deferred |
| Human and structured automation output | Limited | Shipped | Shipped for one device | Per-device aggregate results and partial exit status shipped |
| Stable cross-platform identities | Local UUID today | v1 serial/mDNS/installation hierarchy | Unchanged | Canonical selectors and trusted endpoint updates shipped; reassociation UI deferred |
| Versioned interchange documents | Not yet | Schema and Rust model | Unchanged | macOS import/export + migrations |
| mDNS discovery/manual add/CIDR scan | Shipped | Deferred intentionally | One startup-configured endpoint | Explicit validated add shipped; discovery and scan deferred |
| Persistence/scenes | Shipped | Document shape only | Deferred intentionally | Atomic versioned XDG device/settings persistence shipped; scenes deferred |
| Background polling/offline state | App lifetime | Deferred intentionally | systemd unit, polling, cached offline state, D-Bus signal | Concurrent capped polling, retry policy, and honest multi-device offline state shipped |
| Compact graphical controls | Menu bar | Deferred intentionally | Deferred intentionally | Separate `elgatobar-ui` GTK4/libadwaita crate |
| Waybar/global shortcuts/packaging | N/A | Deferred intentionally | Service unit only | Desktop integration milestones |

Milestone 1 contained only `elgatobar-core` and `elgatobar-cli`. The daemon foundation added `elgatobar-dbus` and `elgatobar-daemon` and cut the CLI over to D-Bus. The multi-device slice now persists explicitly configured devices and settings, exposes additive typed manager methods, and implements per-device and aggregate CLI workflows. The future crate name `elgatobar-ui` is reserved here; no empty GTK, discovery, scanner, scene, or Waybar implementation is added merely to suggest completion.

With the daemon foundation, the daemon becomes the sole shipped device poller and writer. The CLI requires D-Bus and does not silently fall back to direct access; future Waybar and GTK clients will follow the same rule.
