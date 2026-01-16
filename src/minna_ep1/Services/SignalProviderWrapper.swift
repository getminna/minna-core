import Foundation
import SwiftUI
import Combine

/// Type-erased wrapper that allows @ObservedObject to work with SignalProvider protocol
/// This bridges the gap between protocol types and property wrappers
class SignalProviderWrapper: ObservableObject {
    private let wrapped: any SignalProvider
    private var cancellables = Set<AnyCancellable>()
    
    // Forward all properties from wrapped engine
    var providerStates: [Provider: SyncStatus] {
        wrapped.providerStates
    }
    
    var syncHistory: [Provider: [SyncEvent]] {
        wrapped.syncHistory
    }
    
    var syncProgressData: [Provider: SyncProgressData] {
        wrapped.syncProgressData
    }
    
    var isEngineReady: Bool {
        wrapped.isEngineReady
    }
    
    var outputLog: String {
        wrapped.outputLog
    }
    
    var state: EngineState {
        wrapped.state
    }
    
    var isAnySyncing: Bool {
        wrapped.isAnySyncing
    }
    
    var activeProviders: [Provider] {
        wrapped.activeProviders
    }
    
    init(_ engine: any SignalProvider) {
        self.wrapped = engine
        
        // Subscribe to changes from the wrapped engine
        // Handle each concrete type separately to access objectWillChange
        if let realEngine = engine as? RealSignalEngine {
            // RealSignalEngine is ObservableObject - observe it directly
            realEngine.objectWillChange
                .sink { [weak self] _ in
                    self?.objectWillChange.send()
                }
                .store(in: &cancellables)
        } else if let mockEngine = engine as? MockSignalEngine {
            // MockSignalEngine is ObservableObject - observe it directly
            mockEngine.objectWillChange
                .sink { [weak self] _ in
                    self?.objectWillChange.send()
                }
                .store(in: &cancellables)
        }
    }
    
    // Forward all methods
    func isProviderConnected(_ provider: Provider) -> Bool {
        wrapped.isProviderConnected(provider)
    }
    
    func triggerSync(for provider: Provider, sinceDays: Int?, mode: String) {
        wrapped.triggerSync(for: provider, sinceDays: sinceDays, mode: mode)
    }
    
    func connectOrSync(for provider: Provider) {
        wrapped.connectOrSync(for: provider)
    }
    
    func startLocalOAuth(for provider: Provider) {
        wrapped.startLocalOAuth(for: provider)
    }
    
    func cancelSync(for provider: Provider) {
        wrapped.cancelSync(for: provider)
    }
    
    func cancelAllSyncs() {
        wrapped.cancelAllSyncs()
    }
    
    func addSyncEvent(for provider: Provider, type: SyncEvent.EventType, message: String) {
        wrapped.addSyncEvent(for: provider, type: type, message: message)
    }
    
    func runDiscovery(for provider: Provider, completion: @escaping (Result<SlackDiscoveryResult, Error>) -> Void) {
        wrapped.runDiscovery(for: provider, completion: completion)
    }
}
