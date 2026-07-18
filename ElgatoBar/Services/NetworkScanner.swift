//
//  NetworkScanner.swift
//  ElgatoBar
//
//  IP range scanning service for discovering Elgato lights on other networks/VLANs
//

import Foundation
import Combine
import Darwin

/// Service for scanning IP ranges to discover Elgato lights
@MainActor
final class NetworkScanner: ObservableObject {
    @Published var isScanning = false
    @Published var progress: (current: Int, total: Int)?
    @Published var discoveredLights: [DiscoveredLight] = []
    @Published var scanError: String?

    private let client = LightClient(timeout: 1.5)  // Short timeout for probing
    private var scanTask: Task<Void, Never>?

    /// Scan networks - uses local network if no custom networks provided
    func startScan(networks: [ScanNetwork]) {
        stopScan()

        discoveredLights = []
        scanError = nil
        isScanning = true
        progress = nil

        let networksToScan = networks.isEmpty ? detectLocalNetworks() : networks

        if networksToScan.isEmpty {
            scanError = "Could not detect local network"
            isScanning = false
            return
        }

        scanTask = Task { [weak self] in
            await self?.scanNetworks(networksToScan)
        }
    }

    /// Stop scanning
    func stopScan() {
        scanTask?.cancel()
        scanTask = nil
        isScanning = false
        progress = nil
    }

    /// Detect the Mac's current network(s) from active interfaces
    func detectLocalNetworks() -> [ScanNetwork] {
        let networkBases = Self.getLocalNetworkBases()
        return networkBases.map { ScanNetwork(networkBase: $0) }
    }

    /// Get network base strings from active interfaces (nonisolated helper)
    nonisolated private static func getLocalNetworkBases() -> [String] {
        var networkBases: [String] = []
        var ifaddr: UnsafeMutablePointer<ifaddrs>?

        guard getifaddrs(&ifaddr) == 0, let firstAddr = ifaddr else { return [] }
        defer { freeifaddrs(ifaddr) }

        var ptr: UnsafeMutablePointer<ifaddrs>? = firstAddr
        while let current = ptr {
            let interface = current.pointee
            let family = interface.ifa_addr.pointee.sa_family

            // Only IPv4, skip loopback
            if family == UInt8(AF_INET),
               let namePtr = interface.ifa_name,
               let name = String(cString: namePtr, encoding: .utf8),
               name.hasPrefix("en") {

                // Extract IP address
                var addr = interface.ifa_addr.pointee
                var hostname = [CChar](repeating: 0, count: Int(NI_MAXHOST))
                getnameinfo(&addr, socklen_t(addr.sa_len), &hostname, socklen_t(hostname.count),
                            nil, 0, NI_NUMERICHOST)
                let ip = String(cString: hostname)

                // Derive network base (e.g., "192.168.10.45" -> "192.168.10")
                let components = ip.split(separator: ".")
                if components.count == 4 {
                    let networkBase = components.dropLast().joined(separator: ".")
                    // Avoid duplicates
                    if !networkBases.contains(networkBase) {
                        networkBases.append(networkBase)
                    }
                }
            }

            ptr = interface.ifa_next
        }

        return networkBases
    }

    /// Scan multiple networks
    private func scanNetworks(_ networks: [ScanNetwork]) async {
        let allIPs = networks.flatMap { $0.ipsToScan }
        let total = allIPs.count

        await MainActor.run {
            self.progress = (0, total)
        }

        // Use concurrent scanning with limited parallelism
        await withTaskGroup(of: DiscoveredLight?.self) { group in
            var completed = 0
            var activeCount = 0
            let maxConcurrent = 30

            for ip in allIPs {
                // Wait if we've hit max concurrent
                while activeCount >= maxConcurrent {
                    if let light = await group.next() {
                        activeCount -= 1
                        completed += 1
                        await MainActor.run {
                            self.progress = (completed, total)
                            if let light = light {
                                self.discoveredLights.append(light)
                            }
                        }
                    }
                }

                // Check for cancellation
                if Task.isCancelled { break }

                group.addTask { [weak self] in
                    await self?.probeIP(ip)
                }
                activeCount += 1
            }

            // Collect remaining results
            for await light in group {
                completed += 1
                await MainActor.run {
                    self.progress = (completed, total)
                    if let light = light {
                        self.discoveredLights.append(light)
                    }
                }
            }
        }

        await MainActor.run {
            self.isScanning = false
        }
    }

    /// Probe a single IP - returns DiscoveredLight if Elgato light found
    private func probeIP(_ ip: String, port: Int = 9123) async -> DiscoveredLight? {
        do {
            let info = try await client.getAccessoryInfo(ip: ip, port: port)
            return DiscoveredLight(
                name: info.bestName,
                host: ip,
                port: port
            )
        } catch {
            return nil
        }
    }
}
