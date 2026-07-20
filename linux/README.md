# ElgatoBar for Linux

The current Linux edition provides a portable Rust control core, a user-session daemon, a versioned D-Bus API, and the `elgatobar` client CLI. This daemon foundation intentionally manages one manually configured endpoint. Discovery, scanning, persistence, scenes, aggregate commands, GTK, and Waybar remain later milestones.

The daemon is now the only shipped Linux component that polls or writes a light. The CLI requires D-Bus and never silently falls back to direct HTTP.

## Build and test

```bash
cargo build --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

The D-Bus integration tests create isolated session buses with `dbus-run-session` and exercise the daemon, public methods, state snapshots, signals, and CLI process boundary without physical hardware.

## Run from the workspace

Start the daemon in a graphical login session, replacing the endpoint with your light:

```bash
cargo run -p elgatobar-daemon -- --endpoint key-light.local
```

Then use the client from another terminal in the same user session:

```bash
cargo run -p elgatobar-cli -- info
cargo run -p elgatobar-cli -- state
cargo run -p elgatobar-cli -- refresh
cargo run -p elgatobar-cli -- set --on --brightness 75 --kelvin 5000
cargo run -p elgatobar-cli -- toggle
cargo run -p elgatobar-cli -- identify
cargo run -p elgatobar-cli -- --json state
```

`state` returns the daemon's cached snapshot; `refresh` performs an immediate device poll. The daemon otherwise polls every five seconds. CLI exit statuses are `0` for success, `2` for invalid arguments/domain values, `3` for daemon or device connectivity failure, and `4` for HTTP/protocol/response failure.

## systemd user service

The repository includes `systemd/elgatobar.service`. Install the built daemon and unit through packaging or copy them to suitable user locations, then create `~/.config/elgatobar/daemon.env`:

```ini
ELGATOBAR_ENDPOINT=key-light.local
```

Reload and enable the user service:

```bash
systemctl --user daemon-reload
systemctl --user enable --now elgatobar.service
journalctl --user -u elgatobar.service
```

The service acquires `io.github.ttiimmaahh.ElgatoBar1` on the user session bus. Separate D-Bus activation is intentionally not installed.

## Security and hardware validation

Devices expose unauthenticated plaintext LAN HTTP. Run the daemon only on a trusted local network; requests disable redirects and environment proxy inheritance. See `../shared/protocol/elgato-http-v1.md` and `../shared/protocol/elgatobar-dbus-v1.md`.

An Elgato Ring Light running firmware 1.0.4 was smoke-tested through the daemon and D-Bus CLI on 2026-07-20. The test exercised info, cached state, refresh, set, toggle, and identify, then restored and re-read the exact original power, brightness, and temperature state. The device address and serial number are intentionally not recorded in the repository.
