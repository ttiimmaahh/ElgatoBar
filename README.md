# ElgatoBar

A lightweight macOS menu bar app for controlling [Elgato Key Lights](https://www.elgato.com/us/en/p/key-light) and Ring Lights, built with SwiftUI and Swift 6 strict concurrency.

## Features

- **Menu bar control** — toggle power, adjust brightness and color temperature for each light without leaving your workflow
- **Automatic discovery** — finds lights on your network via mDNS/Bonjour (`_elg._tcp`)
- **Network scanning** — optional IP-range scan to locate lights that don't advertise over mDNS
- **Lighting scenes** — save brightness/temperature presets and apply them instantly
- **Global hotkeys** — bind keyboard shortcuts to scenes (powered by [KeyboardShortcuts](https://github.com/sindresorhus/KeyboardShortcuts))
- **Persistence** — remembers your lights and scenes across launches

## Requirements

- macOS 14 (Sonoma) or later
- Xcode 16+ (Swift 6)
- One or more Elgato Key Lights / Ring Lights on the same network

## Building

```bash
# Debug build
xcodebuild -scheme ElgatoBar -configuration Debug build

# Clean build
xcodebuild clean -scheme ElgatoBar && xcodebuild -scheme ElgatoBar build
```

Or open `ElgatoBar.xcodeproj` in Xcode and run the `ElgatoBar` scheme.

### Linux standalone milestone

The repository also contains an incremental Rust workspace for the Linux edition. The first milestone provides the portable control core and direct `elgatobar` CLI without moving or changing the macOS project. See [`linux/README.md`](linux/README.md) for commands, scope, validation, and security notes; shared protocol fixtures and the versioned interchange schema live under [`shared/`](shared/).

> **Note:** Code signing is set to *Automatic* with no development team baked in.
> Open the project in Xcode and select your own team under
> *Signing & Capabilities* before running on your machine.

## Architecture

SwiftUI App lifecycle with a `MenuBarExtra` + `Window` scene. State is managed
through an `@Observable` `AppState`, and light communication runs through an
actor-based HTTP client.

```
ElgatoBar/
├── ElgatoBarApp.swift          # App entry (MenuBarExtra + Settings window)
├── AppState.swift              # @Observable state manager
├── Models/Models.swift         # Light, LightState, LightingScene, API types
├── Services/
│   ├── LightClient.swift       # Actor-based HTTP client for the Elgato API
│   ├── DiscoveryService.swift  # mDNS/Bonjour discovery
│   ├── NetworkScanner.swift    # IP-range scanning fallback
│   ├── HotkeyManager.swift     # Global keyboard shortcuts
│   └── PersistenceManager.swift# UserDefaults persistence
└── Views/                      # Menu bar, light rows, settings UI
```

## Elgato Light API

The app talks to lights over their local HTTP API (no authentication required):

| Method | Endpoint | Purpose |
|--------|----------|---------|
| `GET`  | `/elgato/lights` | Get current state |
| `PUT`  | `/elgato/lights` | Set on/off, brightness, temperature |
| `GET`  | `/elgato/accessory-info` | Device info |
| `POST` | `/elgato/identify` | Flash the light |

Default port is `9123`. A [Postman collection](Postman/ElgatoKeyLight.postman_collection.json)
is included for exploring the API directly — set the `light_ip` variable to your
light's address.

## License

[MIT](LICENSE) © Tim Pearson
