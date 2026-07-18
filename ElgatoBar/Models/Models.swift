//
//  Models.swift
//  ElgatoBar
//
//  Data models for Elgato Key Light management
//  Note: All model types are nonisolated and Sendable for Swift 6 concurrency
//

import Foundation

// MARK: - Light

/// Represents an Elgato light device
struct Light: Identifiable, Codable, Hashable, Sendable {
    let id: UUID
    var name: String
    var ipAddress: String
    var port: Int
    var isManuallyAdded: Bool

    // Runtime state (not persisted via Codable, updated at runtime)
    var isOnline: Bool = false
    var currentState: LightState?
    var accessoryInfo: AccessoryInfo?
    var consecutiveFailures: Int = 0  // Track failures across refresh cycles

    init(id: UUID = UUID(), name: String, ipAddress: String, port: Int = 9123, isManuallyAdded: Bool = true) {
        self.id = id
        self.name = name
        self.ipAddress = ipAddress
        self.port = port
        self.isManuallyAdded = isManuallyAdded
    }

    // Custom Codable to exclude runtime state
    enum CodingKeys: String, CodingKey {
        case id, name, ipAddress, port, isManuallyAdded
    }

    nonisolated var baseURL: URL {
        URL(string: "http://\(ipAddress):\(port)")!
    }

    /// Number of consecutive failures required before marking offline
    static let offlineThreshold = 2
}

// MARK: - Light State

/// Current state of a light (brightness, temperature, on/off)
struct LightState: Codable, Hashable, Sendable {
    var isOn: Bool
    var brightness: Int      // 0-100
    var temperature: Int     // 143-344 (inverse: 143=7000K cool, 344=2900K warm)

    /// Temperature in Kelvin (computed from API value)
    var temperatureKelvin: Int {
        // Elgato uses inverse: 143 = 7000K, 344 = 2900K
        // Formula: kelvin ≈ 1,000,000 / temperature
        Int(1_000_000 / Double(temperature))
    }

    /// Create from Kelvin value
    static func temperatureFromKelvin(_ kelvin: Int) -> Int {
        // Clamp to valid range 2900K-7000K
        let clamped = min(max(kelvin, 2900), 7000)
        return Int(1_000_000 / Double(clamped))
    }

    /// Default "on" state
    static var defaultOn: LightState {
        LightState(isOn: true, brightness: 50, temperature: 200)
    }

    /// Default "off" state
    static var defaultOff: LightState {
        LightState(isOn: false, brightness: 50, temperature: 200)
    }
}

// MARK: - Lighting Scene

/// A saved lighting scene/preset
struct LightingScene: Identifiable, Codable, Hashable, Sendable {
    let id: UUID
    var name: String
    var lightConfigs: [LightConfig]

    init(id: UUID = UUID(), name: String, lightConfigs: [LightConfig] = []) {
        self.id = id
        self.name = name
        self.lightConfigs = lightConfigs
    }

    /// Configuration for a single light within a scene
    struct LightConfig: Codable, Hashable, Sendable {
        let lightId: UUID
        var isOn: Bool
        var brightness: Int
        var temperature: Int

        init(lightId: UUID, state: LightState) {
            self.lightId = lightId
            self.isOn = state.isOn
            self.brightness = state.brightness
            self.temperature = state.temperature
        }

        nonisolated var asLightState: LightState {
            LightState(isOn: isOn, brightness: brightness, temperature: temperature)
        }
    }
}

// MARK: - Accessory Info (API Response)

/// Device information from /elgato/accessory-info
struct AccessoryInfo: Codable, Hashable, Sendable {
    let productName: String
    let hardwareBoardType: Int
    let firmwareBuildNumber: Int
    let firmwareVersion: String
    let serialNumber: String
    let displayName: String?
    let features: [String]?
    let wifiInfo: WifiInfo?

    enum CodingKeys: String, CodingKey {
        case productName, hardwareBoardType, firmwareBuildNumber, firmwareVersion
        case serialNumber, displayName, features
        case wifiInfo = "wifi-info"
    }

    struct WifiInfo: Codable, Hashable, Sendable {
        let ssid: String?
        let frequencyMHz: Int?
        let rssi: Int?
    }

    /// Best display name (displayName if set, otherwise productName)
    var bestName: String {
        if let displayName = displayName, !displayName.isEmpty {
            return displayName
        }
        return productName
    }
}

// MARK: - API Request/Response Types

/// API response for GET /elgato/lights
struct ElgatoLightsResponse: Codable, Sendable {
    let numberOfLights: Int
    let lights: [ElgatoLightData]
}

/// Individual light data in API response
struct ElgatoLightData: Codable, Sendable {
    let on: Int          // 0 or 1
    let brightness: Int  // 0-100
    let temperature: Int // 143-344

    nonisolated var asLightState: LightState {
        LightState(isOn: on == 1, brightness: brightness, temperature: temperature)
    }
}

/// API request for PUT /elgato/lights
struct ElgatoLightsRequest: Codable, Sendable {
    let lights: [ElgatoLightData]

    nonisolated init(state: LightState) {
        self.lights = [ElgatoLightData(on: state.isOn ? 1 : 0, brightness: state.brightness, temperature: state.temperature)]
    }
}

// MARK: - Scan Network

/// Persisted network for cross-VLAN scanning
struct ScanNetwork: Identifiable, Codable, Hashable, Sendable {
    let id: UUID
    var networkBase: String  // e.g., "192.168.20" or "192.168.20.0"

    init(id: UUID = UUID(), networkBase: String) {
        self.id = id
        // Normalize: strip trailing .0 if present
        self.networkBase = networkBase.replacingOccurrences(of: #"\.0$"#, with: "", options: .regularExpression)
    }

    /// Display name for UI
    nonisolated var displayName: String { "\(networkBase).0/24" }

    /// Generate all IPs to scan (.1 through .254)
    nonisolated var ipsToScan: [String] {
        (1...254).map { "\(networkBase).\($0)" }
    }
}

// MARK: - App Settings

/// Persisted app settings
struct AppSettings: Codable {
    var refreshInterval: TimeInterval = 5.0
    var showTemperatureInKelvin: Bool = true
    var scanNetworks: [ScanNetwork] = []  // Saved networks for scanning

    static var `default`: AppSettings {
        AppSettings()
    }
}
