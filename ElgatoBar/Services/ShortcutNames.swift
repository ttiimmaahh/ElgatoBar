//
//  ShortcutNames.swift
//  ElgatoBar
//
//  Keyboard shortcut name definitions for KeyboardShortcuts library
//

import Foundation
import KeyboardShortcuts

extension KeyboardShortcuts.Name {
    // Global toggle for all lights
    static let toggleAllLights = Self("toggleAllLights")

    // Dynamic scene shortcuts (created per-scene)
    static func scene(_ id: UUID) -> Self {
        Self("scene-\(id.uuidString)")
    }
}
