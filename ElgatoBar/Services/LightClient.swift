//
//  LightClient.swift
//  ElgatoBar
//
//  Actor-based HTTP client for Elgato Light API
//

import Foundation

/// Errors that can occur when communicating with Elgato lights
enum LightClientError: LocalizedError {
    case unreachable(ip: String)
    case timeout(ip: String)
    case invalidResponse
    case apiError(statusCode: Int)
    case encodingError
    case decodingError(String)

    var errorDescription: String? {
        switch self {
        case .unreachable(let ip):
            return "Cannot reach light at \(ip)"
        case .timeout(let ip):
            return "Connection to \(ip) timed out"
        case .invalidResponse:
            return "Invalid response from light"
        case .apiError(let code):
            return "API error: HTTP \(code)"
        case .encodingError:
            return "Failed to encode request"
        case .decodingError(let detail):
            return "Failed to decode response: \(detail)"
        }
    }
}

/// Actor-based HTTP client for Elgato Light API
/// Thread-safe and designed for concurrent access
actor LightClient {
    private let session: URLSession
    private let timeout: TimeInterval
    private let decoder: JSONDecoder
    private let encoder: JSONEncoder

    init(timeout: TimeInterval = 5.0) {
        self.timeout = timeout

        let config = URLSessionConfiguration.default
        config.timeoutIntervalForRequest = timeout
        config.timeoutIntervalForResource = timeout
        self.session = URLSession(configuration: config)

        self.decoder = JSONDecoder()
        self.encoder = JSONEncoder()
    }

    // MARK: - Public API

    /// Fetch current light state
    func getLightState(light: Light) async throws -> LightState {
        let url = light.baseURL.appendingPathComponent("elgato/lights")

        do {
            let (data, response) = try await session.data(from: url)
            try validateResponse(response, for: light.ipAddress)

            // Decode on MainActor for Swift 6 isolation
            let lightsResponse = try await MainActor.run {
                try decoder.decode(ElgatoLightsResponse.self, from: data)
            }
            guard let firstLight = lightsResponse.lights.first else {
                throw LightClientError.invalidResponse
            }

            return firstLight.asLightState
        } catch let error as LightClientError {
            throw error
        } catch let error as URLError {
            throw mapURLError(error, ip: light.ipAddress)
        } catch let error as DecodingError {
            throw LightClientError.decodingError(error.localizedDescription)
        } catch {
            throw LightClientError.unreachable(ip: light.ipAddress)
        }
    }

    /// Update light state
    func setLightState(light: Light, state: LightState) async throws {
        let url = light.baseURL.appendingPathComponent("elgato/lights")

        var request = URLRequest(url: url)
        request.httpMethod = "PUT"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")

        // Encode on MainActor for Swift 6 isolation
        let requestBody = ElgatoLightsRequest(state: state)
        let body = try await MainActor.run {
            try encoder.encode(requestBody)
        }
        request.httpBody = body

        do {
            let (_, response) = try await session.data(for: request)
            try validateResponse(response, for: light.ipAddress)
        } catch let error as LightClientError {
            throw error
        } catch let error as URLError {
            throw mapURLError(error, ip: light.ipAddress)
        } catch {
            throw LightClientError.unreachable(ip: light.ipAddress)
        }
    }

    /// Toggle light on/off and return new state
    func toggleLight(light: Light) async throws -> LightState {
        let currentState = try await getLightState(light: light)
        let newState = LightState(
            isOn: !currentState.isOn,
            brightness: currentState.brightness,
            temperature: currentState.temperature
        )
        try await setLightState(light: light, state: newState)
        return newState
    }

    /// Get accessory info (device details)
    func getAccessoryInfo(light: Light) async throws -> AccessoryInfo {
        let url = light.baseURL.appendingPathComponent("elgato/accessory-info")

        do {
            let (data, response) = try await session.data(from: url)
            try validateResponse(response, for: light.ipAddress)

            // Decode on MainActor for Swift 6 isolation
            return try await MainActor.run {
                try decoder.decode(AccessoryInfo.self, from: data)
            }
        } catch let error as LightClientError {
            throw error
        } catch let error as URLError {
            throw mapURLError(error, ip: light.ipAddress)
        } catch let error as DecodingError {
            throw LightClientError.decodingError(error.localizedDescription)
        } catch {
            throw LightClientError.unreachable(ip: light.ipAddress)
        }
    }

    /// Get accessory info from IP address (for adding new lights)
    func getAccessoryInfo(ip: String, port: Int = 9123) async throws -> AccessoryInfo {
        guard let url = URL(string: "http://\(ip):\(port)/elgato/accessory-info") else {
            throw LightClientError.unreachable(ip: ip)
        }

        do {
            let (data, response) = try await session.data(from: url)
            try validateResponse(response, for: ip)

            // Decode on MainActor for Swift 6 isolation
            return try await MainActor.run {
                try decoder.decode(AccessoryInfo.self, from: data)
            }
        } catch let error as LightClientError {
            throw error
        } catch let error as URLError {
            throw mapURLError(error, ip: ip)
        } catch let error as DecodingError {
            throw LightClientError.decodingError(error.localizedDescription)
        } catch {
            throw LightClientError.unreachable(ip: ip)
        }
    }

    /// Identify light (flash it a few times)
    func identify(light: Light) async throws {
        let url = light.baseURL.appendingPathComponent("elgato/identify")

        var request = URLRequest(url: url)
        request.httpMethod = "POST"

        do {
            let (_, response) = try await session.data(for: request)
            try validateResponse(response, for: light.ipAddress)
        } catch let error as LightClientError {
            throw error
        } catch let error as URLError {
            throw mapURLError(error, ip: light.ipAddress)
        } catch {
            throw LightClientError.unreachable(ip: light.ipAddress)
        }
    }

    /// Check if light is reachable (non-throwing)
    func isReachable(light: Light) async -> Bool {
        do {
            _ = try await getLightState(light: light)
            return true
        } catch {
            return false
        }
    }

    /// Check if IP is reachable and has an Elgato light
    func isReachable(ip: String, port: Int = 9123) async -> Bool {
        do {
            _ = try await getAccessoryInfo(ip: ip, port: port)
            return true
        } catch {
            return false
        }
    }

    // MARK: - Private Helpers

    private func validateResponse(_ response: URLResponse, for ip: String) throws {
        guard let httpResponse = response as? HTTPURLResponse else {
            throw LightClientError.invalidResponse
        }

        guard (200...299).contains(httpResponse.statusCode) else {
            throw LightClientError.apiError(statusCode: httpResponse.statusCode)
        }
    }

    private func mapURLError(_ error: URLError, ip: String) -> LightClientError {
        switch error.code {
        case .timedOut:
            return .timeout(ip: ip)
        case .cannotConnectToHost, .networkConnectionLost, .notConnectedToInternet:
            return .unreachable(ip: ip)
        default:
            return .unreachable(ip: ip)
        }
    }
}
