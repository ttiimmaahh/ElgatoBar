# ElgatoBar for Linux

The Linux edition provides a portable Rust control core, a persistent multi-device user-session daemon, a typed D-Bus interface, and the `elgatobar` client CLI. The daemon is the only shipped Linux process that polls or writes lights. CLI, future GTK, and future Waybar clients require D-Bus and never silently fall back to direct device HTTP.

Discovery, network scanning, scenes, GTK, and Waybar remain later milestones. Devices are added explicitly by endpoint.

## Build and test

```bash
cargo build --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

The integration tests create isolated session buses and temporary XDG roots. They exercise persistence across daemon restarts, D-Bus methods and signals, CLI process behavior, retry/offline rules, aggregates, and partial failure without physical hardware.

## Storage and first-device setup

The daemon starts without an endpoint argument and creates default settings on first run. It follows XDG overrides:

- devices: `$XDG_DATA_HOME/elgatobar/devices-v1.json`, falling back to `~/.local/share/elgatobar/devices-v1.json`;
- settings: `$XDG_CONFIG_HOME/elgatobar/settings-v1.json`, falling back to `~/.config/elgatobar/settings-v1.json`.

Both JSON documents are versioned and atomically replaced. The settings file supports `refreshIntervalSeconds` values `3`, `5`, `10`, and `30`; the default is `5`. Unknown future versions stop daemon startup without rewriting the document. See [`../shared/protocol/elgatobar-linux-storage-v1.md`](../shared/protocol/elgatobar-linux-storage-v1.md).

Start the daemon in a graphical login session, then add the first light from another terminal on the same user bus:

```bash
cargo run -p elgatobar-daemon
cargo run -p elgatobar-cli -- devices add key-light.local
```

Adding validates both accessory-info and light-state before persistence. A serial-backed identity can retain its stable ID when its endpoint changes. An installation-local identity cannot silently move; remove the old local configuration and explicitly add the confirmed new endpoint.

## Multi-device CLI

Stable IDs are printed by `devices list` and are the selectors for device-specific operations:

```bash
elgatobar devices list
elgatobar devices add key-light.local
elgatobar devices remove 'serial/…'
elgatobar state
elgatobar state 'serial/…'
elgatobar refresh
elgatobar refresh 'serial/…'
elgatobar set 'serial/…' --on --brightness 75 --kelvin 5000
elgatobar toggle 'serial/…'
elgatobar toggle-all
elgatobar identify 'serial/…'
elgatobar --json devices list
```

`state` reads cache only. `refresh` polls all devices and returns one result per configured device. The daemon polls concurrently with at most eight device requests in flight, retries once after 500 ms, and marks a previously online device offline after two failed refresh cycles while retaining last-known power, brightness, and native temperature.

`toggle-all` skips offline devices. If any online light is on, it targets every online light off; otherwise it targets them all on. Results identify successes, failures, and skipped offline targets independently. There is no transactional hardware rollback after a partial network failure.

CLI exit statuses are:

| Status | Meaning |
| --- | --- |
| `0` | Full success |
| `2` | Invalid arguments, values, or selector |
| `3` | D-Bus/device connectivity, daemon absence, storage, or all-offline aggregate failure |
| `4` | Device HTTP/protocol/response failure |
| `5` | Partial aggregate failure (at least one success and at least one failure/skip) |

With `--json`, successful and partial aggregate output is one document on stdout; structured command errors are written to stderr.

## systemd user service

The repository includes `systemd/elgatobar.service`. Install the built daemon and unit through packaging or copy them to suitable user locations, then run:

```bash
systemctl --user daemon-reload
systemctl --user enable --now elgatobar.service
elgatobar devices add key-light.local
journalctl --user -u elgatobar.service
```

No `ELGATOBAR_ENDPOINT` environment file is required. The service acquires `io.github.ttiimmaahh.ElgatoBar1` on the user session bus; separate D-Bus activation is intentionally not installed.

## Security and hardware validation

Devices expose unauthenticated plaintext LAN HTTP. Run the daemon only on a trusted local network; requests disable redirects and environment proxy inheritance. See the shared HTTP and D-Bus protocol documents.

Hardware validation must use the daemon-backed CLI. Immediately before mutation, record current power, brightness, and native temperature; use cleanup that restores every field even after a failure; then re-read and compare exact values. Never record private device addresses or serial numbers in repository files or public issue comments.
