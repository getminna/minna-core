import Foundation
import SwiftUI
import Combine
import AppKit

// MARK: - Provider Abstraction

/// Supported data providers for the Minna context engine.
enum Provider: String, CaseIterable, Identifiable {
    case slack
    case googleWorkspace
    case github
    
    var id: String { rawValue }
    
    var displayName: String {
        switch self {
        case .slack: return "Slack"
        case .googleWorkspace: return "Google Workspace"
        case .github: return "GitHub"
        }
    }
    
    var description: String {
        switch self {
        case .slack: return "Messages and channels"
        case .googleWorkspace: return "Calendar and email"
        case .github: return "Issues and comments"
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
        }
    }
    
    /// CLI argument for Python backend
    var cliArgument: String {
        switch self {
        case .slack: return "slack"
        case .googleWorkspace: return "google"
        case .github: return "github"
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
        
        DispatchQueue.main.async {
            self.providerStates[provider] = .syncing(progress: "Opening browser for auth...")
        }
        
        LocalOAuthManager.shared.startGoogleOAuth { [weak self] result in
            DispatchQueue.main.async {
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
    
    private var activeProcesses: [Provider: Process] = [:]
    private var engineProcess: Process?
    
    // MARK: - Path Discovery
    
    /// Discovers the Python executable from Poetry environment
    /// Falls back to system python3 if Poetry not available
    private var pythonExecutable: String {
        // Try to find Poetry's virtual environment
        let poetryVenvPath = discoverPoetryPython()
        if let path = poetryVenvPath, FileManager.default.fileExists(atPath: path) {
            return path
        }
        
        // Fall back to system python3
        return "/usr/bin/python3"
    }
    
    /// Discovers the project root (where pyproject.toml lives)
    private var projectRoot: String {
        // Option 1: Environment variable (for development)
        if let envPath = ProcessInfo.processInfo.environment["MINNA_PROJECT_ROOT"] {
            return envPath
        }
        
        // Option 2: Bundle resource (for distributed app)
        if let bundlePath = Bundle.main.resourcePath {
            let candidatePath = (bundlePath as NSString).deletingLastPathComponent
            let pyprojectPath = (candidatePath as NSString).appendingPathComponent("pyproject.toml")
            if FileManager.default.fileExists(atPath: pyprojectPath) {
                return candidatePath
            }
        }
        
        // Option 3: Walk up from current directory looking for pyproject.toml
        var currentPath = FileManager.default.currentDirectoryPath
        for _ in 0..<10 {
            let pyprojectPath = (currentPath as NSString).appendingPathComponent("pyproject.toml")
            if FileManager.default.fileExists(atPath: pyprojectPath) {
                return currentPath
            }
            currentPath = (currentPath as NSString).deletingLastPathComponent
        }
        
        // Last resort: assume we're in the right place
        return FileManager.default.currentDirectoryPath
    }
    
    /// Discovers Poetry's virtual environment Python path
    private func discoverPoetryPython() -> String? {
        // Run `poetry env info -p` to get the venv path
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = ["poetry", "env", "info", "-p"]
        process.currentDirectoryURL = URL(fileURLWithPath: projectRoot)
        
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = FileHandle.nullDevice
        
        do {
            try process.run()
            process.waitUntilExit()
            
            if process.terminationStatus == 0 {
                let data = pipe.fileHandleForReading.readDataToEndOfFile()
                if let venvPath = String(data: data, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines) {
                    return (venvPath as NSString).appendingPathComponent("bin/python")
                }
            }
        } catch {
            print("⚠️ Could not discover Poetry environment: \(error)")
        }
        
        return nil
    }
    
    private var enginePath: String? {
        guard let bundlePath = Bundle.main.path(forResource: "MinnaEngine", ofType: "bundle") else {
            return nil
        }
        return bundlePath + "/minna-engine"
    }
    
    // MARK: - Initialization
    
    init() {
        // Check which providers are already connected (have tokens)
        checkProviderConnections()
    }
    
    private func checkProviderConnections() {
        for provider in Provider.allCases {
            let status = CredentialManager.shared.status(for: provider)
            
            switch status {
            case .configured:
                providerStates[provider] = .active
                print("✅ \(provider.displayName): Configured and ready")
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
    
    // MARK: - Sync Engine
    
    func triggerSync(for provider: Provider) {
        if let existing = activeProcesses[provider], existing.isRunning {
            existing.terminate()
            activeProcesses[provider] = nil
        }
        
        DispatchQueue.main.async {
            self.providerStates[provider] = .syncing(progress: "Initializing...")
        }
        
        let process = Process()
        process.executableURL = URL(fileURLWithPath: pythonExecutable)
        process.arguments = ["-m", "minna.cli", "sync", "--provider", provider.cliArgument]
        
        var env = ProcessInfo.processInfo.environment
        env["PYTHONPATH"] = "\(projectRoot)/src"
        env["PYTHONUNBUFFERED"] = "1"
        process.environment = env
        
        let outputPipe = Pipe()
        let errorPipe = Pipe()
        process.standardOutput = outputPipe
        process.standardError = errorPipe
        
        outputPipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            guard !data.isEmpty, let output = String(data: data, encoding: .utf8) else { return }
            
            for line in output.components(separatedBy: .newlines) where !line.isEmpty {
                if line.hasPrefix("MINNA_PROGRESS:") {
                    let jsonString = String(line.dropFirst("MINNA_PROGRESS:".count))
                    if let jsonData = jsonString.data(using: .utf8),
                       let progress = try? JSONSerialization.jsonObject(with: jsonData) as? [String: Any] {
                        
                        let status = progress["status"] as? String ?? ""
                        let message = progress["message"] as? String ?? ""
                        let docsProcessed = progress["documents_processed"] as? Int
                        let totalDocs = progress["total_documents"] as? Int
                        
                        DispatchQueue.main.async {
                            if status == "complete" {
                                self?.syncProgressData[provider] = nil
                                self?.onSyncComplete(for: provider, success: true, documentsCount: docsProcessed)
                            } else if status == "error" {
                                self?.syncProgressData[provider] = nil
                                self?.onSyncComplete(for: provider, success: false, error: message)
                            } else {
                                // Update progress data
                                var progressData = self?.syncProgressData[provider] ?? SyncProgressData()
                                if let docs = docsProcessed {
                                    progressData.documentsProcessed = docs
                                }
                                if let total = totalDocs {
                                    progressData.totalDocuments = total
                                }
                                progressData.currentAction = message
                                self?.syncProgressData[provider] = progressData
                                
                                self?.providerStates[provider] = .syncing(progress: message)
                            }
                        }
                    }
                }
            }
        }
        
        process.terminationHandler = { [weak self] proc in
            outputPipe.fileHandleForReading.readabilityHandler = nil
            errorPipe.fileHandleForReading.readabilityHandler = nil
            
            DispatchQueue.main.async {
                self?.activeProcesses[provider] = nil
                
                if case .syncing = self?.providerStates[provider] {
                    if proc.terminationStatus == 0 {
                        self?.onSyncComplete(for: provider, success: true)
                    } else {
                        self?.onSyncComplete(for: provider, success: false, error: "Process exited with code \(proc.terminationStatus)")
                    }
                }
            }
        }
        
        activeProcesses[provider] = process
        
        do {
            try process.run()
        } catch {
            DispatchQueue.main.async { [weak self] in
                self?.providerStates[provider] = .error("Failed to start sync")
                self?.addSyncEvent(for: provider, type: .error, message: "Failed to start sync process")
            }
        }
    }
    
    private func onSyncComplete(for provider: Provider, success: Bool, error: String? = nil, documentsCount: Int? = nil) {
        activeProcesses[provider] = nil
        syncProgressData[provider] = nil
        
        if success {
            providerStates[provider] = .active
            
            // Determine if this was initial or delta sync
            let isInitial = (syncHistory[provider]?.isEmpty ?? true) ||
                           (syncHistory[provider]?.allSatisfy { $0.type != .initialSync } ?? true)
            
            let countStr = documentsCount.map { " (\($0) docs)" } ?? ""
            addSyncEvent(for: provider, type: isInitial ? .initialSync : .deltaSync,
                        message: isInitial ? "Initial sync completed\(countStr)" : "Delta sync completed\(countStr)")
        } else {
            providerStates[provider] = .error(error ?? "Sync failed")
            addSyncEvent(for: provider, type: .error, message: error ?? "Sync failed")
        }
    }
    
    func cancelSync(for provider: Provider) {
        if let process = activeProcesses[provider], process.isRunning {
            process.terminate()
        }
        activeProcesses[provider] = nil
        
        DispatchQueue.main.async { [weak self] in
            self?.providerStates[provider] = .idle
        }
    }
    
    func cancelAllSyncs() {
        for (provider, process) in activeProcesses {
            if process.isRunning {
                process.terminate()
            }
        }
        activeProcesses.removeAll()
        
        DispatchQueue.main.async { [weak self] in
            for provider in Provider.allCases {
                if self?.providerStates[provider]?.isSyncing == true {
                    self?.providerStates[provider] = .idle
                }
            }
        }
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
