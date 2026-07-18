//
//  DiscoveryService.swift
//  ElgatoBar
//
//  mDNS discovery service for finding Elgato lights on the network
//

import Foundation
import Network
import Combine

/// Thread-safe flag for tracking continuation resume state
private final class ResumeFlag: @unchecked Sendable {
    private let lock = NSLock()
    private nonisolated(unsafe) var _hasResumed = false

    nonisolated var hasResumed: Bool {
        lock.lock()
        defer { lock.unlock() }
        return _hasResumed
    }

    /// Attempts to mark as resumed. Returns true if this is the first call, false if already resumed.
    nonisolated func tryResume() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        if _hasResumed { return false }
        _hasResumed = true
        return true
    }
}

/// Represents a discovered Elgato light from mDNS
struct DiscoveredLight: Identifiable, Hashable, Sendable {
    let id = UUID()
    let name: String
    let host: String
    let port: Int

    nonisolated var displayName: String {
        name.replacingOccurrences(of: "._elg._tcp.local.", with: "")
            .replacingOccurrences(of: "._elg._tcp", with: "")
    }
}

/// Service for discovering Elgato lights via mDNS/Bonjour
@MainActor
final class DiscoveryService: ObservableObject {
    @Published var discoveredLights: [DiscoveredLight] = []
    @Published var isScanning = false
    @Published var scanError: String?

    private var browser: NWBrowser?
    private var resolutionTasks: [String: Task<Void, Never>] = [:]

    /// Start scanning for Elgato lights
    func startScan() {
        stopScan()

        discoveredLights = []
        scanError = nil
        isScanning = true

        let parameters = NWParameters()
        parameters.includePeerToPeer = true

        let browser = NWBrowser(for: .bonjour(type: "_elg._tcp", domain: "local."), using: parameters)
        self.browser = browser

        browser.stateUpdateHandler = { [weak self] state in
            guard let self else { return }
            Task { @MainActor in
                self.handleBrowserState(state)
            }
        }

        browser.browseResultsChangedHandler = { [weak self] results, changes in
            guard let self else { return }
            Task { @MainActor in
                self.handleBrowseResults(results, changes: changes)
            }
        }

        browser.start(queue: .main)

        // Auto-stop after 10 seconds
        Task { [weak self] in
            try? await Task.sleep(for: .seconds(10))
            await MainActor.run {
                if self?.isScanning == true {
                    self?.stopScan()
                }
            }
        }
    }

    /// Stop scanning
    func stopScan() {
        browser?.cancel()
        browser = nil
        isScanning = false

        for (_, task) in resolutionTasks {
            task.cancel()
        }
        resolutionTasks.removeAll()
    }

    private func handleBrowserState(_ state: NWBrowser.State) {
        switch state {
        case .failed(let error):
            scanError = "Discovery failed: \(error.localizedDescription)"
            isScanning = false
        case .cancelled:
            isScanning = false
        case .ready:
            scanError = nil
        default:
            break
        }
    }

    private func handleBrowseResults(_ results: Set<NWBrowser.Result>, changes: Set<NWBrowser.Result.Change>) {
        for change in changes {
            switch change {
            case .added(let result):
                resolveEndpoint(result)
            case .removed(let result):
                removeLight(result)
            default:
                break
            }
        }
    }

    private func resolveEndpoint(_ result: NWBrowser.Result) {
        guard case .service(let name, _, _, _) = result.endpoint else { return }

        let task = Task { [weak self] in
            guard let self else { return }

            // Prefer IPv4 for Elgato lights
            let parameters = NWParameters.tcp
            parameters.requiredInterfaceType = .wifi
            if let ipOptions = parameters.defaultProtocolStack.internetProtocol as? NWProtocolIP.Options {
                ipOptions.version = .v4
            }

            let connection = NWConnection(to: result.endpoint, using: parameters)

            await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
                let resumeFlag = ResumeFlag()

                connection.stateUpdateHandler = { [weak self] state in
                    switch state {
                    case .ready:
                        // Ensure we only resume the continuation once
                        guard resumeFlag.tryResume() else { return }

                        if let innerEndpoint = connection.currentPath?.remoteEndpoint,
                           case .hostPort(let host, let port) = innerEndpoint {
                            let hostString: String
                            switch host {
                            case .ipv4(let addr):
                                // Get raw bytes and format as dotted-quad (avoids %interface suffix)
                                let bytes = addr.rawValue
                                hostString = bytes.map { String($0) }.joined(separator: ".")
                            case .ipv6(let addr):
                                // Strip interface suffix (e.g., %en0) if present
                                let raw = addr.debugDescription
                                hostString = raw.components(separatedBy: "%").first ?? raw
                            case .name(let hostname, _):
                                hostString = hostname
                            @unknown default:
                                hostString = host.debugDescription
                            }

                            let light = DiscoveredLight(
                                name: name,
                                host: hostString,
                                port: Int(port.rawValue)
                            )

                            if let self {
                                Task { @MainActor in
                                    self.addDiscoveredLight(light)
                                }
                            }
                        }
                        connection.cancel()
                        continuation.resume()
                    case .failed, .cancelled:
                        // Ensure we only resume the continuation once
                        guard resumeFlag.tryResume() else { return }
                        connection.cancel()
                        continuation.resume()
                    default:
                        break
                    }
                }
                connection.start(queue: .main)
            }
        }

        resolutionTasks[name] = task
    }

    private func addDiscoveredLight(_ light: DiscoveredLight) {
        if !discoveredLights.contains(where: { $0.host == light.host }) {
            discoveredLights.append(light)
        }
    }

    private func removeLight(_ result: NWBrowser.Result) {
        guard case .service(let name, _, _, _) = result.endpoint else { return }
        discoveredLights.removeAll { $0.name == name }
        resolutionTasks[name]?.cancel()
        resolutionTasks.removeValue(forKey: name)
    }

    deinit {
        browser?.cancel()
    }
}
