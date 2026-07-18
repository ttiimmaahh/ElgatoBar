//
//  LightRowView.swift
//  ElgatoBar
//
//  Individual light control row
//

import SwiftUI

struct LightRowView: View {
    let light: Light
    @Bindable var state: AppState
    @State private var isExpanded = false
    @State private var localBrightness: Double = 50
    @State private var localTemperature: Double = 200
    @State private var isDraggingBrightness = false
    @State private var isDraggingTemperature = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Main row
            mainRow

            // Expanded controls
            if isExpanded && light.isOnline {
                expandedControls
            }
        }
        .background(Color.primary.opacity(0.001))
        .onChange(of: light.currentState) { _, newState in
            if !isDraggingBrightness, let brightness = newState?.brightness {
                localBrightness = Double(brightness)
            }
            if !isDraggingTemperature, let temperature = newState?.temperature {
                localTemperature = Double(temperature)
            }
        }
        .onAppear {
            if let state = light.currentState {
                localBrightness = Double(state.brightness)
                localTemperature = Double(state.temperature)
            }
        }
    }

    // MARK: - Main Row

    private var mainRow: some View {
        HStack(spacing: 8) {
            // Status indicator
            Circle()
                .fill(statusColor)
                .frame(width: 8, height: 8)

            // Light info
            VStack(alignment: .leading, spacing: 2) {
                Text(light.name)
                    .lineLimit(1)

                Text(light.ipAddress)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Spacer()

            if light.isOnline {
                // Toggle button
                Toggle("", isOn: Binding(
                    get: { light.currentState?.isOn ?? false },
                    set: { _ in
                        Task { await state.toggleLight(light) }
                    }
                ))
                .toggleStyle(.switch)
                .labelsHidden()
                .controlSize(.small)

                // Expand button
                Button {
                    withAnimation(.easeInOut(duration: 0.2)) {
                        isExpanded.toggle()
                    }
                } label: {
                    Image(systemName: isExpanded ? "chevron.up" : "chevron.down")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
            } else {
                Text("Offline")
                    .font(.caption)
                    .foregroundStyle(.red)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .contentShape(Rectangle())
    }

    // MARK: - Expanded Controls

    private var expandedControls: some View {
        VStack(alignment: .leading, spacing: 12) {
            // Brightness slider
            VStack(alignment: .leading, spacing: 4) {
                HStack {
                    Image(systemName: "sun.min")
                        .foregroundStyle(.secondary)

                    Slider(value: $localBrightness, in: 3...100, step: 1) { editing in
                        isDraggingBrightness = editing
                        if !editing {
                            Task {
                                await state.updateBrightness(light, brightness: Int(localBrightness))
                            }
                        }
                    }

                    Image(systemName: "sun.max")
                        .foregroundStyle(.secondary)

                    Text("\(Int(localBrightness))%")
                        .font(.caption)
                        .monospacedDigit()
                        .frame(width: 36, alignment: .trailing)
                }
            }

            // Temperature slider
            VStack(alignment: .leading, spacing: 4) {
                HStack {
                    Image(systemName: "thermometer.snowflake")
                        .foregroundStyle(.blue)

                    Slider(value: $localTemperature, in: 143...344, step: 1) { editing in
                        isDraggingTemperature = editing
                        if !editing {
                            Task {
                                await state.updateTemperature(light, temperature: Int(localTemperature))
                            }
                        }
                    }
                    .tint(temperatureGradient)

                    Image(systemName: "thermometer.sun")
                        .foregroundStyle(.orange)

                    Text(temperatureLabel)
                        .font(.caption)
                        .monospacedDigit()
                        .frame(width: 48, alignment: .trailing)
                }
            }

            // Action buttons
            HStack {
                Button {
                    Task { await state.identifyLight(light) }
                } label: {
                    Label("Identify", systemImage: "lightbulb.max")
                }
                .buttonStyle(.bordered)
                .controlSize(.small)

                Spacer()
            }
        }
        .padding(.horizontal, 12)
        .padding(.bottom, 12)
        .transition(.opacity.combined(with: .move(edge: .top)))
    }

    // MARK: - Helpers

    private var statusColor: Color {
        if !light.isOnline {
            return .red
        }
        if light.currentState?.isOn == true {
            return .green
        }
        return .gray
    }

    private var temperatureLabel: String {
        let kelvin = Int(1_000_000 / localTemperature)
        return "\(kelvin)K"
    }

    private var temperatureGradient: LinearGradient {
        LinearGradient(
            colors: [.blue, .white, .orange],
            startPoint: .leading,
            endPoint: .trailing
        )
    }
}

#Preview {
    VStack {
        LightRowView(
            light: Light(name: "Test Light", ipAddress: "192.168.1.100"),
            state: AppState()
        )
    }
    .frame(width: 320)
}
