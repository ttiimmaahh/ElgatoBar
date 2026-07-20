# Shared platform contracts

Swift and Rust do not share executable source. They share only reviewed behavior contracts:

- `protocol/elgato-http-v1.md` — Elgato LAN HTTP methods, payloads, errors, and trust model.
- `protocol/elgatobar-dbus-v1.md` — Linux daemon/client service boundary, snapshots, signals, and errors.
- `api-fixtures/` — recorded known-good and edge-case JSON examples.
- `schemas/elgatobar-interchange-v1.schema.json` — versioned device/scene interchange format.
- `feature-parity.md` — delivered and deferred platform capabilities.

The Rust integration suite validates `api-fixtures/interchange-v1.json` against the JSON Schema. Future macOS import/export work should consume the same fixture and schema.

Interchange v1 writes UUIDs as lowercase hyphenated text, stores IPv6 hosts without brackets, rejects URL syntax in stored hosts, and writes integer-valued fields as JSON integer tokens. Readers accept mathematically integral JSON numbers such as `53.0`, as required by JSON Schema's `integer` semantics, and canonicalize them when writing.
