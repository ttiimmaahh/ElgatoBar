# ElgatoBar feature parity roadmap

| Capability | macOS | Linux milestone 1 | Later Linux milestone |
| --- | --- | --- | --- |
| Read accessory/state | Shipped | Direct CLI + portable core | Daemon-owned snapshots |
| Set power/brightness/temperature | Shipped | Direct CLI, full-state writes | D-Bus clients |
| Toggle preserving state | Shipped | Direct CLI/controller | Aggregate daemon commands |
| Identify | Shipped | Direct CLI | GTK device management |
| Human and structured automation output | Limited | Shipped | Per-device partial results |
| Stable cross-platform identities | Local UUID today | v1 serial/mDNS/installation hierarchy | Explicit reassociation UI |
| Versioned interchange documents | Not yet | Schema and Rust model | macOS import/export + migrations |
| mDNS discovery/manual add/CIDR scan | Shipped | Deferred intentionally | Core adapters + daemon ownership |
| Persistence/scenes | Shipped | Document shape only; no placeholder adapter | Atomic XDG persistence + controller workflows |
| Background polling/offline state | App lifetime | Deferred intentionally | systemd user daemon + D-Bus |
| Compact graphical controls | Menu bar | Deferred intentionally | Separate `elgatobar-ui` GTK4/libadwaita crate |
| Waybar/global shortcuts/packaging | N/A | Deferred intentionally | Desktop integration milestones |

Milestone 1 contains only `elgatobar-core` and `elgatobar-cli`. The future crate name `elgatobar-ui` is reserved here; no empty GTK, daemon, D-Bus, discovery, scanner, or persistence implementation is added merely to suggest completion.

Once the daemon milestone ships, it becomes the sole device poller and mutable-data writer. CLI, Waybar, and GTK clients will require D-Bus and will not silently fall back to direct access.
