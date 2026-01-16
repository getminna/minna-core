import Foundation
import SwiftUI
import Combine

/// Protocol that abstracts the engine interface for UI components
/// Allows views to work with either RealSignalEngine (Rust backend) or MockSignalEngine (instant mocks)
protocol SignalProvider: AnyObject, ObservableObject {
    // MARK: - Published State
    
    /// Current sync status for each provider
    var providerStates: [Provider: SyncStatus] { get }
    
    /// Sync history events per provider
    var syncHistory: [Provider: [SyncEvent]] { get }
    
    /// Active sync progress data per provider
    var syncProgressData: [Provider: SyncProgressData] { get }
    
    /// Engine initialization status
    var isEngineReady: Bool { get }
    
    /// Output log for debugging
    var outputLog: String { get }
    
    // MARK: - Computed Properties
    
    /// Overall engine state (welcome, syncing, running)
    var state: EngineState { get }
    
    /// Whether any provider is currently syncing
    var isAnySyncing: Bool { get }
    
    /// List of providers that are currently active
    var activeProviders: [Provider] { get }
    
    // MARK: - Methods
    
    /// Check if a provider has credentials stored
    func isProviderConnected(_ provider: Provider) -> Bool
    
    /// Trigger a sync for a provider
    /// - Parameters:
    ///   - provider: The provider to sync
    ///   - sinceDays: Optional number of days to sync (for quick sync)
    ///   - mode: Sync mode ("full" or "quick")
    func triggerSync(for provider: Provider, sinceDays: Int?, mode: String)
    
    /// Smart connect/sync handler - connects if not connected, syncs if already connected
    func connectOrSync(for provider: Provider)
    
    /// Start local OAuth flow for a provider (Google Workspace)
    func startLocalOAuth(for provider: Provider)
    
    /// Cancel an active sync for a provider
    func cancelSync(for provider: Provider)
    
    /// Cancel all active syncs
    func cancelAllSyncs()
    
    /// Add a sync event to history
    func addSyncEvent(for provider: Provider, type: SyncEvent.EventType, message: String)
    
    /// Run discovery for a provider (used by FirstSyncSheet)
    /// - Parameters:
    ///   - provider: The provider to discover
    ///   - completion: Callback with discovery result
    func runDiscovery(for provider: Provider, completion: @escaping (Result<SlackDiscoveryResult, Error>) -> Void)
}
