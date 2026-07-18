//
//  ElgatoBarApp.swift
//  ElgatoBar
//
//  Elgato Key Light controller menu bar app
//

import SwiftUI

@main
struct ElgatoBarApp: App {
    @State private var state = AppState()

    var body: some Scene {
        MenuBarExtra {
            MenuBarView(state: state)
        } label: {
            Image("MenuBarIcon")
        }
        .menuBarExtraStyle(.window)

        Window("ElgatoBar Settings", id: "settings") {
            SettingsView(state: state)
        }
        .windowResizability(.contentSize)
    }
}
