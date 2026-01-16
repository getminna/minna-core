import Foundation
import SwiftUI
import Combine

/// Wrapper around MinnaEngineManager that conforms to SignalProvider protocol
/// This is a thin forwarding layer - no logic changes, just protocol conformance
class RealSignalEngine: SignalProvider {
    private let manager = MinnaEngineManager.shared
    private var cancellables = Set<AnyCancellable>()
    
    // MARK: - Published State (forwarded from manager)
    
    @Published var providerStates: [Provider: SyncStatus] = [:]
    @Published var syncHistory: [Provider: [SyncEvent]] = [:]
    @Published var syncProgressData: [Provider: SyncProgressData] = [:]
    @Published var outputLog: String = ""
    @Published var isEngineReady: Bool = false
    
    // MARK: - Computed Properties (forwarded from manager)
    
    var state: EngineState {
        manager.state
    }
    
    var isAnySyncing: Bool {
        manager.isAnySyncing
    }
    
    var activeProviders: [Provider] {
        manager.activeProviders
    }
    
    // MARK: - Initialization
    
    init() {
        // Initialize with current manager state
        providerStates = manager.providerStates
        syncHistory = manager.syncHistory
        syncProgressData = manager.syncProgressData
        outputLog = manager.outputLog
        isEngineReady = manager.isEngineReady
        
        // Forward all @Published property changes from manager using sink
        manager.$providerStates
            .sink { [weak self] newValue in
                self?.providerStates = newValue
            }
            .store(in: &cancellables)
        
        manager.$syncHistory
            .sink { [weak self] newValue in
                self?.syncHistory = newValue
            }
            .store(in: &cancellables)
        
        manager.$syncProgressData
            .sink { [weak self] newValue in
                self?.syncProgressData = newValue
            }
            .store(in: &cancellables)
        
        manager.$outputLog
            .sink { [weak self] newValue in
                self?.outputLog = newValue
            }
            .store(in: &cancellables)
        
        manager.$isEngineReady
            .sink { [weak self] newValue in
                self?.isEngineReady = newValue
            }
            .store(in: &cancellables)
    }
    
    // MARK: - Methods (forwarded to manager)
    
    func isProviderConnected(_ provider: Provider) -> Bool {
        manager.isProviderConnected(provider)
    }
    
    func triggerSync(for provider: Provider, sinceDays: Int?, mode: String) {
        manager.triggerSync(for: provider, sinceDays: sinceDays, mode: mode)
    }
    
    func connectOrSync(for provider: Provider) {
        manager.connectOrSync(for: provider)
    }
    
    func startLocalOAuth(for provider: Provider) {
        manager.startLocalOAuth(for: provider)
    }
    
    func cancelSync(for provider: Provider) {
        manager.cancelSync(for: provider)
    }
    
    func cancelAllSyncs() {
        manager.cancelAllSyncs()
    }
    
    func addSyncEvent(for provider: Provider, type: SyncEvent.EventType, message: String) {
        manager.addSyncEvent(for: provider, type: type, message: message)
    }
    
    func runDiscovery(for provider: Provider, completion: @escaping (Result<SlackDiscoveryResult, Error>) -> Void) {
        manager.runDiscovery(for: provider, completion: completion)
    }
}
