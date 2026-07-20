# Platform repository layout

Status: proposed migration after the Linux first-run slice.

ElgatoBar is one product with independent macOS and Linux implementations. The
repository should make that relationship visible without implying that Swift
and Rust implementation code are shared.

## Target layout

```text
ElgatoBar/
├── apps/
│   ├── macos/
│   │   ├── ElgatoBar.xcodeproj/
│   │   ├── ElgatoBar/
│   │   ├── ElgatoBarTests/
│   │   └── ElgatoBarUITests/
│   └── linux/
│       ├── Cargo.toml
│       ├── Cargo.lock
│       ├── crates/
│       ├── systemd/
│       ├── waybar/
│       └── README.md
├── shared/
│   ├── api-fixtures/
│   ├── protocol/
│   └── schemas/
├── docs/
├── README.md
└── LICENSE
```

Each platform directory is a deep module. A platform maintainer should only
need its build tool and README to build and test that implementation. The
`shared` interface is intentionally narrow: versioned protocol documents,
recorded API fixtures, schemas, and the feature-parity record. It must not grow
platform runtime code or create source-level coupling between Swift and Rust.

## Migration rules

- Perform the move as a mechanical, dedicated change with no feature work.
- Move the Xcode project and all of its source/test directories together so its
  relative file references remain local to `apps/macos`.
- Move the Cargo workspace manifest and lockfile into `apps/linux`; workspace
  members become `crates/...` paths.
- Keep shared artifacts at the repository root and update both platforms' test
  fixture paths explicitly.
- Update root documentation and agent instructions to route platform commands
  through the appropriate application directory.
- Do not leave duplicate trees or long-lived compatibility symlinks.

## Acceptance gates

The layout change must not merge until both platform gates pass from the moved
paths:

### macOS

```bash
xcodebuild -project apps/macos/ElgatoBar.xcodeproj \
  -scheme ElgatoBar -configuration Debug build
xcodebuild -project apps/macos/ElgatoBar.xcodeproj \
  -scheme ElgatoBar test
```

Run these on macOS with the supported Xcode version. Linux cannot substitute
for this gate.

### Linux and shared contracts

```bash
cargo build --manifest-path apps/linux/Cargo.toml --workspace
cargo fmt --manifest-path apps/linux/Cargo.toml --all -- --check
cargo clippy --manifest-path apps/linux/Cargo.toml \
  --workspace --all-targets --all-features -- -D warnings
cargo test --manifest-path apps/linux/Cargo.toml --workspace --all-features
jq empty shared/schemas/*.json shared/api-fixtures/*.json
git diff --check
```

Keeping this migration separate preserves locality: a path regression is
attributable to the move, while discovery and first-run behavior remain
independently reviewable and reversible.
