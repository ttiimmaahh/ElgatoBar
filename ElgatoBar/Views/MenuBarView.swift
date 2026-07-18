//
//  MenuBarView.swift
//  ElgatoBar
//
//  Main menu bar popover UI
//

import SwiftUI

struct MenuBarView: View {
    @Bindable var state: AppState
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Header
            header

            Divider()

            // Lights section
            if state.lights.isEmpty {
                emptyLightsView
            } else {
                lightsSection
            }

            Divider()

            // Scenes section
            if !state.scenes.isEmpty {
                scenesSection
                Divider()
            }

            // Footer buttons
            footer
        }
        .frame(width: 320)
        .overlay(alignment: .top) {
            if state.showError, let message = state.lastError {
                ErrorToastView(message: message) {
                    state.dismissError()
                }
                .padding(.top, 4)
                .animation(.easeInOut(duration: 0.2), value: state.showError)
            }
        }
    }

    // MARK: - Header

    private var header: some View {
        HStack {
            Text("ElgatoBar")
                .font(.headline)

            Spacer()

            if state.isRefreshing {
                ProgressView()
                    .scaleEffect(0.7)
                    .frame(width: 16, height: 16)
            }

            Button {
                Task { await state.refresh() }
            } label: {
                Image(systemName: "arrow.clockwise")
            }
            .buttonStyle(.plain)
            .help("Refresh")
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
    }

    // MARK: - Lights Section

    private var lightsSection: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("LIGHTS")
                    .font(.caption)
                    .foregroundStyle(.secondary)

                Spacer()

                Button {
                    Task { await state.toggleAllLights() }
                } label: {
                    Image(systemName: state.lights.allSatisfy { $0.currentState?.isOn == true } ? "lightbulb.fill" : "lightbulb")
                        .foregroundStyle(.primary)
                }
                .buttonStyle(.plain)
                .help("Toggle All")
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 6)

            ForEach(state.lights) { light in
                LightRowView(light: light, state: state)
            }
        }
    }

    private var emptyLightsView: some View {
        VStack(spacing: 8) {
            Image(systemName: "lightbulb.slash")
                .font(.largeTitle)
                .foregroundStyle(.secondary)

            Text("No lights configured")
                .foregroundStyle(.secondary)

            Text("Add lights in Settings")
                .font(.caption)
                .foregroundStyle(.tertiary)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 24)
    }

    // MARK: - Scenes Section

    private var scenesSection: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("SCENES")
                .font(.caption)
                .foregroundStyle(.secondary)
                .padding(.horizontal, 12)
                .padding(.vertical, 6)

            ForEach(state.scenes) { scene in
                SceneRowView(scene: scene, state: state)
                    .id(scene.id)  // Explicit identity for proper view tracking
            }
        }
    }

    // MARK: - Footer

    private var footer: some View {
        HStack {
            Button {
                openWindow(id: "settings")
                NSApp.activate(ignoringOtherApps: true)
            } label: {
                Image(systemName: "gear")
                Text("Settings")
            }
            .buttonStyle(.plain)

            Spacer()

            Button {
                NSApplication.shared.terminate(nil)
            } label: {
                Text("Quit")
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
    }
}

// MARK: - Scene Row View

struct SceneRowView: View {
    let scene: LightingScene
    @Bindable var state: AppState

    var isActive: Bool {
        state.activeSceneId == scene.id
    }

    var body: some View {
        Button {
            // Capture scene ID and look up fresh from state to avoid stale closure capture
            let sceneId = scene.id
            Task {
                if let currentScene = state.scenes.first(where: { $0.id == sceneId }) {
                    await state.toggleScene(currentScene)
                }
            }
        } label: {
            HStack {
                // Active indicator
                Image(systemName: isActive ? "circle.fill" : "circle")
                    .font(.caption2)
                    .foregroundStyle(isActive ? .blue : .secondary)

                Text(scene.name)
                    .foregroundStyle(isActive ? .primary : .primary)

                if isActive {
                    Text("Active")
                        .font(.caption2)
                        .foregroundStyle(.blue)
                        .padding(.horizontal, 4)
                        .padding(.vertical, 1)
                        .background(.blue.opacity(0.15))
                        .cornerRadius(3)
                }

                Spacer()

                Image(systemName: isActive ? "stop.fill" : "play.fill")
                    .font(.caption)
                    .foregroundStyle(isActive ? .blue : .secondary)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .background(isActive ? Color.blue.opacity(0.08) : Color.primary.opacity(0.001))
    }
}

#Preview {
    MenuBarView(state: AppState())
}
