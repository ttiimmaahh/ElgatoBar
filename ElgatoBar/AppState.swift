//
//  AppState.swift
//  ElgatoBar
//
//  Observable state management for the application
//

import Foundation
import SwiftUI
import ServiceManagement
import os

private let logger = Logger(subsystem: "com.elgatobar", category: "AppState")

@Observable
final class AppState {
    // MARK: - Lights

    var lights: [Light] = []
    var isRefreshing = false

    // MARK: - Error State

    var lastError: String?
    var showError = false

    /// Show an error message to the user (auto-dismisses after delay)
    @MainActor
    func showError(_ message: String) {
        lastError = message
        showError = true

        // Auto-dismiss after 4 seconds
        Task {
            try? await Task.sleep(for: .seconds(4))
            if lastError == message {
                showError = false
            }
        }
    }

    /// Dismiss the current error
    @MainActor
    func dismissError() {
        showError = false
    }

    // MARK: - Scenes

    var scenes: [LightingScene] = [] {
        didSet {
            PersistenceManager.saveScenes(scenes)
            hotkeyManager?.setupSceneHotkeys(scenes)
        }
    }
    var activeSceneId: UUID?  // Currently active scene (nil = no scene active)

    // MARK: - Settings

    var settings: AppSettings = .default {
        didSet { PersistenceManager.saveSettings(settings) }
    }

    var launchAtLogin: Bool {
        get { SMAppService.mainApp.status == .enabled }
        set {
            do {
                if newValue {
                    try SMAppService.mainApp.register()
                } else {
                    try SMAppService.mainApp.unregister()
                }
            } catch {
                logger.error("Failed to update launch at login: \(error.localizedDescription)")
                showError("Failed to update launch at login setting")
            }
        }
    }

    // MARK: - Private

    private let client = LightClient()
    private var refreshTask: Task<Void, Never>?
    private var hotkeyManager: HotkeyManager?

    // MARK: - Init

    init() {
        loadPersistedData()
        setupHotkeyManager()
        startAutoRefresh()
    }

    private func setupHotkeyManager() {
        hotkeyManager = HotkeyManager(appState: self)
        hotkeyManager?.setupSceneHotkeys(scenes)
    }

    private func loadPersistedData() {
        lights = PersistenceManager.loadLights()
        scenes = PersistenceManager.loadScenes()
        settings = PersistenceManager.loadSettings()
    }

    // MARK: - Light Operations

    /// Refresh all lights' status
    @MainActor
    func refresh() async {
        guard !isRefreshing else { return }
        isRefreshing = true
        defer { isRefreshing = false }

        // Update each light's status in parallel with retry logic
        await withTaskGroup(of: (UUID, Bool, LightState?, AccessoryInfo?).self) { group in
            for light in lights {
                group.addTask {
                    // Single attempt with retry on failure
                    do {
                        let state = try await self.client.getLightState(light: light)
                        let info = try? await self.client.getAccessoryInfo(light: light)
                        return (light.id, true, state, info)
                    } catch {
                        // Brief delay then retry once
                        try? await Task.sleep(for: .milliseconds(500))
                        do {
                            let state = try await self.client.getLightState(light: light)
                            let info = try? await self.client.getAccessoryInfo(light: light)
                            return (light.id, true, state, info)
                        } catch {
                            return (light.id, false, nil, nil)
                        }
                    }
                }
            }

            for await (id, succeeded, state, info) in group {
                if let index = lights.firstIndex(where: { $0.id == id }) {
                    if succeeded {
                        // Success - reset failure counter and update state
                        lights[index].consecutiveFailures = 0
                        lights[index].isOnline = true
                        lights[index].currentState = state
                        if let info = info {
                            lights[index].accessoryInfo = info
                        }
                    } else {
                        // Failed - increment counter and check threshold
                        lights[index].consecutiveFailures += 1
                        if lights[index].consecutiveFailures >= Light.offlineThreshold {
                            lights[index].isOnline = false
                        }
                        // Keep existing state/info when failing (don't clear it)
                    }
                }
            }
        }
    }

    /// Toggle a light on/off
    @MainActor
    func toggleLight(_ light: Light) async {
        guard let index = lights.firstIndex(where: { $0.id == light.id }) else { return }

        do {
            let newState = try await client.toggleLight(light: light)
            lights[index].currentState = newState
            lights[index].isOnline = true
        } catch {
            lights[index].isOnline = false
            logger.error("Failed to toggle light '\(light.name)': \(error.localizedDescription)")
            showError("Failed to toggle \(light.name)")
        }
    }

    /// Update a light's state
    @MainActor
    func updateLight(_ light: Light, state: LightState) async {
        guard let index = lights.firstIndex(where: { $0.id == light.id }) else { return }

        do {
            try await client.setLightState(light: light, state: state)
            lights[index].currentState = state
            lights[index].isOnline = true
        } catch {
            lights[index].isOnline = false
            logger.error("Failed to update light '\(light.name)': \(error.localizedDescription)")
            showError("Failed to update \(light.name)")
        }
    }

    /// Update brightness for a light
    @MainActor
    func updateBrightness(_ light: Light, brightness: Int) async {
        guard let currentState = light.currentState else { return }
        let newState = LightState(isOn: currentState.isOn, brightness: brightness, temperature: currentState.temperature)
        await updateLight(light, state: newState)
    }

    /// Update temperature for a light
    @MainActor
    func updateTemperature(_ light: Light, temperature: Int) async {
        guard let currentState = light.currentState else { return }
        let newState = LightState(isOn: currentState.isOn, brightness: currentState.brightness, temperature: temperature)
        await updateLight(light, state: newState)
    }

    /// Identify a light (flash it)
    func identifyLight(_ light: Light) async {
        do {
            try await client.identify(light: light)
        } catch {
            logger.error("Failed to identify light '\(light.name)': \(error.localizedDescription)")
            showError("Failed to identify \(light.name)")
        }
    }

    /// Toggle all lights on/off
    @MainActor
    func toggleAllLights() async {
        let anyOn = lights.contains { $0.currentState?.isOn == true }
        let targetState = !anyOn

        for light in lights where light.isOnline {
            if let state = light.currentState {
                let newState = LightState(isOn: targetState, brightness: state.brightness, temperature: state.temperature)
                await updateLight(light, state: newState)
            }
        }
    }

    // MARK: - Light Management

    /// Add a light manually by IP address
    @MainActor
    func addManualLight(ip: String, name: String? = nil, port: Int = 9123) async -> Bool {
        // Check if already exists
        if lights.contains(where: { $0.ipAddress == ip && $0.port == port }) {
            return false
        }

        do {
            let info = try await client.getAccessoryInfo(ip: ip, port: port)
            let state = try await client.getLightState(light: Light(name: "", ipAddress: ip, port: port))

            let lightName = name ?? info.bestName
            var newLight = Light(name: lightName, ipAddress: ip, port: port, isManuallyAdded: true)
            newLight.isOnline = true
            newLight.currentState = state
            newLight.accessoryInfo = info

            lights.append(newLight)
            PersistenceManager.saveLights(lights)
            return true
        } catch {
            logger.error("Failed to add light at \(ip): \(error.localizedDescription)")
            showError("Failed to add light at \(ip)")
            return false
        }
    }

    /// Remove a light
    @MainActor
    func removeLight(_ light: Light) {
        lights.removeAll { $0.id == light.id }
        PersistenceManager.saveLights(lights)
    }

    /// Update a light's name
    @MainActor
    func updateLightName(_ light: Light, name: String) {
        if let index = lights.firstIndex(where: { $0.id == light.id }) {
            lights[index].name = name
            PersistenceManager.saveLights(lights)
        }
    }

    // MARK: - Scene Operations

    /// Apply a scene
    @MainActor
    func applyScene(_ scene: LightingScene) async {
        for config in scene.lightConfigs {
            if let light = lights.first(where: { $0.id == config.lightId }) {
                await updateLight(light, state: config.asLightState)
            }
        }
    }

    /// Create a scene from current light states
    @MainActor
    func createScene(name: String) -> LightingScene {
        let configs = lights.compactMap { light -> LightingScene.LightConfig? in
            guard let state = light.currentState else { return nil }
            return LightingScene.LightConfig(lightId: light.id, state: state)
        }

        let scene = LightingScene(name: name, lightConfigs: configs)
        scenes.append(scene)
        return scene
    }

    /// Update a scene
    @MainActor
    func updateScene(_ scene: LightingScene) {
        if let index = scenes.firstIndex(where: { $0.id == scene.id }) {
            scenes[index] = scene
        }
    }

    /// Delete a scene
    @MainActor
    func deleteScene(_ scene: LightingScene) {
        scenes.removeAll { $0.id == scene.id }
        // Clear active if deleted scene was active
        if activeSceneId == scene.id {
            activeSceneId = nil
        }
    }

    /// Toggle a scene on/off
    @MainActor
    func toggleScene(_ scene: LightingScene) async {
        if activeSceneId == scene.id {
            // Same scene - turn it off
            await turnOffSceneLights(scene)
            activeSceneId = nil
        } else {
            // Different scene - apply it
            await applyScene(scene)
            activeSceneId = scene.id
        }
    }

    /// Turn off all lights in a scene
    @MainActor
    func turnOffSceneLights(_ scene: LightingScene) async {
        for config in scene.lightConfigs {
            if let light = lights.first(where: { $0.id == config.lightId }) {
                let offState = LightState(isOn: false, brightness: config.brightness, temperature: config.temperature)
                await updateLight(light, state: offState)
            }
        }
    }

    /// Save a new scene from builder
    @MainActor
    func saveScene(_ scene: LightingScene) {
        if let index = scenes.firstIndex(where: { $0.id == scene.id }) {
            scenes[index] = scene
        } else {
            scenes.append(scene)
        }
    }

    // MARK: - Network Scanning

    /// Add a network for scanning
    @MainActor
    func addScanNetwork(_ networkBase: String) {
        // Normalize and validate
        let normalized = networkBase.trimmingCharacters(in: .whitespaces)
            .replacingOccurrences(of: #"\.0$"#, with: "", options: .regularExpression)

        // Basic validation: should look like "192.168.x" or "10.0.x" etc.
        let components = normalized.split(separator: ".")
        guard components.count == 3,
              components.allSatisfy({ Int($0) != nil && Int($0)! >= 0 && Int($0)! <= 255 }) else {
            return
        }

        // Check if already exists
        guard !settings.scanNetworks.contains(where: { $0.networkBase == normalized }) else {
            return
        }

        settings.scanNetworks.append(ScanNetwork(networkBase: normalized))
    }

    /// Remove a network from scanning
    @MainActor
    func removeScanNetwork(_ network: ScanNetwork) {
        settings.scanNetworks.removeAll { $0.id == network.id }
    }

    // MARK: - Auto Refresh

    private func startAutoRefresh() {
        refreshTask = Task {
            // Initial refresh
            await refresh()

            // Continuous refresh loop
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(settings.refreshInterval))
                if !Task.isCancelled {
                    await refresh()
                }
            }
        }
    }

    /// Stop auto-refresh (for cleanup)
    func stopAutoRefresh() {
        refreshTask?.cancel()
        refreshTask = nil
    }
}
