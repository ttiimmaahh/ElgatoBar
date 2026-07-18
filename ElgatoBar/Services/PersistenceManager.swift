//
//  PersistenceManager.swift
//  ElgatoBar
//
//  UserDefaults-based persistence for lights, scenes, and settings
//

import Foundation

/// Handles persistence of lights, scenes, and settings using UserDefaults
struct PersistenceManager {
    private static let lightsKey = "elgatobar.lights"
    private static let scenesKey = "elgatobar.scenes"
    private static let settingsKey = "elgatobar.settings"

    private static let encoder = JSONEncoder()
    private static let decoder = JSONDecoder()

    // MARK: - Lights

    /// Save lights to UserDefaults
    static func saveLights(_ lights: [Light]) {
        // Only save manually added lights (discovered lights are transient)
        let manualLights = lights.filter { $0.isManuallyAdded }

        if let data = try? encoder.encode(manualLights) {
            UserDefaults.standard.set(data, forKey: lightsKey)
        }
    }

    /// Load lights from UserDefaults
    static func loadLights() -> [Light] {
        guard let data = UserDefaults.standard.data(forKey: lightsKey),
              let lights = try? decoder.decode([Light].self, from: data) else {
            return []
        }
        return lights
    }

    // MARK: - Scenes

    /// Save scenes to UserDefaults
    static func saveScenes(_ scenes: [LightingScene]) {
        if let data = try? encoder.encode(scenes) {
            UserDefaults.standard.set(data, forKey: scenesKey)
        }
    }

    /// Load scenes from UserDefaults
    static func loadScenes() -> [LightingScene] {
        guard let data = UserDefaults.standard.data(forKey: scenesKey),
              let scenes = try? decoder.decode([LightingScene].self, from: data) else {
            return []
        }
        return scenes
    }

    // MARK: - Settings

    /// Save settings to UserDefaults
    static func saveSettings(_ settings: AppSettings) {
        if let data = try? encoder.encode(settings) {
            UserDefaults.standard.set(data, forKey: settingsKey)
        }
    }

    /// Load settings from UserDefaults
    static func loadSettings() -> AppSettings {
        guard let data = UserDefaults.standard.data(forKey: settingsKey),
              let settings = try? decoder.decode(AppSettings.self, from: data) else {
            return .default
        }
        return settings
    }

    // MARK: - Reset

    /// Clear all persisted data
    static func resetAll() {
        UserDefaults.standard.removeObject(forKey: lightsKey)
        UserDefaults.standard.removeObject(forKey: scenesKey)
        UserDefaults.standard.removeObject(forKey: settingsKey)
    }
}
