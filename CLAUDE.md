# ElgatoBar

macOS menu bar app for controlling Elgato Key Lights. Built with SwiftUI and Swift 6 strict concurrency.

## Project Overview

- **Target**: macOS 14+ menu bar application
- **Architecture**: SwiftUI App lifecycle with `MenuBarExtra` + `Window` scenes
- **Concurrency**: Swift 6 with `SWIFT_DEFAULT_ACTOR_ISOLATION = MainActor`

## Build Commands

```bash
# Build
xcodebuild -scheme ElgatoBar -configuration Debug build

# Clean build
xcodebuild clean -scheme ElgatoBar && xcodebuild -scheme ElgatoBar build
```

## Project Structure

```
ElgatoBar/
├── ElgatoBarApp.swift          # App entry point with MenuBarExtra + Window
├── AppState.swift              # @Observable state manager
├── Models/
│   └── Models.swift            # Light, LightState, LightingScene, API types
├── Services/
│   ├── LightClient.swift       # Actor-based HTTP client for Elgato API
│   ├── DiscoveryService.swift  # mDNS/Bonjour light discovery
│   └── PersistenceManager.swift # UserDefaults persistence
└── Views/
    ├── MenuBarView.swift       # Menu bar popover UI
    ├── LightRowView.swift      # Individual light control row
    └── SettingsView.swift      # Settings window with tabs
```

## Swift 6 Concurrency Rules

This project uses `SWIFT_DEFAULT_ACTOR_ISOLATION = MainActor`, meaning all types are MainActor-isolated by default. Follow these patterns:

### 1. Data Models - Mark as `Sendable` with `nonisolated` computed properties

```swift
struct Light: Codable, Hashable, Sendable {
    let id: UUID
    var ipAddress: String
    var port: Int

    // Computed properties accessed from actors must be nonisolated
    // Note: Force unwrap is acceptable here because ipAddress is validated on creation
    nonisolated var baseURL: URL {
        URL(string: "http://\(ipAddress):\(port)")!
    }
}

// For user-provided input, always use guard let:
guard let url = URL(string: "http://\(ip):\(port)/path") else {
    throw SomeError.invalidURL
}
```

### 2. API Types - Add `Sendable` and `nonisolated` initializers

```swift
struct ElgatoLightsRequest: Codable, Sendable {
    let lights: [ElgatoLightData]

    // Initializers called from actors must be nonisolated
    nonisolated init(state: LightState) {
        self.lights = [ElgatoLightData(on: state.isOn ? 1 : 0, ...)]
    }
}
```

### 3. Actor JSON Encoding/Decoding - Wrap in `MainActor.run`

```swift
actor LightClient {
    private let decoder = JSONDecoder()

    func getLightState(light: Light) async throws -> LightState {
        let (data, response) = try await session.data(from: url)

        // Decode on MainActor for Swift 6 isolation
        let lightsResponse = try await MainActor.run {
            try decoder.decode(ElgatoLightsResponse.self, from: data)
        }
        return lightsResponse.lights.first!.asLightState
    }
}
```

### 4. Weak Self in Nested Closures - Guard let before inner Task

```swift
// WRONG - captures var 'self' in concurrent code
Task { [weak self] in
    someHandler = {
        Task { @MainActor [weak self] in  // Error: self is still a var here
            self?.doSomething()
        }
    }
}

// CORRECT - unwrap self before nested closure
Task { [weak self] in
    guard let self else { return }
    someHandler = { [weak self] in
        guard let self else { return }
        Task { @MainActor in
            self.doSomething()  // self is now a let constant
        }
    }
}
```

### 5. Callback Handlers with Tasks - Guard let pattern

```swift
browser.stateUpdateHandler = { [weak self] state in
    guard let self else { return }  // Unwrap immediately
    Task { @MainActor in
        self.handleState(state)     // Now safe to use
    }
}
```

### 6. Continuation Resume Guards - Thread-safe flag class

When using `withCheckedContinuation` with callbacks that may fire multiple times, use a thread-safe flag to ensure the continuation only resumes once:

```swift
/// Thread-safe flag for tracking continuation resume state
private final class ResumeFlag: @unchecked Sendable {
    private let lock = NSLock()
    private nonisolated(unsafe) var _hasResumed = false

    /// Attempts to mark as resumed. Returns true if first call, false if already resumed.
    nonisolated func tryResume() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        if _hasResumed { return false }
        _hasResumed = true
        return true
    }
}

// Usage:
await withCheckedContinuation { continuation in
    let resumeFlag = ResumeFlag()

    connection.stateUpdateHandler = { state in
        switch state {
        case .ready:
            guard resumeFlag.tryResume() else { return }  // Only resume once
            // ... handle ready state ...
            continuation.resume()
        case .failed, .cancelled:
            guard resumeFlag.tryResume() else { return }  // Only resume once
            continuation.resume()
        default:
            break
        }
    }
}
```

### 7. ObservableObject in SwiftUI - Use @StateObject not @State

For classes conforming to `ObservableObject` with `@Published` properties, always use `@StateObject` (not `@State`):

```swift
// WRONG - @State doesn't observe @Published changes in classes
@State private var discovery = DiscoveryService()  // UI won't update!

// CORRECT - @StateObject properly observes ObservableObject
@StateObject private var discovery = DiscoveryService()  // UI updates correctly
```

- `@State` is for value types (structs, enums, primitives)
- `@StateObject` is for reference types (classes) that conform to `ObservableObject`
- `@ObservedObject` is for ObservableObject passed in from parent view

## Elgato Light API

- **Port**: 9123 (default)
- **No authentication required**
- **Endpoints**:
  - `GET /elgato/lights` - Get current state
  - `PUT /elgato/lights` - Set state (brightness, temperature, on/off)
  - `GET /elgato/accessory-info` - Device info (name, firmware, etc.)
  - `POST /elgato/identify` - Flash the light

- **mDNS Discovery**: Service type `_elg._tcp` in domain `local.`

## Entitlements

The app requires network access for communicating with lights:
- `com.apple.security.network.client` - Outgoing connections to lights

## Key Dependencies

- **Network.framework** - mDNS discovery via `NWBrowser`
- **Combine** - `@Published` properties in `DiscoveryService`
