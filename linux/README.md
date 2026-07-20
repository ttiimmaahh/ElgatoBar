# ElgatoBar for Linux — standalone milestone

This milestone provides a portable Rust control core and the `elgatobar` direct CLI. It intentionally does not install a daemon or implement D-Bus, GTK, Waybar, discovery, scanning, or persistence. Direct access is temporary; the daemon milestone will make the daemon the only device poller and writer.

## Build and test

```bash
cargo build --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Direct CLI

Endpoints accept `host`, `host:port`, or `http://host:port`; omitted ports default to `9123`.

```bash
cargo run -p elgatobar-cli -- info key-light.local
cargo run -p elgatobar-cli -- state 192.168.1.20:9123
cargo run -p elgatobar-cli -- set 192.168.1.20 --on --brightness 75 --kelvin 5000
cargo run -p elgatobar-cli -- toggle 192.168.1.20
cargo run -p elgatobar-cli -- identify 192.168.1.20
cargo run -p elgatobar-cli -- --json state 192.168.1.20
```

Exit statuses are `0` for success, `2` for invalid arguments/endpoints/domain values, `3` for timeout or connectivity failure, and `4` for HTTP/protocol/response failure. A partial-failure status is reserved for later multi-device commands.

## Security and hardware validation

Devices expose unauthenticated plaintext LAN HTTP. Use the CLI only on a trusted local network; requests disable redirects and environment proxy inheritance. See `../shared/protocol/elgato-http-v1.md`.

No real-light endpoint was supplied for this implementation run, so hardware smoke testing was not run. Before release, record the current state, exercise info/state/set/toggle/identify, and restore the original state where practical.
