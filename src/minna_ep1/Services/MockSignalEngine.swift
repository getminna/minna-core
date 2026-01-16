import Foundation
import SwiftUI
import Combine

#if DEBUG
/// Mock implementation of SignalProvider for instant UI iteration
/// Provides predefined states without requiring Rust backend connection
/// DEV-ONLY: Excluded from Release builds
class MockSignalEngine: SignalProvider {
    // MARK: - Published State
    
    @Published var providerStates: [Provider: SyncStatus] = [:]
    @Published var syncHistory: [Provider: [SyncEvent]] = [:]
    @Published var syncProgressData: [Provider: SyncProgressData] = [:]
    @Published var outputLog: String = ""
    @Published var isEngineReady: Bool = false
    
    // MARK: - Mock State Scenarios
    
    enum MockState {
        case sunny      // All providers active
        case indexing   // One provider syncing
        case welcome    // No providers connected
        case error      // Error state
    }
    
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
    
    // MARK: - Initialization
    
    init(state: MockState = .sunny) {
        configureState(state)
    }
    
    private func configureState(_ mockState: MockState) {
        let now = Date()
        
        switch mockState {
        case .sunny:
            // All providers active with history
            providerStates = [
                .slack: .active,
                .googleWorkspace: .active,
                .github: .active
            ]
            syncHistory = [
                .slack: [
                    SyncEvent(timestamp: now.addingTimeInterval(-3600), type: .deltaSync, message: "Synced 42 new messages"),
                    SyncEvent(timestamp: now.addingTimeInterval(-86400), type: .initialSync, message: "Initial sync completed (1,247 messages)")
                ],
                .googleWorkspace: [
                    SyncEvent(timestamp: now.addingTimeInterval(-7200), type: .deltaSync, message: "Synced 15 new emails"),
                    SyncEvent(timestamp: now.addingTimeInterval(-172800), type: .initialSync, message: "Initial sync completed (892 emails)")
                ],
                .github: [
                    SyncEvent(timestamp: now.addingTimeInterval(-1800), type: .deltaSync, message: "Synced 8 new comments"),
                    SyncEvent(timestamp: now.addingTimeInterval(-259200), type: .initialSync, message: "Initial sync completed (234 comments)")
                ]
            ]
            isEngineReady = true
            outputLog = "✅ All providers active and synced"
            
        case .indexing:
            // One provider syncing, others idle
            providerStates = [
                .slack: .syncing(progress: "Indexing messages..."),
                .googleWorkspace: .idle,
                .github: .idle
            ]
            syncProgressData = [
                .slack: SyncProgressData(
                    documentsProcessed: 1247,
                    totalDocuments: 3500,
                    currentAction: "Indexing #engineering channel"
                )
            ]
            syncHistory = [
                .slack: [
                    SyncEvent(timestamp: now.addingTimeInterval(-300), type: .initialSync, message: "Starting initial sync...")
                ]
            ]
            isEngineReady = true
            outputLog = "🔄 Syncing Slack..."
            
        case .welcome:
            // No providers connected
            providerStates = [
                .slack: .idle,
                .googleWorkspace: .idle,
                .github: .idle
            ]
            syncHistory = [:]
            syncProgressData = [:]
            isEngineReady = false
            outputLog = "👋 Welcome! Connect your first source to get started."
            
        case .error:
            // Error state
            providerStates = [
                .slack: .active,
                .googleWorkspace: .error("Authentication expired"),
                .github: .idle
            ]
            syncHistory = [
                .slack: [
                    SyncEvent(timestamp: now.addingTimeInterval(-3600), type: .deltaSync, message: "Synced 42 new messages")
                ],
                .googleWorkspace: [
                    SyncEvent(timestamp: now.addingTimeInterval(-7200), type: .error, message: "Authentication expired - please reconnect")
                ]
            ]
            isEngineReady = true
            outputLog = "⚠️ Google Workspace authentication expired"
        }
    }
    
    // MARK: - Methods (mock implementations)
    
    func isProviderConnected(_ provider: Provider) -> Bool {
        // In mock, consider providers "connected" if they're active or syncing
        switch providerStates[provider] {
        case .active, .syncing:
            return true
        default:
            return false
        }
    }
    
    func triggerSync(for provider: Provider, sinceDays: Int?, mode: String) {
        // Simulate sync with instant completion
        DispatchQueue.main.async { [weak self] in
            guard let self = self else { return }
            
            // Set syncing state
            self.providerStates[provider] = .syncing(progress: "Starting sync...")
            
            // Simulate progress after a short delay
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) {
                self.syncProgressData[provider] = SyncProgressData(
                    documentsProcessed: 0,
                    totalDocuments: 100,
                    currentAction: "Fetching data..."
                )
            }
            
            // Complete sync after another short delay
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) {
                self.providerStates[provider] = .active
                self.syncProgressData[provider] = nil
                
                let count = sinceDays != nil ? 42 : 1247
                let modeLabel = sinceDays != nil ? "Quick sync" : "Full sync"
                self.addSyncEvent(
                    for: provider,
                    type: .deltaSync,
                    message: "\(modeLabel) completed (\(count) docs)"
                )
            }
        }
    }
    
    func connectOrSync(for provider: Provider) {
        if isProviderConnected(provider) {
            triggerSync(for: provider, sinceDays: nil, mode: "full")
        } else {
            // Simulate connection
            DispatchQueue.main.async { [weak self] in
                self?.providerStates[provider] = .syncing(progress: "Connecting...")
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) {
                    self?.providerStates[provider] = .active
                    self?.addSyncEvent(for: provider, type: .connected, message: "Connected successfully")
                }
            }
        }
    }
    
    func startLocalOAuth(for provider: Provider) {
        // Mock OAuth flow
        DispatchQueue.main.async { [weak self] in
            guard let self = self else { return }
            self.providerStates[provider] = .syncing(progress: "Opening browser for auth...")
            
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) {
                self.providerStates[provider] = .active
                self.addSyncEvent(for: provider, type: .connected, message: "OAuth completed successfully")
            }
        }
    }
    
    func cancelSync(for provider: Provider) {
        DispatchQueue.main.async { [weak self] in
            self?.providerStates[provider] = .idle
            self?.syncProgressData[provider] = nil
        }
    }
    
    func cancelAllSyncs() {
        DispatchQueue.main.async { [weak self] in
            for provider in Provider.allCases {
                if self?.providerStates[provider]?.isSyncing == true {
                    self?.providerStates[provider] = .idle
                }
            }
            self?.syncProgressData = [:]
        }
    }
    
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
    
    func runDiscovery(for provider: Provider, completion: @escaping (Result<SlackDiscoveryResult, Error>) -> Void) {
        // Mock discovery result
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) {
            let formatter = ISO8601DateFormatter()
            let mockResult = SlackDiscoveryResult(
                publicChannels: 15,
                privateChannels: 8,
                dms: 42,
                groupDms: 5,
                totalChannels: 70,
                oldestMessageDate: "2020-01-01T00:00:00Z",
                newestMessageDate: formatter.string(from: Date()),
                estimatedFullSyncMinutes: 45,
                estimatedQuickSyncMinutes: 3
            )
            completion(.success(mockResult))
        }
    }
}
#endif // DEBUG
