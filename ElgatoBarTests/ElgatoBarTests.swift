//
//  ElgatoBarTests.swift
//  ElgatoBarTests
//
//  Unit tests for ElgatoBar models and persistence
//

import Testing
import Foundation
@testable import ElgatoBar

// MARK: - LightState Tests

struct LightStateTests {

    @Test func encodingDecoding() throws {
        let state = LightState(isOn: true, brightness: 75, temperature: 200)

        let encoder = JSONEncoder()
        let decoder = JSONDecoder()

        let data = try encoder.encode(state)
        let decoded = try decoder.decode(LightState.self, from: data)

        #expect(decoded.isOn == true)
        #expect(decoded.brightness == 75)
        #expect(decoded.temperature == 200)
    }

    @Test func temperatureKelvinConversion() {
        // 143 = ~7000K (cool), 344 = ~2900K (warm)
        let coolState = LightState(isOn: true, brightness: 50, temperature: 143)
        let warmState = LightState(isOn: true, brightness: 50, temperature: 344)

        // Cool should be around 7000K
        #expect(coolState.temperatureKelvin >= 6900)
        #expect(coolState.temperatureKelvin <= 7100)

        // Warm should be around 2900K
        #expect(warmState.temperatureKelvin >= 2850)
        #expect(warmState.temperatureKelvin <= 2950)
    }

    @Test func temperatureFromKelvin() {
        // Convert Kelvin back to API value
        let apiValue = LightState.temperatureFromKelvin(5000)
        #expect(apiValue == 200) // 1_000_000 / 5000 = 200
    }

    @Test func temperatureFromKelvinClamping() {
        // Values outside range should be clamped
        let tooHot = LightState.temperatureFromKelvin(1000) // Below 2900K
        let tooCold = LightState.temperatureFromKelvin(10000) // Above 7000K

        // Should clamp to 2900K (warm) -> ~345
        #expect(tooHot >= 340 && tooHot <= 350)

        // Should clamp to 7000K (cool) -> ~143
        #expect(tooCold >= 140 && tooCold <= 145)
    }

    @Test func defaultStates() {
        let on = LightState.defaultOn
        let off = LightState.defaultOff

        #expect(on.isOn == true)
        #expect(off.isOn == false)
        #expect(on.brightness == off.brightness)
        #expect(on.temperature == off.temperature)
    }
}

// MARK: - Light Tests

struct LightTests {

    @Test func baseURLComputation() {
        let light = Light(name: "Test Light", ipAddress: "192.168.1.100", port: 9123)

        #expect(light.baseURL.absoluteString == "http://192.168.1.100:9123")
    }

    @Test func baseURLWithCustomPort() {
        let light = Light(name: "Test Light", ipAddress: "10.0.0.50", port: 8080)

        #expect(light.baseURL.absoluteString == "http://10.0.0.50:8080")
    }

    @Test func defaultValues() {
        let light = Light(name: "My Light", ipAddress: "192.168.1.1")

        #expect(light.port == 9123)
        #expect(light.isManuallyAdded == true)
        #expect(light.isOnline == false)
        #expect(light.currentState == nil)
        #expect(light.consecutiveFailures == 0)
    }

    @Test func codableExcludesRuntimeState() throws {
        var light = Light(name: "Test", ipAddress: "192.168.1.1")
        light.isOnline = true
        light.consecutiveFailures = 5
        light.currentState = LightState.defaultOn

        let encoder = JSONEncoder()
        let decoder = JSONDecoder()

        let data = try encoder.encode(light)
        let decoded = try decoder.decode(Light.self, from: data)

        // Persisted fields should survive
        #expect(decoded.name == "Test")
        #expect(decoded.ipAddress == "192.168.1.1")

        // Runtime state should reset to defaults
        #expect(decoded.isOnline == false)
        #expect(decoded.consecutiveFailures == 0)
        #expect(decoded.currentState == nil)
    }

    @Test func offlineThreshold() {
        #expect(Light.offlineThreshold == 2)
    }
}

// MARK: - LightingScene Tests

struct LightingSceneTests {

    @Test func creation() {
        let scene = LightingScene(name: "Work Mode")

        #expect(scene.name == "Work Mode")
        #expect(scene.lightConfigs.isEmpty)
    }

    @Test func lightConfigFromState() {
        let lightId = UUID()
        let state = LightState(isOn: true, brightness: 80, temperature: 180)

        let config = LightingScene.LightConfig(lightId: lightId, state: state)

        #expect(config.lightId == lightId)
        #expect(config.isOn == true)
        #expect(config.brightness == 80)
        #expect(config.temperature == 180)
    }

    @Test func lightConfigAsLightState() {
        let lightId = UUID()
        let state = LightState(isOn: false, brightness: 30, temperature: 300)
        let config = LightingScene.LightConfig(lightId: lightId, state: state)

        let converted = config.asLightState

        #expect(converted.isOn == false)
        #expect(converted.brightness == 30)
        #expect(converted.temperature == 300)
    }

    @Test func encodingDecoding() throws {
        let lightId = UUID()
        let config = LightingScene.LightConfig(
            lightId: lightId,
            state: LightState(isOn: true, brightness: 50, temperature: 200)
        )
        let scene = LightingScene(name: "Test Scene", lightConfigs: [config])

        let encoder = JSONEncoder()
        let decoder = JSONDecoder()

        let data = try encoder.encode(scene)
        let decoded = try decoder.decode(LightingScene.self, from: data)

        #expect(decoded.name == "Test Scene")
        #expect(decoded.lightConfigs.count == 1)
        #expect(decoded.lightConfigs.first?.lightId == lightId)
    }
}

// MARK: - ScanNetwork Tests

struct ScanNetworkTests {

    @Test func normalization() {
        let network = ScanNetwork(networkBase: "192.168.1.0")

        // Should strip trailing .0
        #expect(network.networkBase == "192.168.1")
    }

    @Test func displayName() {
        let network = ScanNetwork(networkBase: "192.168.1")

        #expect(network.displayName == "192.168.1.0/24")
    }

    @Test func ipsToScan() {
        // Use "192.168.1" which won't be affected by .0 normalization
        let network = ScanNetwork(networkBase: "192.168.1")
        let ips = network.ipsToScan

        #expect(ips.count == 254)
        #expect(ips.first == "192.168.1.1")
        #expect(ips.last == "192.168.1.254")
    }
}

// MARK: - AppSettings Tests

struct AppSettingsTests {

    @Test func defaultValues() {
        let settings = AppSettings.default

        #expect(settings.refreshInterval == 5.0)
        #expect(settings.showTemperatureInKelvin == true)
        #expect(settings.scanNetworks.isEmpty)
    }

    @Test func encodingDecoding() throws {
        var settings = AppSettings()
        settings.refreshInterval = 10.0
        settings.showTemperatureInKelvin = false
        settings.scanNetworks = [ScanNetwork(networkBase: "192.168.1")]

        let encoder = JSONEncoder()
        let decoder = JSONDecoder()

        let data = try encoder.encode(settings)
        let decoded = try decoder.decode(AppSettings.self, from: data)

        #expect(decoded.refreshInterval == 10.0)
        #expect(decoded.showTemperatureInKelvin == false)
        #expect(decoded.scanNetworks.count == 1)
    }
}

// MARK: - API Types Tests

struct APITypesTests {

    @Test func elgatoLightDataAsLightState() {
        let data = ElgatoLightData(on: 1, brightness: 60, temperature: 250)
        let state = data.asLightState

        #expect(state.isOn == true)
        #expect(state.brightness == 60)
        #expect(state.temperature == 250)
    }

    @Test func elgatoLightDataOffState() {
        let data = ElgatoLightData(on: 0, brightness: 50, temperature: 200)
        let state = data.asLightState

        #expect(state.isOn == false)
    }

    @Test func elgatoLightsRequestFromState() {
        let state = LightState(isOn: true, brightness: 100, temperature: 143)
        let request = ElgatoLightsRequest(state: state)

        #expect(request.lights.count == 1)
        #expect(request.lights.first?.on == 1)
        #expect(request.lights.first?.brightness == 100)
        #expect(request.lights.first?.temperature == 143)
    }

    @Test func elgatoLightsRequestOffState() {
        let state = LightState(isOn: false, brightness: 50, temperature: 200)
        let request = ElgatoLightsRequest(state: state)

        #expect(request.lights.first?.on == 0)
    }
}

// MARK: - AccessoryInfo Tests

struct AccessoryInfoTests {

    @Test func bestNameWithDisplayName() throws {
        let json = """
        {
            "productName": "Elgato Key Light",
            "hardwareBoardType": 53,
            "firmwareBuildNumber": 218,
            "firmwareVersion": "1.0.3",
            "serialNumber": "ABC123",
            "displayName": "Desk Light"
        }
        """

        let info = try JSONDecoder().decode(AccessoryInfo.self, from: json.data(using: .utf8)!)

        #expect(info.bestName == "Desk Light")
    }

    @Test func bestNameFallsBackToProductName() throws {
        let json = """
        {
            "productName": "Elgato Key Light",
            "hardwareBoardType": 53,
            "firmwareBuildNumber": 218,
            "firmwareVersion": "1.0.3",
            "serialNumber": "ABC123"
        }
        """

        let info = try JSONDecoder().decode(AccessoryInfo.self, from: json.data(using: .utf8)!)

        #expect(info.bestName == "Elgato Key Light")
    }

    @Test func bestNameIgnoresEmptyDisplayName() throws {
        let json = """
        {
            "productName": "Elgato Key Light",
            "hardwareBoardType": 53,
            "firmwareBuildNumber": 218,
            "firmwareVersion": "1.0.3",
            "serialNumber": "ABC123",
            "displayName": ""
        }
        """

        let info = try JSONDecoder().decode(AccessoryInfo.self, from: json.data(using: .utf8)!)

        #expect(info.bestName == "Elgato Key Light")
    }
}
