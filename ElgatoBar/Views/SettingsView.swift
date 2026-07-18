//
//  SettingsView.swift
//  ElgatoBar
//
//  Settings window with tabs for configuration
//

import SwiftUI
import KeyboardShortcuts

struct SettingsView: View {
    @Bindable var state: AppState

    var body: some View {
        TabView {
            GeneralTab(state: state)
                .tabItem { Label("General", systemImage: "gear") }

            LightsTab(state: state)
                .tabItem { Label("Lights", systemImage: "lightbulb") }

            ScenesTab(state: state)
                .tabItem { Label("Scenes", systemImage: "theatermasks") }

            KeyboardTab(state: state)
                .tabItem { Label("Keyboard", systemImage: "keyboard") }
        }
        .frame(width: 450, height: 450)
    }
}

// MARK: - General Tab

struct GeneralTab: View {
    @Bindable var state: AppState

    var body: some View {
        Form {
            Section("Startup") {
                Toggle("Launch at Login", isOn: $state.launchAtLogin)
            }

            Section("Refresh") {
                Picker("Refresh Interval", selection: $state.settings.refreshInterval) {
                    Text("3 seconds").tag(3.0)
                    Text("5 seconds").tag(5.0)
                    Text("10 seconds").tag(10.0)
                    Text("30 seconds").tag(30.0)
                }
            }

            Section("Display") {
                Toggle("Show temperature in Kelvin", isOn: $state.settings.showTemperatureInKelvin)
            }

            Section("About") {
                HStack {
                    Text("ElgatoBar")
                    Spacer()
                    Text("Version 1.0")
                        .foregroundStyle(.secondary)
                }
            }
        }
        .formStyle(.grouped)
    }
}

// MARK: - Keyboard Tab

struct KeyboardTab: View {
    @Bindable var state: AppState

    var body: some View {
        Form {
            Section("Global Shortcuts") {
                HStack {
                    VStack(alignment: .leading) {
                        Text("Toggle All Lights")
                        Text("Turn all lights on or off")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    KeyboardShortcuts.Recorder(for: .toggleAllLights)
                }
            }

            Section("Scene Shortcuts") {
                if state.scenes.isEmpty {
                    HStack {
                        Image(systemName: "info.circle")
                            .foregroundStyle(.secondary)
                        Text("Create scenes in the Scenes tab to assign shortcuts")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                    }
                } else {
                    ForEach(state.scenes) { scene in
                        HStack {
                            VStack(alignment: .leading) {
                                Text(scene.name)
                                Text("Toggle scene on/off")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            Spacer()
                            KeyboardShortcuts.Recorder(for: .scene(scene.id))
                        }
                    }
                }
            }

            Section {
                Text("Shortcuts work system-wide, even when ElgatoBar is in the background.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
    }
}

// MARK: - Lights Tab

struct LightsTab: View {
    @Bindable var state: AppState
    @State private var newLightIP = ""
    @State private var newLightName = ""
    @State private var isAdding = false
    @State private var addError: String?
    @StateObject private var discovery = DiscoveryService()
    @StateObject private var networkScanner = NetworkScanner()
    @State private var newNetworkBase = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // mDNS Scan section
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Text("Local Network (mDNS)")
                        .font(.headline)

                    Spacer()

                    if discovery.isScanning {
                        ProgressView()
                            .scaleEffect(0.7)
                    }

                    Button(discovery.isScanning ? "Stop" : "Scan") {
                        if discovery.isScanning {
                            discovery.stopScan()
                        } else {
                            discovery.startScan()
                        }
                    }
                    .buttonStyle(.borderedProminent)
                }

                if let error = discovery.scanError {
                    Text(error)
                        .font(.caption)
                        .foregroundStyle(.red)
                }

                if !discovery.discoveredLights.isEmpty {
                    VStack(alignment: .leading, spacing: 4) {
                        ForEach(discovery.discoveredLights) { discovered in
                            DiscoveredLightRow(
                                discovered: discovered,
                                state: state,
                                isAlreadyAdded: state.lights.contains { $0.ipAddress == discovered.host }
                            )
                        }
                    }
                    .padding(.top, 4)
                }
            }
            .padding()

            Divider()

            // Network Scan section
            VStack(alignment: .leading, spacing: 8) {
                Text("Network Scan")
                    .font(.headline)

                // Show saved networks or auto-detect message
                if state.settings.scanNetworks.isEmpty {
                    Text("Networks: Local (auto-detect)")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(state.settings.scanNetworks) { network in
                        HStack {
                            Text(network.displayName)
                                .font(.caption)
                            Spacer()
                            Button {
                                state.removeScanNetwork(network)
                            } label: {
                                Image(systemName: "xmark.circle.fill")
                                    .foregroundStyle(.secondary)
                            }
                            .buttonStyle(.plain)
                        }
                    }
                }

                // Add network input
                HStack {
                    TextField("192.168.20", text: $newNetworkBase)
                        .textFieldStyle(.roundedBorder)
                        .frame(width: 120)

                    Button("Add Network") {
                        if !newNetworkBase.isEmpty {
                            state.addScanNetwork(newNetworkBase)
                            newNetworkBase = ""
                        }
                    }
                    .buttonStyle(.bordered)
                    .disabled(newNetworkBase.isEmpty)

                    Spacer()

                    if networkScanner.isScanning {
                        ProgressView()
                            .scaleEffect(0.7)
                    }

                    Button(networkScanner.isScanning ? "Stop" : "Scan Networks") {
                        if networkScanner.isScanning {
                            networkScanner.stopScan()
                        } else {
                            networkScanner.startScan(networks: state.settings.scanNetworks)
                        }
                    }
                    .buttonStyle(.borderedProminent)
                }

                // Progress
                if let progress = networkScanner.progress, networkScanner.isScanning {
                    HStack {
                        ProgressView(value: Double(progress.current), total: Double(progress.total))
                        Text("\(progress.current)/\(progress.total)")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }

                if let error = networkScanner.scanError {
                    Text(error)
                        .font(.caption)
                        .foregroundStyle(.red)
                }

                // Discovered lights from network scan
                if !networkScanner.discoveredLights.isEmpty {
                    VStack(alignment: .leading, spacing: 4) {
                        ForEach(networkScanner.discoveredLights) { discovered in
                            DiscoveredLightRow(
                                discovered: discovered,
                                state: state,
                                isAlreadyAdded: state.lights.contains { $0.ipAddress == discovered.host }
                            )
                        }
                    }
                    .padding(.top, 4)
                }
            }
            .padding()

            Divider()

            // Add light manually
            VStack(alignment: .leading, spacing: 8) {
                Text("Add Manually")
                    .font(.headline)

                HStack {
                    TextField("IP Address", text: $newLightIP)
                        .textFieldStyle(.roundedBorder)
                        .frame(width: 150)

                    TextField("Name (optional)", text: $newLightName)
                        .textFieldStyle(.roundedBorder)

                    Button("Add") {
                        addLight()
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(newLightIP.isEmpty || isAdding)

                    if isAdding {
                        ProgressView()
                            .scaleEffect(0.7)
                    }
                }

                if let error = addError {
                    Text(error)
                        .font(.caption)
                        .foregroundStyle(.red)
                }
            }
            .padding()

            Divider()

            // Lights list
            if state.lights.isEmpty {
                VStack {
                    Spacer()
                    Text("No lights configured")
                        .foregroundStyle(.secondary)
                    Text("Scan for lights or add manually by IP address")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                    Spacer()
                }
                .frame(maxWidth: .infinity)
            } else {
                List {
                    ForEach(state.lights) { light in
                        LightSettingsRow(light: light, state: state)
                    }
                }
            }
        }
    }

    private func addLight() {
        isAdding = true
        addError = nil

        Task {
            let name = newLightName.isEmpty ? nil : newLightName
            let success = await state.addManualLight(ip: newLightIP, name: name)

            await MainActor.run {
                isAdding = false
                if success {
                    newLightIP = ""
                    newLightName = ""
                } else {
                    addError = "Could not connect to light at \(newLightIP)"
                }
            }
        }
    }
}

struct DiscoveredLightRow: View {
    let discovered: DiscoveredLight
    @Bindable var state: AppState
    let isAlreadyAdded: Bool
    @State private var isAdding = false

    var body: some View {
        HStack {
            Image(systemName: "lightbulb.fill")
                .foregroundStyle(.yellow)

            VStack(alignment: .leading) {
                Text(discovered.displayName)
                Text("\(discovered.host):\(discovered.port)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Spacer()

            if isAlreadyAdded {
                Text("Added")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else if isAdding {
                ProgressView()
                    .scaleEffect(0.7)
            } else {
                Button("Add") {
                    addDiscoveredLight()
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
            }
        }
        .padding(.vertical, 2)
    }

    private func addDiscoveredLight() {
        isAdding = true
        Task {
            _ = await state.addManualLight(
                ip: discovered.host,
                name: discovered.displayName,
                port: discovered.port
            )
            await MainActor.run {
                isAdding = false
            }
        }
    }
}

struct LightSettingsRow: View {
    let light: Light
    @Bindable var state: AppState
    @State private var editName = ""
    @State private var isEditing = false

    var body: some View {
        HStack {
            Circle()
                .fill(light.isOnline ? .green : .red)
                .frame(width: 8, height: 8)

            VStack(alignment: .leading) {
                if isEditing {
                    TextField("Name", text: $editName, onCommit: {
                        state.updateLightName(light, name: editName)
                        isEditing = false
                    })
                    .textFieldStyle(.roundedBorder)
                } else {
                    Text(light.name)
                }

                Text(light.ipAddress)
                    .font(.caption)
                    .foregroundStyle(.secondary)

                if let info = light.accessoryInfo {
                    Text("\(info.productName) - FW \(info.firmwareVersion)")
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
            }

            Spacer()

            Button {
                editName = light.name
                isEditing.toggle()
            } label: {
                Image(systemName: "pencil")
            }
            .buttonStyle(.plain)

            Button {
                state.removeLight(light)
            } label: {
                Image(systemName: "trash")
                    .foregroundStyle(.red)
            }
            .buttonStyle(.plain)
        }
        .padding(.vertical, 4)
    }
}

// MARK: - Scenes Tab

struct ScenesTab: View {
    @Bindable var state: AppState
    @State private var showingNewSceneSheet = false
    @State private var showingSceneBuilder = false
    @State private var editingScene: LightingScene?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Header with + button
            HStack {
                Text("Scenes")
                    .font(.headline)

                Spacer()

                Button {
                    showingNewSceneSheet = true
                } label: {
                    Image(systemName: "plus")
                }
                .buttonStyle(.borderedProminent)
                .disabled(state.lights.isEmpty)
            }
            .padding()

            Divider()

            // Scenes list
            if state.scenes.isEmpty {
                VStack(spacing: 12) {
                    Spacer()
                    Image(systemName: "theatermasks")
                        .font(.largeTitle)
                        .foregroundStyle(.secondary)
                    Text("No scenes created")
                        .foregroundStyle(.secondary)
                    Text("Tap + to create your first scene")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                    Spacer()
                }
                .frame(maxWidth: .infinity)
            } else {
                List {
                    ForEach(state.scenes) { scene in
                        SceneSettingsRow(scene: scene, state: state) {
                            editingScene = scene
                            showingSceneBuilder = true
                        }
                    }
                }
            }
        }
        .sheet(isPresented: $showingNewSceneSheet) {
            NewSceneSheet(state: state) { useCurrentState in
                showingNewSceneSheet = false
                if useCurrentState {
                    // Will be handled in the sheet
                } else {
                    editingScene = nil
                    showingSceneBuilder = true
                }
            }
        }
        .sheet(isPresented: $showingSceneBuilder) {
            SceneBuilderView(state: state, existingScene: editingScene)
        }
    }
}

// MARK: - Scene Presets

struct ScenePreset: Identifiable {
    let id = UUID()
    let name: String
    let description: String
    let brightness: Int
    let temperature: Int
    let icon: String

    static let presets: [ScenePreset] = [
        ScenePreset(
            name: "Meeting Mode",
            description: "Balanced for video calls (~4350K)",
            brightness: 50,
            temperature: 230,
            icon: "video"
        ),
        ScenePreset(
            name: "Streaming Mode",
            description: "Bright neutral daylight (~5000K)",
            brightness: 75,
            temperature: 200,
            icon: "play.rectangle"
        ),
        ScenePreset(
            name: "Low Light / Evening",
            description: "Dim warm ambiance (~3125K)",
            brightness: 20,
            temperature: 320,
            icon: "moon"
        )
    ]
}

// MARK: - New Scene Sheet

struct NewSceneSheet: View {
    @Bindable var state: AppState
    @Environment(\.dismiss) var dismiss
    var onChoice: (Bool) -> Void  // true = from current, false = custom

    @State private var sceneName = ""
    @FocusState private var isNameFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            // Header
            HStack {
                Text("New Scene")
                    .font(.headline)
                Spacer()
                Button("Cancel") { dismiss() }
                    .buttonStyle(.plain)
            }
            .padding()

            Divider()

            ScrollView {
                VStack(spacing: 16) {
                    // Option 1: From Current State
                    VStack(alignment: .leading, spacing: 12) {
                        Text("Quick Capture")
                            .font(.subheadline)
                            .foregroundStyle(.secondary)

                        HStack {
                            TextField("Scene Name", text: $sceneName)
                                .textFieldStyle(.roundedBorder)
                                .focused($isNameFocused)

                            Button("Save Current") {
                                if !sceneName.isEmpty {
                                    _ = state.createScene(name: sceneName)
                                    dismiss()
                                }
                            }
                            .buttonStyle(.borderedProminent)
                            .disabled(sceneName.isEmpty)
                        }

                        Text("Captures the current state of all online lights")
                            .font(.caption)
                            .foregroundStyle(.tertiary)
                    }
                    .padding()
                    .background(Color.primary.opacity(0.03))
                    .cornerRadius(8)

                    // Divider with "or"
                    HStack {
                        Rectangle()
                            .fill(Color.secondary.opacity(0.3))
                            .frame(height: 1)
                        Text("or")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        Rectangle()
                            .fill(Color.secondary.opacity(0.3))
                            .frame(height: 1)
                    }

                    // Option 2: Presets
                    VStack(alignment: .leading, spacing: 12) {
                        Text("Add a Preset")
                            .font(.subheadline)
                            .foregroundStyle(.secondary)

                        ForEach(ScenePreset.presets) { preset in
                            PresetRowButton(preset: preset) {
                                addPresetScene(preset)
                            }
                        }

                        Text("Presets apply to all your lights")
                            .font(.caption)
                            .foregroundStyle(.tertiary)
                    }
                    .padding()
                    .background(Color.primary.opacity(0.03))
                    .cornerRadius(8)

                    // Divider with "or"
                    HStack {
                        Rectangle()
                            .fill(Color.secondary.opacity(0.3))
                            .frame(height: 1)
                        Text("or")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        Rectangle()
                            .fill(Color.secondary.opacity(0.3))
                            .frame(height: 1)
                    }

                    // Option 3: Custom Scene
                    VStack(alignment: .leading, spacing: 12) {
                        Text("Custom Scene")
                            .font(.subheadline)
                            .foregroundStyle(.secondary)

                        Button {
                            dismiss()
                            // Small delay to allow sheet to dismiss before opening new one
                            DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {
                                onChoice(false)
                            }
                        } label: {
                            HStack {
                                VStack(alignment: .leading) {
                                    Text("Create Custom Scene")
                                        .font(.body)
                                    Text("Choose specific lights and configure each one")
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                                Spacer()
                                Image(systemName: "chevron.right")
                                    .foregroundStyle(.secondary)
                            }
                            .padding()
                            .background(Color.accentColor.opacity(0.1))
                            .cornerRadius(8)
                        }
                        .buttonStyle(.plain)
                    }
                    .padding()
                    .background(Color.primary.opacity(0.03))
                    .cornerRadius(8)
                }
                .padding()
            }
        }
        .frame(width: 400, height: 450)
        .onAppear {
            isNameFocused = true
        }
    }

    private func addPresetScene(_ preset: ScenePreset) {
        // Check if scene with this name already exists
        if state.scenes.contains(where: { $0.name == preset.name }) {
            // Scene already exists, don't add duplicate
            dismiss()
            return
        }

        // Create configs for all lights using preset values
        let configs = state.lights.map { light in
            LightingScene.LightConfig(
                lightId: light.id,
                state: LightState(isOn: true, brightness: preset.brightness, temperature: preset.temperature)
            )
        }

        let scene = LightingScene(name: preset.name, lightConfigs: configs)
        state.saveScene(scene)
        dismiss()
    }
}

struct PresetRowButton: View {
    let preset: ScenePreset
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 12) {
                Image(systemName: preset.icon)
                    .font(.title3)
                    .foregroundStyle(.blue)
                    .frame(width: 24)

                VStack(alignment: .leading, spacing: 2) {
                    Text(preset.name)
                        .font(.body)
                        .foregroundStyle(.primary)
                    Text(preset.description)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                Spacer()

                Image(systemName: "plus.circle.fill")
                    .foregroundStyle(.blue)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(Color.blue.opacity(0.05))
            .cornerRadius(8)
        }
        .buttonStyle(.plain)
    }
}

struct SceneSettingsRow: View {
    let scene: LightingScene
    @Bindable var state: AppState
    var onEdit: () -> Void

    var isActive: Bool {
        state.activeSceneId == scene.id
    }

    var body: some View {
        HStack {
            // Active indicator
            Image(systemName: isActive ? "circle.fill" : "circle")
                .foregroundStyle(isActive ? .blue : .secondary)
                .font(.caption)

            VStack(alignment: .leading) {
                HStack {
                    Text(scene.name)
                    if isActive {
                        Text("Active")
                            .font(.caption2)
                            .foregroundStyle(.blue)
                            .padding(.horizontal, 4)
                            .padding(.vertical, 1)
                            .background(.blue.opacity(0.1))
                            .cornerRadius(4)
                    }
                }

                Text("\(scene.lightConfigs.count) light(s)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Spacer()

            Button {
                // Capture scene ID and look up fresh from state to avoid stale closure capture
                let sceneId = scene.id
                Task {
                    if let currentScene = state.scenes.first(where: { $0.id == sceneId }) {
                        await state.toggleScene(currentScene)
                    }
                }
            } label: {
                Image(systemName: isActive ? "stop.fill" : "play.fill")
            }
            .buttonStyle(.plain)
            .help(isActive ? "Turn Off Scene" : "Apply Scene")

            Button {
                onEdit()
            } label: {
                Image(systemName: "pencil")
            }
            .buttonStyle(.plain)
            .help("Edit Scene")

            Button {
                state.deleteScene(scene)
            } label: {
                Image(systemName: "trash")
                    .foregroundStyle(.red)
            }
            .buttonStyle(.plain)
            .help("Delete Scene")
        }
        .padding(.vertical, 4)
    }
}

// MARK: - Scene Builder

struct SceneBuilderView: View {
    @Bindable var state: AppState
    @Environment(\.dismiss) var dismiss

    var existingScene: LightingScene?

    @State private var sceneName = ""
    @State private var lightConfigs: [EditableLightConfig] = []
    @State private var newSceneId = UUID()  // For new scenes, pre-generate ID for hotkey binding

    struct EditableLightConfig: Identifiable {
        let id: UUID  // light.id
        let lightName: String
        var isIncluded: Bool
        var isOn: Bool
        var brightness: Int
        var temperature: Int
    }

    var isEditing: Bool { existingScene != nil }

    /// The scene ID to use for hotkey binding
    var sceneId: UUID { existingScene?.id ?? newSceneId }

    var body: some View {
        VStack(spacing: 0) {
            // Header
            HStack {
                Text(isEditing ? "Edit Scene" : "Create Custom Scene")
                    .font(.headline)
                Spacer()
                Button("Cancel") { dismiss() }
                    .buttonStyle(.plain)
            }
            .padding()

            Divider()

            // Scene name
            HStack {
                Text("Scene Name:")
                TextField("My Scene", text: $sceneName)
                    .textFieldStyle(.roundedBorder)
            }
            .padding(.horizontal)
            .padding(.top)

            // Hotkey
            HStack {
                Text("Hotkey:")
                Spacer()
                KeyboardShortcuts.Recorder(for: .scene(sceneId))
            }
            .padding(.horizontal)
            .padding(.bottom)

            Divider()

            // Light configurations
            Text("Configure Lights")
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal)
                .padding(.top, 8)

            ScrollView {
                VStack(spacing: 12) {
                    ForEach($lightConfigs) { $config in
                        SceneLightConfigRow(config: $config, showTemperatureInKelvin: state.settings.showTemperatureInKelvin)
                    }
                }
                .padding()
            }

            Divider()

            // Actions
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                    .buttonStyle(.bordered)
                Button(isEditing ? "Save Changes" : "Create Scene") {
                    saveScene()
                    dismiss()
                }
                .buttonStyle(.borderedProminent)
                .disabled(sceneName.isEmpty || !lightConfigs.contains { $0.isIncluded })
            }
            .padding()
        }
        .frame(width: 500, height: 500)
        .onAppear {
            initializeConfigs()
        }
    }

    private func initializeConfigs() {
        if let existing = existingScene {
            // Editing existing scene
            sceneName = existing.name
            lightConfigs = state.lights.map { light in
                if let config = existing.lightConfigs.first(where: { $0.lightId == light.id }) {
                    return EditableLightConfig(
                        id: light.id,
                        lightName: light.name,
                        isIncluded: true,
                        isOn: config.isOn,
                        brightness: config.brightness,
                        temperature: config.temperature
                    )
                } else {
                    return EditableLightConfig(
                        id: light.id,
                        lightName: light.name,
                        isIncluded: false,
                        isOn: true,
                        brightness: 50,
                        temperature: 200
                    )
                }
            }
        } else {
            // New scene - default all lights on with sensible defaults
            lightConfigs = state.lights.map { light in
                let currentState = light.currentState ?? LightState.defaultOn
                return EditableLightConfig(
                    id: light.id,
                    lightName: light.name,
                    isIncluded: true,
                    isOn: currentState.isOn,
                    brightness: currentState.brightness,
                    temperature: currentState.temperature
                )
            }
        }
    }

    private func saveScene() {
        let configs = lightConfigs
            .filter { $0.isIncluded }
            .map { config in
                LightingScene.LightConfig(
                    lightId: config.id,
                    state: LightState(isOn: config.isOn, brightness: config.brightness, temperature: config.temperature)
                )
            }

        if let existing = existingScene {
            // Update existing
            var updated = existing
            updated.name = sceneName
            updated.lightConfigs = configs
            state.saveScene(updated)
        } else {
            // Create new - use the pre-generated ID so hotkey binding is preserved
            let scene = LightingScene(id: newSceneId, name: sceneName, lightConfigs: configs)
            state.saveScene(scene)
        }
    }
}

struct SceneLightConfigRow: View {
    @Binding var config: SceneBuilderView.EditableLightConfig
    var showTemperatureInKelvin: Bool

    var temperatureDisplay: String {
        if showTemperatureInKelvin {
            let kelvin = Int(1_000_000 / Double(config.temperature))
            return "\(kelvin)K"
        } else {
            return "\(config.temperature)"
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            // Light name and include toggle
            HStack {
                Toggle(isOn: $config.isIncluded) {
                    Text(config.lightName)
                        .font(.headline)
                }
                .toggleStyle(.checkbox)
            }

            if config.isIncluded {
                // On/Off toggle
                HStack {
                    Text("State:")
                        .foregroundStyle(.secondary)
                    Picker("", selection: $config.isOn) {
                        Text("On").tag(true)
                        Text("Off").tag(false)
                    }
                    .pickerStyle(.segmented)
                    .frame(width: 100)
                    Spacer()
                }
                .padding(.leading, 20)

                // Brightness slider
                HStack {
                    Text("Brightness:")
                        .foregroundStyle(.secondary)
                    Slider(value: Binding(
                        get: { Double(config.brightness) },
                        set: { config.brightness = Int($0) }
                    ), in: 3...100)
                    Text("\(config.brightness)%")
                        .frame(width: 40, alignment: .trailing)
                        .foregroundStyle(.secondary)
                }
                .padding(.leading, 20)

                // Temperature slider
                HStack {
                    Text("Temperature:")
                        .foregroundStyle(.secondary)
                    Slider(value: Binding(
                        get: { Double(config.temperature) },
                        set: { config.temperature = Int($0) }
                    ), in: 143...344)
                    Text(temperatureDisplay)
                        .frame(width: 50, alignment: .trailing)
                        .foregroundStyle(.secondary)
                }
                .padding(.leading, 20)
            }
        }
        .padding()
        .background(config.isIncluded ? Color.accentColor.opacity(0.05) : Color.clear)
        .cornerRadius(8)
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .stroke(config.isIncluded ? Color.accentColor.opacity(0.2) : Color.gray.opacity(0.2), lineWidth: 1)
        )
    }
}

#Preview {
    SettingsView(state: AppState())
}
