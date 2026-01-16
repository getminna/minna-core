import Foundation
import Network

import Combine

// MARK: - MCP Protocol Types

/// JSON-RPC-like request format for the Rust MCP server
struct MCPRequest: Codable {
    let id: String?
    let tool: String?
    let method: String?
    let params: [String: AnyCodable]?

    init(tool: String, params: [String: Any] = [:], id: String? = UUID().uuidString) {
        self.id = id
        self.tool = tool
        self.method = nil
        self.params = params.mapValues { AnyCodable($0) }
    }

    init(method: String, params: [String: Any] = [:], id: String? = UUID().uuidString) {
        self.id = id
        self.tool = nil
        self.method = method
        self.params = params.mapValues { AnyCodable($0) }
    }
}

/// JSON-RPC-like response format from the Rust MCP server
struct MCPResponse: Codable {
    let id: String?
    let ok: Bool
    let result: AnyCodable?
    let error: String?
}

/// Type-erased Codable wrapper for dynamic JSON values
struct AnyCodable: Codable {
    let value: Any

    init(_ value: Any) {
        self.value = value
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() {
            value = NSNull()
        } else if let bool = try? container.decode(Bool.self) {
            value = bool
        } else if let int = try? container.decode(Int.self) {
            value = int
        } else if let double = try? container.decode(Double.self) {
            value = double
        } else if let string = try? container.decode(String.self) {
            value = string
        } else if let array = try? container.decode([AnyCodable].self) {
            value = array.map { $0.value }
        } else if let dict = try? container.decode([String: AnyCodable].self) {
            value = dict.mapValues { $0.value }
        } else {
            throw DecodingError.dataCorruptedError(in: container, debugDescription: "Unsupported type")
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        switch value {
        case is NSNull:
            try container.encodeNil()
        case let bool as Bool:
            try container.encode(bool)
        case let int as Int:
            try container.encode(int)
        case let double as Double:
            try container.encode(double)
        case let string as String:
            try container.encode(string)
        case let array as [Any]:
            try container.encode(array.map { AnyCodable($0) })
        case let dict as [String: Any]:
            try container.encode(dict.mapValues { AnyCodable($0) })
        default:
            try container.encodeNil()
        }
    }
}

// MARK: - MCP Client

/// Client for communicating with the Rust MCP server over Unix socket
/// Marked as @unchecked Sendable because all mutable state is protected by dispatch queues
class MCPClient: ObservableObject, @unchecked Sendable {
    static let shared = MCPClient()

    // MARK: - Published State

    @Published private(set) var isConnected = false
    @Published private(set) var lastError: String?

    // MARK: - Private Properties

    // MCP socket (read-only queries for AI clients)
    private var connection: NWConnection?
    private let queue = DispatchQueue(label: "app.minna.mcp.client", qos: .userInitiated)
    private var pendingRequests: [String: CheckedContinuation<MCPResponse, Error>] = [:]
    private var receiveBuffer = Data()

    // Admin socket (control operations for Swift app)
    private var adminConnection: NWConnection?
    private let adminQueue = DispatchQueue(label: "app.minna.mcp.admin", qos: .userInitiated)
    private var adminPendingRequests: [String: CheckedContinuation<MCPResponse, Error>] = [:]
    private var adminReceiveBuffer = Data()
    @Published private(set) var isAdminConnected = false

    // MARK: - Socket Paths

    private var socketPath: String {
        let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        return appSupport.appendingPathComponent("Minna/mcp.sock").path
    }

    private var adminSocketPath: String {
        let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        return appSupport.appendingPathComponent("Minna/admin.sock").path
    }

    // MARK: - Initialization

    private init() {}

    // MARK: - Connection Management

    /// Connect to the MCP server socket
    func connect() {
        guard connection == nil else {
            print("MCPClient: Already connected or connecting")
            return
        }

        let endpoint = NWEndpoint.unix(path: socketPath)
        let params = NWParameters()
        params.defaultProtocolStack.transportProtocol = NWProtocolTCP.Options()

        let conn = NWConnection(to: endpoint, using: params)
        connection = conn

        conn.stateUpdateHandler = { [weak self] state in
            DispatchQueue.main.async {
                switch state {
                case .ready:
                    print("MCPClient: Connected to \(self?.socketPath ?? "")")
                    self?.isConnected = true
                    self?.lastError = nil
                    self?.startReceiving()
                case .failed(let error):
                    print("MCPClient: Connection failed - \(error)")
                    self?.isConnected = false
                    self?.lastError = error.localizedDescription
                    self?.connection = nil
                case .cancelled:
                    print("MCPClient: Connection cancelled")
                    self?.isConnected = false
                    self?.connection = nil
                case .waiting(let error):
                    print("MCPClient: Waiting - \(error)")
                    self?.lastError = "Waiting: \(error.localizedDescription)"
                default:
                    break
                }
            }
        }

        conn.start(queue: queue)
    }

    /// Disconnect from the MCP server
    func disconnect() {
        connection?.cancel()
        connection = nil
        DispatchQueue.main.async {
            self.isConnected = false
        }
    }

    // MARK: - Admin Connection Management

    /// Connect to the admin socket
    func connectAdmin() {
        guard adminConnection == nil else {
            print("MCPClient: Admin already connected or connecting")
            return
        }

        print("MCPClient: Attempting admin connection to \(adminSocketPath)")
        let endpoint = NWEndpoint.unix(path: adminSocketPath)
        let params = NWParameters()
        params.defaultProtocolStack.transportProtocol = NWProtocolTCP.Options()

        let conn = NWConnection(to: endpoint, using: params)
        adminConnection = conn

        conn.stateUpdateHandler = { [weak self] state in
            print("MCPClient: Admin state changed to \(state)")
            DispatchQueue.main.async { [weak self] in
                switch state {
                case .ready:
                    print("MCPClient: Admin connected to \(self?.adminSocketPath ?? "")")
                    self?.isAdminConnected = true
                    self?.startAdminReceiving()
                case .failed(let error):
                    print("MCPClient: Admin connection failed - \(error)")
                    self?.isAdminConnected = false
                    self?.adminConnection = nil
                case .cancelled:
                    print("MCPClient: Admin connection cancelled")
                    self?.isAdminConnected = false
                    self?.adminConnection = nil
                case .waiting(let error):
                    print("MCPClient: Admin waiting - \(error)")
                case .preparing:
                    print("MCPClient: Admin preparing...")
                default:
                    break
                }
            }
        }

        conn.start(queue: adminQueue)
        print("MCPClient: Admin connection started")
    }

    /// Disconnect from the admin socket
    func disconnectAdmin() {
        adminConnection?.cancel()
        adminConnection = nil
        DispatchQueue.main.async { [weak self] in
            self?.isAdminConnected = false
        }
    }

    // MARK: - Send Request

    /// Send a tool request and await the response
    func send(tool: String, params: [String: Any] = [:]) async throws -> MCPResponse {
        let request = MCPRequest(tool: tool, params: params)
        return try await sendRequest(request)
    }

    /// Send a method request and await the response
    func send(method: String, params: [String: Any] = [:]) async throws -> MCPResponse {
        let request = MCPRequest(method: method, params: params)
        return try await sendRequest(request)
    }

    /// Send a tool request to the admin socket
    func sendAdmin(tool: String, params: [String: Any] = [:], timeout: TimeInterval = 30) async throws -> MCPResponse {
        let request = MCPRequest(tool: tool, params: params)
        return try await sendAdminRequest(request, timeout: timeout)
    }

    private func sendRequest(_ request: MCPRequest) async throws -> MCPResponse {
        guard let conn = connection, isConnected else {
            throw MCPError.notConnected
        }

        let encoder = JSONEncoder()
        var data = try encoder.encode(request)
        data.append(contentsOf: "\n".utf8) // Protocol uses newline-delimited JSON

        return try await withCheckedThrowingContinuation { continuation in
            if let requestId = request.id {
                self.queue.sync {
                    self.pendingRequests[requestId] = continuation
                }
            }

            conn.send(content: data, completion: .contentProcessed { [weak self] error in
                if let error = error {
                    if let requestId = request.id, let self = self {
                        self.queue.sync {
                            _ = self.pendingRequests.removeValue(forKey: requestId)
                        }
                    }
                    continuation.resume(throwing: MCPError.sendFailed(error.localizedDescription))
                }
            })
        }
    }

    private func sendAdminRequest(_ request: MCPRequest, timeout: TimeInterval = 30) async throws -> MCPResponse {
        // #region agent log
        let logPath = "/Users/wp/Antigravity/.cursor/debug.log"
        let requestId = request.id ?? UUID().uuidString
        try? "{\"timestamp\":\(Int(Date().timeIntervalSince1970 * 1000)),\"location\":\"MCPClient.swift:sendAdminRequest:entry\",\"message\":\"sendAdminRequest called\",\"data\":{\"requestId\":\"\(requestId)\",\"tool\":\"\(request.tool ?? "none")\",\"isAdminConnected\":\(isAdminConnected),\"hasConnection\":\(adminConnection != nil),\"sessionId\":\"debug-session\",\"runId\":\"run1\",\"hypothesisId\":\"A\"}}\n".appendLine(toFile: logPath)
        // #endregion agent log
        
        guard let conn = adminConnection, isAdminConnected else {
            // #region agent log
            try? "{\"timestamp\":\(Int(Date().timeIntervalSince1970 * 1000)),\"location\":\"MCPClient.swift:sendAdminRequest:not_connected\",\"message\":\"Not connected to admin socket\",\"data\":{\"requestId\":\"\(requestId)\",\"sessionId\":\"debug-session\",\"runId\":\"run1\",\"hypothesisId\":\"A\"}}\n".appendLine(toFile: logPath)
            // #endregion agent log
            throw MCPError.notConnected
        }

        let encoder = JSONEncoder()
        var data = try encoder.encode(request)
        data.append(contentsOf: "\n".utf8)

        // #region agent log
        try? "{\"timestamp\":\(Int(Date().timeIntervalSince1970 * 1000)),\"location\":\"MCPClient.swift:sendAdminRequest:before_send\",\"message\":\"About to send data\",\"data\":{\"requestId\":\"\(requestId)\",\"dataLength\":\(data.count),\"sessionId\":\"debug-session\",\"runId\":\"run1\",\"hypothesisId\":\"A\"}}\n".appendLine(toFile: logPath)
        // #endregion agent log

        // Start a timeout task that will fire if the request takes too long
        let timeoutTask = Task {
            try await Task.sleep(nanoseconds: UInt64(timeout * 1_000_000_000))
            // #region agent log
            try? "{\"timestamp\":\(Int(Date().timeIntervalSince1970 * 1000)),\"location\":\"MCPClient.swift:sendAdminRequest:timeout\",\"message\":\"Request timeout fired\",\"data\":{\"requestId\":\"\(requestId)\",\"timeout\":\(timeout),\"sessionId\":\"debug-session\",\"runId\":\"run1\",\"hypothesisId\":\"A\"}}\n".appendLine(toFile: logPath)
            // #endregion agent log
            // If we get here without being cancelled, trigger timeout
            self.adminQueue.sync {
                if let continuation = self.adminPendingRequests.removeValue(forKey: requestId) {
                    continuation.resume(throwing: MCPError.timeout)
                }
            }
        }

        defer {
            timeoutTask.cancel()
        }

        return try await withCheckedThrowingContinuation { continuation in
            self.adminQueue.sync {
                self.adminPendingRequests[requestId] = continuation
            }
            
            // #region agent log
            try? "{\"timestamp\":\(Int(Date().timeIntervalSince1970 * 1000)),\"location\":\"MCPClient.swift:sendAdminRequest:continuation_set\",\"message\":\"Continuation registered, calling conn.send\",\"data\":{\"requestId\":\"\(requestId)\",\"sessionId\":\"debug-session\",\"runId\":\"run1\",\"hypothesisId\":\"A\"}}\n".appendLine(toFile: logPath)
            // #endregion agent log

            conn.send(content: data, completion: .contentProcessed { [weak self] error in
                // #region agent log
                if let error = error {
                    try? "{\"timestamp\":\(Int(Date().timeIntervalSince1970 * 1000)),\"location\":\"MCPClient.swift:sendAdminRequest:send_error\",\"message\":\"conn.send failed\",\"data\":{\"requestId\":\"\(requestId)\",\"error\":\"\(error.localizedDescription)\",\"sessionId\":\"debug-session\",\"runId\":\"run1\",\"hypothesisId\":\"A\"}}\n".appendLine(toFile: logPath)
                } else {
                    try? "{\"timestamp\":\(Int(Date().timeIntervalSince1970 * 1000)),\"location\":\"MCPClient.swift:sendAdminRequest:send_success\",\"message\":\"conn.send succeeded\",\"data\":{\"requestId\":\"\(requestId)\",\"sessionId\":\"debug-session\",\"runId\":\"run1\",\"hypothesisId\":\"A\"}}\n".appendLine(toFile: logPath)
                }
                // #endregion agent log
                
                if let error = error {
                    if let self = self {
                        self.adminQueue.sync {
                            _ = self.adminPendingRequests.removeValue(forKey: requestId)
                        }
                    }
                    continuation.resume(throwing: MCPError.sendFailed(error.localizedDescription))
                }
            })
        }
    }

    // MARK: - Receive Handling

    private func startReceiving() {
        guard let conn = connection else { return }

        conn.receive(minimumIncompleteLength: 1, maximumLength: 65536) { [weak self] data, _, isComplete, error in
            if let data = data, !data.isEmpty {
                self?.receiveBuffer.append(data)
                self?.processBuffer()
            }

            if let error = error {
                print("MCPClient: Receive error - \(error)")
                return
            }

            if isComplete {
                print("MCPClient: Connection closed by server")
                self?.disconnect()
                return
            }

            // Continue receiving
            self?.startReceiving()
        }
    }

    private func processBuffer() {
        // Protocol uses newline-delimited JSON
        while let newlineIndex = receiveBuffer.firstIndex(of: UInt8(ascii: "\n")) {
            let lineData = receiveBuffer.prefix(upTo: newlineIndex)
            receiveBuffer = receiveBuffer.suffix(from: receiveBuffer.index(after: newlineIndex))

            guard !lineData.isEmpty else { continue }

            do {
                let response = try JSONDecoder().decode(MCPResponse.self, from: lineData)
                handleResponse(response)
            } catch {
                print("MCPClient: Failed to decode response - \(error)")
            }
        }
    }

    private func handleResponse(_ response: MCPResponse) {
        guard let requestId = response.id else {
            print("MCPClient: Received response without ID (notification)")
            return
        }

        // Already on queue (called from startReceiving), no need to sync
        let continuation = pendingRequests.removeValue(forKey: requestId)

        if let continuation = continuation {
            continuation.resume(returning: response)
        } else {
            print("MCPClient: No pending request for ID \(requestId)")
        }
    }

    // MARK: - Admin Receive Handling

    private func startAdminReceiving() {
        guard let conn = adminConnection else { return }

        conn.receive(minimumIncompleteLength: 1, maximumLength: 65536) { [weak self] data, _, isComplete, error in
            if let data = data, !data.isEmpty {
                self?.adminReceiveBuffer.append(data)
                self?.processAdminBuffer()
            }

            if let error = error {
                print("MCPClient: Admin receive error - \(error)")
                return
            }

            if isComplete {
                print("MCPClient: Admin connection closed by server")
                self?.disconnectAdmin()
                return
            }

            self?.startAdminReceiving()
        }
    }

    private func processAdminBuffer() {
        while let newlineIndex = adminReceiveBuffer.firstIndex(of: UInt8(ascii: "\n")) {
            let lineData = adminReceiveBuffer.prefix(upTo: newlineIndex)
            adminReceiveBuffer = adminReceiveBuffer.suffix(from: adminReceiveBuffer.index(after: newlineIndex))

            guard !lineData.isEmpty else { continue }

            do {
                // #region agent log - Hypothesis C,D,E: Track decoding attempt
                let logPath = "/Users/wp/Antigravity/.cursor/debug.log"
                let rawString = String(data: lineData, encoding: .utf8) ?? "invalid utf8"
                if let data = try? JSONSerialization.data(withJSONObject: ["location":"MCPClient.swift:processAdminBuffer","message":"Attempting to decode response","data":["rawLength":lineData.count,"rawPreview":String(rawString.prefix(200))],"hypothesisId":"C,D,E","timestamp":Date().timeIntervalSince1970*1000,"sessionId":"debug-session","runId":"run1"]), let json = String(data: data, encoding: .utf8) {
                    try? json.appendLine(toFile: logPath)
                }
                // #endregion
                
                let response = try JSONDecoder().decode(MCPResponse.self, from: lineData)
                
                // #region agent log - Hypothesis C,D,E: Track successful decode
                if let data = try? JSONSerialization.data(withJSONObject: ["location":"MCPClient.swift:processAdminBuffer","message":"Successfully decoded response","data":["id":response.id ?? "nil","ok":response.ok,"hasResult":response.result != nil,"error":response.error ?? "none"],"hypothesisId":"C,D,E","timestamp":Date().timeIntervalSince1970*1000,"sessionId":"debug-session","runId":"run1"]), let json = String(data: data, encoding: .utf8) {
                    try? json.appendLine(toFile: logPath)
                }
                // #endregion
                
                handleAdminResponse(response)
            } catch {
                let errorMsg = "Failed to decode admin response: \(error.localizedDescription)"
                print("MCPClient: \(errorMsg)")
                if let jsonString = String(data: lineData, encoding: .utf8) {
                    print("MCPClient: Raw response: \(jsonString)")
                }
                
                // #region agent log - Hypothesis C,D,E: Track decode failure
                let logPath = "/Users/wp/Antigravity/.cursor/debug.log"
                let rawString = String(data: lineData, encoding: .utf8) ?? "invalid utf8"
                if let data = try? JSONSerialization.data(withJSONObject: ["location":"MCPClient.swift:processAdminBuffer","message":"Decode failed","data":["error":error.localizedDescription,"errorType":"\(type(of: error))","rawLength":lineData.count,"rawPreview":String(rawString.prefix(500))],"hypothesisId":"C,D,E","timestamp":Date().timeIntervalSince1970*1000,"sessionId":"debug-session","runId":"run1"]), let json = String(data: data, encoding: .utf8) {
                    try? json.appendLine(toFile: logPath)
                }
                // #endregion
                
                // Try to find pending request and fail it with a proper error
                // Extract ID from raw JSON if possible
                if let json = try? JSONSerialization.jsonObject(with: lineData) as? [String: Any],
                   let id = json["id"] as? String {
                    adminQueue.async { [weak self] in
                        if let continuation = self?.adminPendingRequests.removeValue(forKey: id) {
                            let errorResponse = MCPResponse(id: id, ok: false, result: nil, error: errorMsg)
                            continuation.resume(returning: errorResponse)
                        }
                    }
                }
            }
        }
    }

    private func handleAdminResponse(_ response: MCPResponse) {
        guard let requestId = response.id else {
            print("MCPClient: Received admin response without ID (notification)")
            return
        }

        // Already on adminQueue (called from startAdminReceiving), no need to sync
        let continuation = adminPendingRequests.removeValue(forKey: requestId)

        if let continuation = continuation {
            continuation.resume(returning: response)
        } else {
            print("MCPClient: No pending admin request for ID \(requestId)")
        }
    }
}

// MARK: - MCP Errors

enum MCPError: LocalizedError {
    case notConnected
    case sendFailed(String)
    case invalidResponse
    case serverError(String)
    case timeout

    var errorDescription: String? {
        switch self {
        case .notConnected:
            return "Not connected to MCP server"
        case .sendFailed(let reason):
            return "Failed to send request: \(reason)"
        case .invalidResponse:
            return "Invalid response from server"
        case .serverError(let message):
            return "Server error: \(message)"
        case .timeout:
            return "Request timed out. The operation may still be processing. Please check the console logs for details."
        }
    }
}

// MARK: - Convenience Extensions

extension MCPClient {
    /// Query the context engine
    func getContext(query: String, pack: String? = nil, limit: Int? = nil) async throws -> [String: Any] {
        var params: [String: Any] = ["query": query]
        if let pack = pack { params["pack"] = pack }
        if let limit = limit { params["limit"] = limit }

        let response = try await send(tool: "get_context", params: params)

        if !response.ok {
            throw MCPError.serverError(response.error ?? "Unknown error")
        }

        return response.result?.value as? [String: Any] ?? [:]
    }

    /// Read a specific resource by URI
    func readResource(uri: String) async throws -> [String: Any] {
        let response = try await send(tool: "read_resource", params: ["uri": uri])

        if !response.ok {
            throw MCPError.serverError(response.error ?? "Unknown error")
        }

        return response.result?.value as? [String: Any] ?? [:]
    }

    /// Trigger a sync for a specific provider (uses admin socket)
    /// Note: Full syncs can take several minutes, so we use a longer timeout
    func syncProvider(_ provider: String, sinceDays: Int? = nil, mode: String = "full") async throws -> MCPResponse {
        var params: [String: Any] = ["provider": provider, "mode": mode]
        if let days = sinceDays {
            params["since_days"] = days
        }
        // Calculate timeout based on provider and sync type
        let timeout: TimeInterval
        if mode == "full" && sinceDays == nil {
            // Full syncs get 5 minutes
            timeout = 300
        } else if provider == "google" || provider == "google_workspace" || provider == "google_drive" {
            // Google Workspace syncs Drive + Calendar + Gmail sequentially, need more time even for quick syncs
            timeout = 180 // 3 minutes for quick Google Workspace syncs
        } else {
            // Other quick syncs get 30 seconds
            timeout = 30
        }
        return try await sendAdmin(tool: "sync_provider", params: params, timeout: timeout)
    }

    /// Get sync status
    func getSyncStatus() async throws -> [String: Any] {
        let response = try await send(tool: "get_sync_status")

        if !response.ok {
            throw MCPError.serverError(response.error ?? "Unknown error")
        }

        return response.result?.value as? [String: Any] ?? [:]
    }

    /// Ping the server to check connectivity
    func ping() async throws -> Bool {
        let response = try await send(tool: "ping")
        return response.ok
    }

    /// Verify credentials for all providers (uses admin socket)
    /// Returns a dictionary with provider name as key and status info as value
    func verifyCredentials() async throws -> [String: [String: Any]] {
        let response = try await sendAdmin(tool: "verify_credentials", params: [:])

        if !response.ok {
            throw MCPError.serverError(response.error ?? "Unknown error")
        }

        // Parse the result into a typed dictionary
        guard let result = response.result?.value as? [String: Any] else {
            return [:]
        }

        var credentials: [String: [String: Any]] = [:]
        for (key, value) in result {
            if let providerStatus = value as? [String: Any] {
                credentials[key] = providerStatus
            }
        }
        return credentials
    }

    /// Get engine status (uses admin socket, works before Core is ready)
    func getStatus() async throws -> [String: Any] {
        let response = try await sendAdmin(tool: "get_status", params: [:])

        if !response.ok {
            throw MCPError.serverError(response.error ?? "Unknown error")
        }

        return response.result?.value as? [String: Any] ?? [:]
    }
}
