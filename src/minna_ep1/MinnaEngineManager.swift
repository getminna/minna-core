import Foundation
import SwiftUI
import Combine
import AppKit

// Helper extension for file appending
extension String {
    func appendLine(toFile path: String) throws {
        if let fileHandle = FileHandle(forWritingAtPath: path) {
            defer { fileHandle.closeFile() }
            fileHandle.seekToEndOfFile()
            fileHandle.write((self + "\n").data(using: .utf8)!)
        } else {
            try (self + "\n").write(toFile: path, atomically: true, encoding: .utf8)
        }
    }
}

// Helper extension for SyncStatus description
extension SyncStatus {
    var description: String {
        switch self {
        case .idle: return "idle"
        case .active: return "active"
        case .syncing(let progress): return "syncing(\(progress ?? "nil"))"
        case .error(let msg): return "error(\(msg))"
        }
    }
}

// MARK: - Provider Abstraction

/// Supported data providers for the Minna context engine.
enum Provider: String, CaseIterable, Identifiable {
    case slack
    case googleWorkspace
    case github
    case cursor

    var id: String { rawValue }

    var displayName: String {
        switch self {
        case .slack: return "Slack"
        case .googleWorkspace: return "Google Workspace"
        case .github: return "GitHub"
        case .cursor: return "Cursor AI"
        }
    }

    var description: String {
        switch self {
        case .slack: return "Messages and channels"
        case .googleWorkspace: return "Calendar and email"
        case .github: return "Issues and comments"
        case .cursor: return "AI session history"
        }
    }

    /// Keychain account name for this provider's token
    var keychainAccount: String {
        return "\(rawValue)_token"
    }

    /// Icon name for UI display
    var iconName: String {
        switch self {
        case .slack: return "message.fill"
        case .googleWorkspace: return "calendar"
        case .github: return "chevron.left.forwardslash.chevron.right"
        case .cursor: return "brain.head.profile"
        }
    }

    /// Provider identifier for Rust backend
    var rustIdentifier: String {
        switch self {
        case .slack: return "slack"
        case .googleWorkspace: return "google"
        case .github: return "github"
        case .cursor: return "cursor"
        }
    }

    /// Whether this provider requires authentication/credentials
    var requiresAuth: Bool {
        switch self {
        case .slack, .googleWorkspace, .github: return true
        case .cursor: return false  // Local files only
        }
    }
}

// MARK: - Sync Status

/// Granular sync status for each provider.
enum SyncStatus: Equatable {
    case idle
    case syncing(progress: String?)
    case error(String)
    case active
    
    var isSyncing: Bool {
        if case .syncing = self { return true }
        return false
    }
    
    var isActive: Bool {
        if case .active = self { return true }
        return false
    }
    
    var displayText: String {
        switch self {
        case .idle: return "Not connected"
        case .syncing(let progress): return progress ?? "Syncing..."
        case .error(let msg): return "Error: \(msg)"
        case .active: return "Active"
        }
    }
}

// MARK: - Sync Event (for history)

struct SyncEvent: Identifiable {
    let id = UUID()
    let timestamp: Date
    let type: EventType
    let message: String
    
    enum EventType {
        case initialSync
        case deltaSync
        case error
        case connected
    }
    
    var timeString: String {
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm"
        return formatter.string(from: timestamp)
    }
    
    var dateString: String {
        let formatter = DateFormatter()
        formatter.dateFormat = "MMM d"
        return formatter.string(from: timestamp)
    }
    
    var relativeTimeString: String {
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .short
        return formatter.localizedString(for: timestamp, relativeTo: Date())
    }
}

// MARK: - Sync Progress Data

struct SyncProgressData {
    var documentsProcessed: Int = 0
    var totalDocuments: Int?
    var currentAction: String = "Starting..."

    func toSyncProgress() -> SyncProgress {
        SyncProgress(
            documentsProcessed: documentsProcessed,
            totalDocuments: totalDocuments,
            currentAction: currentAction
        )
    }
}

// MARK: - Stdio IPC Types (from Rust daemon)

/// Progress update from Rust daemon via stdout
struct MinnaProgress: Codable {
    let provider: String?
    let status: String
    let message: String
    let documents_processed: Int?
}

/// Final result from Rust daemon via stdout
struct MinnaResult: Codable {
    let type: String
    let status: String
    let data: [String: AnyCodableValue]?
}

/// Helper for decoding arbitrary JSON values
struct AnyCodableValue: Codable {
    let value: Any

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
        } else if let array = try? container.decode([AnyCodableValue].self) {
            value = array.map { $0.value }
        } else if let dict = try? container.decode([String: AnyCodableValue].self) {
            value = dict.mapValues { $0.value }
        } else {
            value = NSNull()
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
        default:
            try container.encodeNil()
        }
    }
}

// MARK: - Engine State

enum EngineState: Equatable {
    case welcome
    case syncing
    case postSync
    case running
}

// MARK: - MinnaEngineManager

class MinnaEngineManager: ObservableObject {
    static let shared = MinnaEngineManager()
    
    // MARK: - Published State
    
    @Published var providerStates: [Provider: SyncStatus] = [
        .slack: .idle,
        .googleWorkspace: .idle,
        .github: .idle
    ]
    
    /// Sync history per provider
    @Published var syncHistory: [Provider: [SyncEvent]] = [
        .slack: [],
        .googleWorkspace: [],
        .github: []
    ]
    
    /// Active sync progress per provider
    @Published var syncProgressData: [Provider: SyncProgressData] = [:]
    
    @Published var outputLog: String = ""

    /// Engine initialization status
    @Published var isEngineReady: Bool = false

    // MARK: - Computed Properties
    
    var state: EngineState {
        if providerStates.values.contains(where: { $0.isSyncing }) {
            return .syncing
        }
        if providerStates.values.contains(where: { $0.isActive }) {
            return .running
        }
        return .welcome
    }
    
    var isAnySyncing: Bool {
        providerStates.values.contains { $0.isSyncing }
    }
    
    var activeProviders: [Provider] {
        Provider.allCases.filter { providerStates[$0]?.isActive == true }
    }
    
    /// Check if a provider has credentials stored (using CredentialManager)
    func isProviderConnected(_ provider: Provider) -> Bool {
        return CredentialManager.shared.isReady(for: provider)
    }
    
    // MARK: - Sovereign Mode Flow
    
    /// Note: In Sovereign Mode, we don't use a bridge.
    /// - Slack: User creates their own app via manifest, provides tokens
    /// - GitHub: User creates a PAT
    /// - Google: User provides their own Client ID/Secret for local OAuth
    ///
    /// This method is called when the user has already configured credentials
    /// and we just need to start an OAuth flow (Google only).
    func startLocalOAuth(for provider: Provider) {
        guard provider == .googleWorkspace else {
            print("❌ startLocalOAuth only supported for Google Workspace")
            return
        }
        
        print("🔐 Starting local OAuth for \(provider.displayName)")

        DispatchQueue.main.async { [weak self, provider] in
            self?.providerStates[provider] = .syncing(progress: "Opening browser for auth...")
        }

        LocalOAuthManager.shared.startGoogleOAuth { [weak self, provider] result in
            DispatchQueue.main.async { [weak self, provider] in
                switch result {
                case .success:
                    self?.providerStates[provider] = .syncing(progress: "Starting sync...")
                    self?.addSyncEvent(for: provider, type: .connected, message: "Connected successfully")
                    self?.triggerSync(for: provider)

                case .failure(let error):
                    self?.providerStates[provider] = .error(error.localizedDescription)
                    self?.addSyncEvent(for: provider, type: .error, message: error.localizedDescription)
                }
            }
        }
    }
    
    /// Smart connect/sync handler
    /// Note: In Sovereign Mode, UI calls the config sheet first if not configured.
    /// This is only called after credentials are set up.
    func connectOrSync(for provider: Provider) {
        if isProviderConnected(provider) {
            triggerSync(for: provider)
        } else {
            // Provider needs configuration - this shouldn't normally be reached
            // because the UI shows config sheets first
            print("⚠️ connectOrSync called but \(provider.displayName) not configured")
        }
    }
    
    // MARK: - Private Properties

    private var daemonProcess: Process?
    @Published private(set) var isDaemonRunning = false

    // MARK: - Rust Daemon Management

    /// Path to the minna-core binary
    private var daemonPath: String {
        // Option 1: Environment variable (for development)
        if let envPath = ProcessInfo.processInfo.environment["MINNA_CORE_PATH"] {
            return envPath
        }

        // Option 2: Bundled in app resources
        if let bundledPath = Bundle.main.path(forResource: "minna-core", ofType: nil) {
            return bundledPath
        }

        // Option 3: Development path (mono-repo location)
        let devPath = NSHomeDirectory() + "/Antigravity/minna/engine/target/release/minna-core"
        if FileManager.default.fileExists(atPath: devPath) {
            return devPath
        }

        // Fallback
        return "/usr/local/bin/minna-core"
    }

    /// Check if daemon is already running
    func ensureDaemonRunning() {
        // Check if our managed process is running
        if let process = daemonProcess, process.isRunning {
            print("✅ Daemon already running (managed process)")
            isDaemonRunning = true
            return
        }

        // Don't use waitUntilExit() here - it pumps the run loop and can cause
        // recursive dispatch_once crashes if applicationWillTerminate fires.
        // Just launch the daemon directly - it handles duplicate instances gracefully.
        launchDaemon()
    }

    /// Launch the Rust daemon
    func launchDaemon() {
        guard FileManager.default.fileExists(atPath: daemonPath) else {
            print("❌ Daemon binary not found at: \(daemonPath)")
            return
        }

        print("🚀 Launching minna-core daemon from: \(daemonPath)")

        let process = Process()
        process.executableURL = URL(fileURLWithPath: daemonPath)

        // Set environment variables
        var env = ProcessInfo.processInfo.environment
        env["RUST_LOG"] = "info"

        // For dev: use existing model cache to avoid re-downloading 523MB on each build
        // For prod: will point to bundled model in app bundle
        if let modelsPath = Bundle.main.resourcePath?.appending("/models"),
           FileManager.default.fileExists(atPath: modelsPath) {
            print("🔍 DEBUG: Setting FASTEMBED_CACHE_DIR to bundled path: \(modelsPath)")
            env["FASTEMBED_CACHE_DIR"] = modelsPath
        } else {
            // Fallback to project cache for development
            let cachePath = "/Users/wp/Antigravity/minna/engine/.fastembed_cache"
            print("🔍 DEBUG: Setting FASTEMBED_CACHE_DIR to dev cache: \(cachePath)")
            env["FASTEMBED_CACHE_DIR"] = cachePath
        }

        print("🔍 DEBUG: Environment variables: \(env)")
        process.environment = env

        // Capture output for debugging
        let outputPipe = Pipe()
        let errorPipe = Pipe()
        process.standardOutput = outputPipe
        process.standardError = errorPipe

        // Parse stdout for structured MINNA_PROGRESS/MINNA_RESULT messages
        outputPipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            if !data.isEmpty, let output = String(data: data, encoding: .utf8) {
                for line in output.components(separatedBy: .newlines) {
                    let trimmed = line.trimmingCharacters(in: .whitespaces)
                    if trimmed.isEmpty { continue }

                    if trimmed.hasPrefix("MINNA_PROGRESS:") {
                        let json = String(trimmed.dropFirst("MINNA_PROGRESS:".count))
                        self?.handleProgressUpdate(json)
                    } else if trimmed.hasPrefix("MINNA_RESULT:") {
                        let json = String(trimmed.dropFirst("MINNA_RESULT:".count))
                        self?.handleResultUpdate(json)
                    } else {
                        // Non-structured output - just log it
                        print("[minna-core stdout] \(trimmed)")
                    }
                }
            }
        }

        // Stderr is for tracing logs - just print them
        errorPipe.fileHandleForReading.readabilityHandler = { handle in
            let data = handle.availableData
            if !data.isEmpty, let output = String(data: data, encoding: .utf8) {
                print("[minna-core] \(output.trimmingCharacters(in: .whitespacesAndNewlines))")
            }
        }

        process.terminationHandler = { [weak self] proc in
            let status = proc.terminationStatus
            DispatchQueue.main.async { [weak self] in
                self?.isDaemonRunning = false
                self?.daemonProcess = nil
                print("⚠️ Daemon terminated with status: \(status)")

                // Auto-restart after a delay if unexpected termination
                if status != 0 {
                    DispatchQueue.main.asyncAfter(deadline: .now() + 2.0) { [weak self] in
                        self?.launchDaemon()
                    }
                }
            }
        }

        do {
            try process.run()
            daemonProcess = process

            // Give daemon time to start, then connect MCP client
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) { [weak self] in
                self?.isDaemonRunning = true
                MCPClient.shared.connect()
                MCPClient.shared.connectAdmin()
                print("✅ Daemon launched successfully")

                // Verify credentials after admin connection is established
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
                    self?.verifyProviderCredentials()
                }
            }
        } catch {
            print("❌ Failed to launch daemon: \(error)")
        }
    }

    /// Stop the daemon
    func stopDaemon() {
        daemonProcess?.terminate()
        daemonProcess = nil
        MCPClient.shared.disconnect()
        MCPClient.shared.disconnectAdmin()
        isDaemonRunning = false
    }
    
    // MARK: - Initialization

    init() {
        // Check which providers are already connected (have tokens)
        checkProviderConnections()

        // Launch the Rust daemon
        ensureDaemonRunning()
    }
    
    private func checkProviderConnections() {
        // #region agent log - Hypothesis A: Track initial provider state
        let logPath = "/Users/wp/Antigravity/.cursor/debug.log"
        if let data = try? JSONSerialization.data(withJSONObject: ["location":"MinnaEngineManager.swift:checkProviderConnections","message":"Initializing provider states","data":["providers":Provider.allCases.map{$0.displayName}],"hypothesisId":"A","timestamp":Date().timeIntervalSince1970*1000,"sessionId":"debug-session","runId":"run1"]), let json = String(data: data, encoding: .utf8) {
            try? json.appendLine(toFile: logPath)
        }
        // #endregion
        
        for provider in Provider.allCases {
            // Local providers (no auth required) are always "active"
            if !provider.requiresAuth {
                providerStates[provider] = .active
                addSyncEvent(for: provider, type: .connected, message: "Connected")
                print("✅ \(provider.displayName): Local provider ready")
                continue
            }

            // Cloud providers: check credential status but don't mark as "active" yet
            // They become active only after a successful sync
            let status = CredentialManager.shared.status(for: provider)

            // #region agent log - Hypothesis A: Log credential status check
            if let data = try? JSONSerialization.data(withJSONObject: ["location":"MinnaEngineManager.swift:checkProviderConnections","message":"Checking credential status","data":["provider":provider.displayName,"status":"\(status)","willSetState":"idle"],"hypothesisId":"A","timestamp":Date().timeIntervalSince1970*1000,"sessionId":"debug-session","runId":"run1"]), let json = String(data: data, encoding: .utf8) {
                try? json.appendLine(toFile: logPath)
            }
            // #endregion

            switch status {
            case .configured:
                // Credentials exist, but don't mark as connected until sync verifies them
                providerStates[provider] = .idle
                print("🔑 \(provider.displayName): Credentials found, needs sync to verify")
            case .expired:
                providerStates[provider] = .idle
                print("⚠️ \(provider.displayName): Credentials expired, needs re-auth")
            case .error(let msg):
                providerStates[provider] = .error(msg)
                print("❌ \(provider.displayName): Error - \(msg)")
            case .notConfigured:
                providerStates[provider] = .idle
                print("⚪ \(provider.displayName): Not configured")
            }
        }
    }

    /// Verify provider credentials via Rust daemon (fast path, works before Core is ready)
    func verifyProviderCredentials() {
        guard MCPClient.shared.isAdminConnected else {
            print("⏳ verifyProviderCredentials: Admin not connected, retrying...")
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) { [weak self] in
                self?.verifyProviderCredentials()
            }
            return
        }

        Task { [weak self] in
            do {
                let credentials = try await MCPClient.shared.verifyCredentials()
                print("🔑 Credentials verified from Rust daemon")

                await MainActor.run { [weak self, credentials] in
                    for (providerName, status) in credentials {
                        guard let provider = self?.providerFromRustIdentifier(providerName) else {
                            continue
                        }

                        let configured = status["configured"] as? Bool ?? false
                        let statusStr = status["status"] as? String ?? "unknown"

                        if configured && statusStr == "ready" {
                            // Credentials are valid - mark as active (connected but not synced yet)
                            // This shows "Connected" in UI instead of "Not connected"
                            self?.providerStates[provider] = .active
                            print("✅ \(provider.displayName): Credentials verified")
                        } else if configured && statusStr == "expired" {
                            // Credentials expired
                            self?.providerStates[provider] = .idle
                            print("⚠️ \(provider.displayName): Credentials expired")
                        } else {
                            // Not configured
                            self?.providerStates[provider] = .idle
                            print("⚪ \(provider.displayName): Not configured")
                        }
                    }
                }
            } catch {
                print("❌ verifyProviderCredentials failed: \(error)")
            }
        }
    }

    // MARK: - Add Sync Event
    
    func addSyncEvent(for provider: Provider, type: SyncEvent.EventType, message: String) {
        let event = SyncEvent(timestamp: Date(), type: type, message: message)
        if syncHistory[provider] == nil {
            syncHistory[provider] = []
        }
        syncHistory[provider]?.insert(event, at: 0)
        
        // Keep only last 20 events
        if let count = syncHistory[provider]?.count, count > 20 {
            syncHistory[provider] = Array(syncHistory[provider]!.prefix(20))
        }
    }
    
    // MARK: - Deep Link Handling (Sovereign Mode)
    
    /// In Sovereign Mode, we use the minna:// URL scheme only for local OAuth callbacks (Google).
    /// Slack and GitHub don't need deep links since they use static tokens.
    func handleDeepLink(url: URL) {
        print("🔗 Deep link received: \(url.absoluteString)")
        // Currently, deep links are only used if we had the bridge.
        // In pure Sovereign Mode, Google uses loopback (127.0.0.1:port) not a URL scheme.
        // This method is kept for potential future use cases.
        
        guard let components = URLComponents(url: url, resolvingAgainstBaseURL: true),
              let host = components.host else {
            print("⚠️ Ignoring deep link - Sovereign Mode uses loopback for OAuth")
            return
        }
        
        // Handle legacy or future deep link patterns
        if host == "callback" || host == "oauth" {
            print("⚠️ OAuth deep links are not used in Sovereign Mode")
            print("   Google uses http://127.0.0.1:8847/callback instead")
        }
    }
    
    // MARK: - Sync Engine (IPC to Rust Daemon)

    /// Active sync tasks for cancellation
    private var activeSyncTasks: [Provider: Task<Void, Never>] = [:]

    func triggerSync(for provider: Provider) {
        // Default to full sync when no parameters specified
        triggerSync(for: provider, sinceDays: nil, mode: "full")
    }

    private func onSyncComplete(for provider: Provider, success: Bool, error: String? = nil, documentsCount: Int? = nil, wasQuickSync: Bool = false) {
        activeSyncTasks[provider] = nil
        syncProgressData[provider] = nil
        
        if success {
            providerStates[provider] = .active
            
            // Determine if this was initial or delta sync
            let isInitial = (syncHistory[provider]?.isEmpty ?? true) ||
                           (syncHistory[provider]?.allSatisfy { $0.type != .initialSync } ?? true)
            
            let countStr = documentsCount.map { " (\($0) docs)" } ?? ""
            
            if wasQuickSync {
                addSyncEvent(for: provider, type: .initialSync, message: "Quick sync completed\(countStr)")
                // Mark that we need to backfill
                pendingBackfill[provider] = true
                // Schedule backfill after a short delay
                DispatchQueue.main.asyncAfter(deadline: .now() + 2.0) { [weak self, provider] in
                    self?.startBackfillSync(for: provider)
                }
            } else {
                addSyncEvent(for: provider, type: isInitial ? .initialSync : .deltaSync,
                            message: isInitial ? "Initial sync completed\(countStr)" : "Delta sync completed\(countStr)")
            }
        } else {
            providerStates[provider] = .error(error ?? "Sync failed")
            addSyncEvent(for: provider, type: .error, message: error ?? "Sync failed")
        }
    }
    
    func cancelSync(for provider: Provider) {
        activeSyncTasks[provider]?.cancel()
        activeSyncTasks[provider] = nil

        DispatchQueue.main.async { [weak self, provider] in
            self?.providerStates[provider] = .idle
        }
    }

    func cancelAllSyncs() {
        for (_, task) in activeSyncTasks {
            task.cancel()
        }
        activeSyncTasks.removeAll()

        DispatchQueue.main.async { [weak self] in
            for provider in Provider.allCases {
                if self?.providerStates[provider]?.isSyncing == true {
                    self?.providerStates[provider] = .idle
                }
            }
        }
    }

    // MARK: - Stdio Progress Handling (from Rust daemon)

    /// Handle a MINNA_PROGRESS JSON message from Rust stdout
    private func handleProgressUpdate(_ json: String) {
        guard let data = json.data(using: .utf8) else {
            print("⚠️ handleProgressUpdate: Invalid UTF-8")
            return
        }

        do {
            let progress = try JSONDecoder().decode(MinnaProgress.self, from: data)

            // Map provider string to Provider enum
            guard let providerName = progress.provider,
                  let provider = providerFromRustIdentifier(providerName) else {
                // Could be an "init" status for engine startup
                if progress.provider == "init" || progress.status == "init" {
                    print("🔧 Engine: \(progress.message)")
                }
                return
            }

            DispatchQueue.main.async { [weak self, provider, progress] in
                switch progress.status {
                case "syncing", "indexing":
                    self?.providerStates[provider] = .syncing(progress: progress.message)
                    if let docs = progress.documents_processed {
                        self?.syncProgressData[provider] = SyncProgressData(
                            documentsProcessed: docs,
                            currentAction: progress.message
                        )
                    }
                case "error":
                    self?.providerStates[provider] = .error(progress.message)
                    self?.addSyncEvent(for: provider, type: .error, message: progress.message)
                case "cancelled":
                    self?.providerStates[provider] = .idle
                    self?.addSyncEvent(for: provider, type: .error, message: "Sync cancelled")
                default:
                    print("⚠️ Unknown progress status: \(progress.status)")
                }
            }
        } catch {
            print("⚠️ handleProgressUpdate decode error: \(error)")
        }
    }

    /// Handle a MINNA_RESULT JSON message from Rust stdout
    private func handleResultUpdate(_ json: String) {
        guard let data = json.data(using: .utf8) else {
            print("⚠️ handleResultUpdate: Invalid UTF-8")
            return
        }

        do {
            let result = try JSONDecoder().decode(MinnaResult.self, from: data)

            switch result.type {
            case "init":
                if result.status == "ready" {
                    print("✅ Engine ready")
                    DispatchQueue.main.async { [weak self] in
                        self?.isEngineReady = true
                    }
                }
            case "sync":
                // Extract provider from data if present
                if let providerName = result.data?["provider"]?.value as? String,
                   let provider = providerFromRustIdentifier(providerName) {
                    let docsCount = result.data?["documents_processed"]?.value as? Int
                    let status = result.status
                    DispatchQueue.main.async { [weak self, provider, docsCount, status] in
                        if status == "complete" {
                            self?.onSyncComplete(for: provider, success: true, documentsCount: docsCount)
                        } else if status == "cancelled" {
                            self?.providerStates[provider] = .idle
                            self?.addSyncEvent(for: provider, type: .error, message: "Sync cancelled (\(docsCount ?? 0) docs)")
                        }
                    }
                }
            case "auth":
                print("🔐 Auth result: \(result.status)")
            default:
                print("ℹ️ Result: \(result.type) - \(result.status)")
            }
        } catch {
            print("⚠️ handleResultUpdate decode error: \(error)")
        }
    }

    /// Convert Rust provider identifier to Swift Provider enum
    private func providerFromRustIdentifier(_ identifier: String) -> Provider? {
        switch identifier {
        case "slack": return .slack
        case "google", "google_drive": return .googleWorkspace
        case "github": return .github
        case "cursor": return .cursor
        default: return nil
        }
    }

    // MARK: - Discovery (IPC to Rust Daemon)

    /// Run discovery for a provider to get metadata before syncing (uses admin socket)
    func runDiscovery(for provider: Provider, completion: @escaping (Result<SlackDiscoveryResult, Error>) -> Void) {
        // #region agent log
        let logPath = "/Users/wp/Antigravity/.cursor/debug.log"
        try? "{\"timestamp\":\(Int(Date().timeIntervalSince1970 * 1000)),\"location\":\"MinnaEngineManager.swift:runDiscovery:entry\",\"message\":\"runDiscovery called\",\"data\":{\"provider\":\"\(provider.rawValue)\",\"rustIdentifier\":\"\(provider.rustIdentifier)\",\"sessionId\":\"debug-session\",\"runId\":\"run1\",\"hypothesisId\":\"A\"}}\n".appendLine(toFile: logPath)
        // #endregion agent log
        
        Task { [provider, completion] in
            do {
                // Refresh Google token if needed before discovery
                if provider == .googleWorkspace {
                    // #region agent log
                    try? "{\"timestamp\":\(Int(Date().timeIntervalSince1970 * 1000)),\"location\":\"MinnaEngineManager.swift:runDiscovery:refresh_token\",\"message\":\"Refreshing Google token before discovery\",\"data\":{\"sessionId\":\"debug-session\",\"runId\":\"run1\",\"hypothesisId\":\"E\"}}\n".appendLine(toFile: logPath)
                    // #endregion agent log
                    
                    // Try to refresh the token (this will update it in keychain if refresh succeeds)
                    // We use a continuation to bridge the completion handler to async/await
                    _ = try? await withCheckedThrowingContinuation { (continuation: CheckedContinuation<String, Error>) in
                        LocalOAuthManager.shared.refreshGoogleToken { result in
                            switch result {
                            case .success(let token):
                                continuation.resume(returning: token)
                            case .failure(let error):
                                // Log but don't fail - the token might still be valid or refresh might not be needed
                                // #region agent log
                                try? "{\"timestamp\":\(Int(Date().timeIntervalSince1970 * 1000)),\"location\":\"MinnaEngineManager.swift:runDiscovery:refresh_failed\",\"message\":\"Token refresh failed (may still be valid)\",\"data\":{\"error\":\"\(error.localizedDescription)\",\"sessionId\":\"debug-session\",\"runId\":\"run1\",\"hypothesisId\":\"E\"}}\n".appendLine(toFile: logPath)
                                // #endregion agent log
                                // Continue anyway - the token might still be valid
                                continuation.resume(returning: "")
                            }
                        }
                    }
                }
                
                // #region agent log
                try? "{\"timestamp\":\(Int(Date().timeIntervalSince1970 * 1000)),\"location\":\"MinnaEngineManager.swift:runDiscovery:before_sendAdmin\",\"message\":\"About to call sendAdmin\",\"data\":{\"provider\":\"\(provider.rawValue)\",\"sessionId\":\"debug-session\",\"runId\":\"run1\",\"hypothesisId\":\"A\"}}\n".appendLine(toFile: logPath)
                // #endregion agent log
                
                // Use longer timeout for discovery (120 seconds for Google Workspace, 90 for GitHub, 60 for others)
                // Google Workspace discovery can take longer as it queries Drive API with pagination
                // GitHub discovery queries repos API with pagination (up to 10 pages)
                let timeout: TimeInterval = (provider == .googleWorkspace) ? 120 : (provider == .github) ? 90 : 60
                let response = try await MCPClient.shared.sendAdmin(tool: "discover", params: ["provider": provider.rustIdentifier], timeout: timeout)
                
                // #region agent log
                try? "{\"timestamp\":\(Int(Date().timeIntervalSince1970 * 1000)),\"location\":\"MinnaEngineManager.swift:runDiscovery:after_sendAdmin\",\"message\":\"sendAdmin returned\",\"data\":{\"ok\":\(response.ok),\"error\":\"\(response.error ?? "none")\",\"sessionId\":\"debug-session\",\"runId\":\"run1\",\"hypothesisId\":\"A\"}}\n".appendLine(toFile: logPath)
                // #endregion agent log

                if response.ok, let result = response.result?.value as? [String: Any] {
                    // Convert dictionary to JSON data for decoding
                    let jsonData = try JSONSerialization.data(withJSONObject: result)
                    let discoveryResult = try JSONDecoder().decode(SlackDiscoveryResult.self, from: jsonData)
                    completion(.success(discoveryResult))
                } else {
                    let error = NSError(domain: "MinnaDiscovery", code: -1,
                                       userInfo: [NSLocalizedDescriptionKey: response.error ?? "Discovery failed"])
                    completion(.failure(error))
                }
            } catch {
                completion(.failure(error))
            }
        }
    }

    /// Trigger sync with specific parameters (for quick vs full sync)
    func triggerSync(for provider: Provider, sinceDays: Int? = nil, mode: String = "full") {
        // #region agent log - Hypothesis B,C,D,E: Track sync trigger
        let logPath = "/Users/wp/Antigravity/.cursor/debug.log"
        let currentState = providerStates[provider]
        let hasHistory = !(syncHistory[provider]?.isEmpty ?? true)
        if let data = try? JSONSerialization.data(withJSONObject: ["location":"MinnaEngineManager.swift:triggerSync","message":"Sync triggered","data":["provider":provider.displayName,"sinceDays":sinceDays ?? -1,"mode":mode,"currentState":"\(currentState?.description ?? "nil")","hasHistory":hasHistory,"isFirstSync":syncHistory[provider]?.isEmpty ?? true],"hypothesisId":"B,C,D,E","timestamp":Date().timeIntervalSince1970*1000,"sessionId":"debug-session","runId":"run1"]), let json = String(data: data, encoding: .utf8) {
            try? json.appendLine(toFile: logPath)
        }
        // #endregion
        
        // Cancel any existing sync for this provider
        activeSyncTasks[provider]?.cancel()

        let modeLabel = sinceDays != nil ? "last \(sinceDays!) days" : "full history"
        DispatchQueue.main.async { [weak self, provider, modeLabel] in
            self?.providerStates[provider] = .syncing(progress: "Initializing (\(modeLabel))...")
        }

        // Ensure daemon is running and connected
        guard isDaemonRunning, MCPClient.shared.isAdminConnected else {
            ensureDaemonRunning()
            DispatchQueue.main.asyncAfter(deadline: .now() + 2.0) { [weak self, provider, sinceDays, mode] in
                self?.triggerSync(for: provider, sinceDays: sinceDays, mode: mode)
            }
            return
        }

        let wasQuickSync = sinceDays != nil

        // Create async task for the sync
        let task = Task { [weak self, provider, sinceDays, mode, wasQuickSync] in
            do {
                // #region agent log - Hypothesis C,D,E: Track sync request start
                if let data = try? JSONSerialization.data(withJSONObject: ["location":"MinnaEngineManager.swift:triggerSync","message":"Calling syncProvider","data":["provider":provider.displayName,"mode":mode],"hypothesisId":"C,D,E","timestamp":Date().timeIntervalSince1970*1000,"sessionId":"debug-session","runId":"run1"]), let json = String(data: data, encoding: .utf8) {
                    try? json.appendLine(toFile: logPath)
                }
                // #endregion
                
                let response = try await MCPClient.shared.syncProvider(provider.rustIdentifier, sinceDays: sinceDays, mode: mode)

                // #region agent log - Hypothesis C,D,E: Track sync response received
                if let data = try? JSONSerialization.data(withJSONObject: ["location":"MinnaEngineManager.swift:triggerSync","message":"Sync response received","data":["provider":provider.displayName,"ok":response.ok,"hasResult":response.result != nil,"error":response.error ?? "none","resultType":"\(type(of: response.result?.value))"],"hypothesisId":"C,D,E","timestamp":Date().timeIntervalSince1970*1000,"sessionId":"debug-session","runId":"run1"]), let json = String(data: data, encoding: .utf8) {
                    try? json.appendLine(toFile: logPath)
                }
                // #endregion

                await MainActor.run { [weak self, provider, wasQuickSync] in
                    if response.ok {
                        let result = response.result?.value as? [String: Any]
                        let docsCount = result?["documents_processed"] as? Int
                        // #region agent log - Hypothesis C,D,E: Track successful response parsing
                        if let data = try? JSONSerialization.data(withJSONObject: ["location":"MinnaEngineManager.swift:triggerSync","message":"Parsing successful response","data":["provider":provider.displayName,"docsCount":docsCount ?? -1,"resultKeys":result?.keys.map{$0} ?? []],"hypothesisId":"C,D,E","timestamp":Date().timeIntervalSince1970*1000,"sessionId":"debug-session","runId":"run1"]), let json = String(data: data, encoding: .utf8) {
                            try? json.appendLine(toFile: logPath)
                        }
                        // #endregion
                        self?.onSyncComplete(for: provider, success: true, documentsCount: docsCount, wasQuickSync: wasQuickSync)
                    } else {
                        // #region agent log - Hypothesis C,D,E: Track error response
                        if let data = try? JSONSerialization.data(withJSONObject: ["location":"MinnaEngineManager.swift:triggerSync","message":"Sync failed","data":["provider":provider.displayName,"error":response.error ?? "Unknown error"],"hypothesisId":"C,D,E","timestamp":Date().timeIntervalSince1970*1000,"sessionId":"debug-session","runId":"run1"]), let json = String(data: data, encoding: .utf8) {
                            try? json.appendLine(toFile: logPath)
                        }
                        // #endregion
                        self?.onSyncComplete(for: provider, success: false, error: response.error ?? "Unknown error")
                    }
                }
            } catch {
                // #region agent log - Hypothesis C,D,E: Track exception
                if let data = try? JSONSerialization.data(withJSONObject: ["location":"MinnaEngineManager.swift:triggerSync","message":"Sync exception","data":["provider":provider.displayName,"error":error.localizedDescription,"errorType":"\(type(of: error))"],"hypothesisId":"C,D,E","timestamp":Date().timeIntervalSince1970*1000,"sessionId":"debug-session","runId":"run1"]), let json = String(data: data, encoding: .utf8) {
                    try? json.appendLine(toFile: logPath)
                }
                // #endregion
                await MainActor.run { [weak self, provider] in
                    self?.onSyncComplete(for: provider, success: false, error: error.localizedDescription)
                }
            }
        }

        activeSyncTasks[provider] = task
    }
    
    /// Tracks pending backfill state per provider
    @Published var pendingBackfill: [Provider: Bool] = [:]
    
    /// Start background backfill after quick sync
    func startBackfillSync(for provider: Provider) {
        guard pendingBackfill[provider] == true else { return }
        
        // Use backfill mode which continues where quick sync left off
        triggerSync(for: provider, sinceDays: nil, mode: "backfill")
        pendingBackfill[provider] = false
    }
    
    // Legacy methods...
    func checkSetup(completion: @escaping (MinnaSetupStatus?) -> Void) {
        completion(nil)
    }
    
    func startEngine(slackToken: String) {}
    func stopEngine() {}
}

// MARK: - Setup Status

struct MinnaSetupStatus: Codable {
    let slack_auth: Bool
    let db_initialized: Bool
    let records_count: Int?
}

