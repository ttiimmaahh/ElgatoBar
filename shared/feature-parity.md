# ElgatoBar feature parity roadmap

| Capability | macOS | Linux milestone 1 | Linux daemon foundation | Later Linux milestone |
| --- | --- | --- | --- | --- |
| Read accessory/state | Shipped | Direct CLI + portable core | Daemon-owned cached snapshots and refresh | Multiple devices |
| Set power/brightness/temperature | Shipped | Direct CLI, full-state writes | D-Bus client, serialized daemon writes | Aggregate commands |
| Toggle preserving state | Shipped | Direct CLI/controller | D-Bus client/controller | Aggregate daemon commands |
| Identify | Shipped | Direct CLI | D-Bus client | GTK device management |
| Human and structured automation output | Limited | Shipped | Shipped for one device | Per-device partial results |
| Stable cross-platform identities | Local UUID today | v1 serial/mDNS/installation hierarchy | Unchanged | Explicit reassociation UI |
| Versioned interchange documents | Not yet | Schema and Rust model | Unchanged | macOS import/export + migrations |
| mDNS discovery/manual add/CIDR scan | Shipped | Deferred intentionally | One startup-configured endpoint | Core adapters + daemon ownership |
| Persistence/scenes | Shipped | Document shape only | Deferred intentionally | Atomic XDG persistence + controller workflows |
| Background polling/offline state | App lifetime | Deferred intentionally | systemd unit, polling, cached offline state, D-Bus signal | Retry policy and multiple devices |
| Compact graphical controls | Menu bar | Deferred intentionally | Deferred intentionally | Separate `elgatobar-ui` GTK4/libadwaita crate |
| Waybar/global shortcuts/packaging | N/A | Deferred intentionally | Service unit only | Desktop integration milestones |

Milestone 1 contained only `elgatobar-core` and `elgatobar-cli`. The daemon foundation adds `elgatobar-dbus` and `elgatobar-daemon`, cuts the CLI over to D-Bus, and deliberately remains limited to one startup-configured endpoint. The future crate name `elgatobar-ui` is reserved here; no empty GTK, discovery, scanner, or persistence implementation is added merely to suggest completion.

With the daemon foundation, the daemon becomes the sole shipped device poller and writer. The CLI requires D-Bus and does not silently fall back to direct access; future Waybar and GTK clients will follow the same rule.
