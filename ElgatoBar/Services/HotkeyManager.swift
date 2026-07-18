//
//  HotkeyManager.swift
//  ElgatoBar
//
//  Manages global keyboard shortcut registration and handling
//

import Foundation
import KeyboardShortcuts

@MainActor
final class HotkeyManager {
    private weak var appState: AppState?

    init(appState: AppState) {
        self.appState = appState
        setupGlobalToggle()
    }

    /// Setup the global toggle all lights hotkey
    func setupGlobalToggle() {
        KeyboardShortcuts.onKeyUp(for: .toggleAllLights) { [weak self] in
            Task { @MainActor in
                await self?.appState?.toggleAllLights()
            }
        }
    }

    /// Setup hotkeys for all scenes
    /// Call this when scenes are loaded or changed
    func setupSceneHotkeys(_ scenes: [LightingScene]) {
        for scene in scenes {
            setupSceneHotkey(scene)
        }
    }

    /// Setup hotkey for a single scene
    private func setupSceneHotkey(_ scene: LightingScene) {
        let sceneId = scene.id
        let name = KeyboardShortcuts.Name.scene(sceneId)

        KeyboardShortcuts.onKeyUp(for: name) { [weak self] in
            Task { @MainActor in
                guard let self = self,
                      let state = self.appState,
                      let currentScene = state.scenes.first(where: { $0.id == sceneId }) else {
                    return
                }
                await state.toggleScene(currentScene)
            }
        }
    }
}
