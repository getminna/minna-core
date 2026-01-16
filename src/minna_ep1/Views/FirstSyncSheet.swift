import SwiftUI
#if canImport(Inject)
import Inject
#endif

/// Discovery results from the backend
struct SlackDiscoveryResult: Codable {
    let publicChannels: Int
    let privateChannels: Int
    let dms: Int
    let groupDms: Int
    let totalChannels: Int
    let oldestMessageDate: String?
    let newestMessageDate: String?
    let estimatedFullSyncMinutes: Int
    let estimatedQuickSyncMinutes: Int
    
    enum CodingKeys: String, CodingKey {
        case publicChannels = "public_channels"
        case privateChannels = "private_channels"
        case dms
        case groupDms = "group_dms"
        case totalChannels = "total_channels"
        case oldestMessageDate = "oldest_message_date"
        case newestMessageDate = "newest_message_date"
        case estimatedFullSyncMinutes = "estimated_full_sync_minutes"
        case estimatedQuickSyncMinutes = "estimated_quick_sync_minutes"
    }
}

/// First sync experience for large data sources like Slack
/// Shows discovery results and lets user choose Quick Start vs Full History
struct FirstSyncSheet: View {
    let provider: Provider
    let engine: SignalProviderWrapper
    let onStartSync: (SyncMode) -> Void
    let onCancel: () -> Void
    
    enum SyncMode {
        case quick  // Last 7 days
        case full   // All history
    }
    
    enum DiscoveryState {
        case loading
        case discovered(SlackDiscoveryResult)
        case error(String)
    }
    
    @State private var discoveryState: DiscoveryState = .loading
    @State private var selectedMode: SyncMode = .quick
    
    init(provider: Provider, engine: SignalProviderWrapper? = nil, onStartSync: @escaping (SyncMode) -> Void, onCancel: @escaping () -> Void) {
        self.provider = provider
        // Use provided wrapper or create one from RealSignalEngine
        self.engine = engine ?? SignalProviderWrapper(RealSignalEngine())
        self.onStartSync = onStartSync
        self.onCancel = onCancel
    }
    
    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    switch discoveryState {
                    case .loading:
                        loadingView
                    case .discovered(let result):
                        discoveredView(result)
                    case .error(let message):
                        errorView(message)
                    }
                }
                .padding(24)
            }
            
            Divider()
            footer
        }
        .frame(width: 480, height: 520)
        .background(CityPopTheme.background)
        .onAppear {
            runDiscovery()
        }
#if canImport(Inject)
        .enableInjection()
        #endif
    }

    #if canImport(Inject)
    @ObserveInjection var forceRedraw
    #endif

    // MARK: - Header
    
    private var header: some View {
        HStack {
            VStack(alignment: .leading, spacing: 4) {
                Text("Set Up \(provider.displayName) Sync")
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundColor(CityPopTheme.textPrimary)
                
                Text("Choose how much history to sync")
                    .font(.system(size: 12))
                    .foregroundColor(CityPopTheme.textSecondary)
            }
            
            Spacer()
            
            Button(action: onCancel) {
                Image(systemName: "xmark")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundColor(CityPopTheme.textMuted)
            }
            .buttonStyle(.plain)
        }
        .padding(20)
        .background(CityPopTheme.surface)
    }
    
    // MARK: - Loading View
    
    private var loadingView: some View {
        VStack(spacing: 16) {
            ProgressView()
                .scaleEffect(1.2)
            
            Text("Scanning your \(provider.displayName) workspace...")
                .font(.system(size: 13))
                .foregroundColor(CityPopTheme.textSecondary)
            
            Text("This only takes a moment")
                .font(.system(size: 11))
                .foregroundColor(CityPopTheme.textMuted)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 60)
    }
    
    // MARK: - Discovered View
    
    private func discoveredView(_ result: SlackDiscoveryResult) -> some View {
        VStack(alignment: .leading, spacing: 20) {
            // Discovery summary
            discoveryCard(result)
            
            // Sync options
            Text("Choose your sync strategy:")
                .font(.system(size: 13, weight: .medium))
                .foregroundColor(CityPopTheme.textPrimary)
            
            // Quick Start option
            syncOptionCard(
                mode: .quick,
                title: "Quick Start",
                subtitle: "Last 7 days",
                time: "~\(result.estimatedQuickSyncMinutes) min",
                description: "Get started fast. We'll continue syncing older messages in the background.",
                isSelected: selectedMode == .quick,
                isRecommended: true
            )
            
            // Full History option
            syncOptionCard(
                mode: .full,
                title: "Full History",
                subtitle: "All messages",
                time: "~\(result.estimatedFullSyncMinutes) min",
                description: "Sync everything now. Best if you need historical context immediately.",
                isSelected: selectedMode == .full,
                isRecommended: false
            )
            
            // Background sync note
            if selectedMode == .quick {
                backgroundSyncNote
            }
        }
    }
    
    private func discoveryCard(_ result: SlackDiscoveryResult) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 6) {
                Image(systemName: "chart.bar.fill")
                    .foregroundColor(CityPopTheme.accentSecondary)
                Text("We found:")
                    .font(.system(size: 13, weight: .medium))
                    .foregroundColor(CityPopTheme.textPrimary)
            }
            
            if provider == .googleWorkspace {
                // Google Workspace: Show file count
                HStack(spacing: 24) {
                    statItem(count: result.totalChannels, label: "Files")
                }
            } else {
                // Slack: Show channel breakdown
                HStack(spacing: 24) {
                    statItem(count: result.publicChannels, label: "Public")
                    statItem(count: result.privateChannels, label: "Private")
                    statItem(count: result.dms, label: "DMs")
                    statItem(count: result.groupDms, label: "Group DMs")
                }
            }
            
            Divider()
            
            HStack {
                Text(provider == .googleWorkspace ? "\(result.totalChannels) total files" : "\(result.totalChannels) total conversations")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundColor(CityPopTheme.textPrimary)
                
                Spacer()
                
                if let oldest = result.oldestMessageDate {
                    Text("Since \(oldest)")
                        .font(.system(size: 11))
                        .foregroundColor(CityPopTheme.textMuted)
                }
            }
        }
        .padding(16)
        .background(CityPopTheme.surface)
        .cornerRadius(8)
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(CityPopTheme.border, lineWidth: 1))
    }
    
    private func statItem(count: Int, label: String) -> some View {
        VStack(spacing: 2) {
            Text("\(count)")
                .font(.system(size: 18, weight: .semibold, design: .rounded))
                .foregroundColor(CityPopTheme.textPrimary)
            Text(label)
                .font(.system(size: 10))
                .foregroundColor(CityPopTheme.textMuted)
        }
    }
    
    private func syncOptionCard(
        mode: SyncMode,
        title: String,
        subtitle: String,
        time: String,
        description: String,
        isSelected: Bool,
        isRecommended: Bool
    ) -> some View {
        Button(action: { selectedMode = mode }) {
            HStack(alignment: .top, spacing: 12) {
                // Radio button - teal for sync-related selections
                Circle()
                    .strokeBorder(isSelected ? CityPopTheme.accentSecondary : CityPopTheme.border, lineWidth: 2)
                    .background(Circle().fill(isSelected ? CityPopTheme.accentSecondary : Color.clear))
                    .frame(width: 20, height: 20)
                    .overlay(
                        isSelected ? Circle().fill(Color.white).frame(width: 8, height: 8) : nil
                    )
                    .padding(.top, 2)
                
                VStack(alignment: .leading, spacing: 4) {
                    HStack {
                        Text(title)
                            .font(.system(size: 14, weight: .semibold))
                            .foregroundColor(CityPopTheme.textPrimary)
                        
                        if isRecommended {
                            Text("Recommended")
                                .font(.system(size: 9, weight: .bold))
                                .foregroundColor(.white)
                                .padding(.horizontal, 6)
                                .padding(.vertical, 2)
                                .background(CityPopTheme.success)
                                .cornerRadius(4)
                        }
                        
                        Spacer()
                        
                        Text(time)
                            .font(.system(size: 12, weight: .medium))
                            .foregroundColor(CityPopTheme.textSecondary)
                    }
                    
                    Text(subtitle)
                        .font(.system(size: 12))
                        .foregroundColor(CityPopTheme.textSecondary)
                    
                    Text(description)
                        .font(.system(size: 11))
                        .foregroundColor(CityPopTheme.textMuted)
                        .lineSpacing(2)
                }
            }
            .padding(16)
            .background(isSelected ? CityPopTheme.accentSecondary.opacity(0.05) : CityPopTheme.surface)
            .cornerRadius(8)
            .overlay(
                RoundedRectangle(cornerRadius: 8)
                    .stroke(isSelected ? CityPopTheme.accentSecondary : CityPopTheme.border, lineWidth: isSelected ? 2 : 1)
            )
        }
        .buttonStyle(.plain)
    }
    
    private var backgroundSyncNote: some View {
        HStack(spacing: 10) {
            Image(systemName: "arrow.trianglehead.2.clockwise.rotate.90")
                .font(.system(size: 14))
                .foregroundColor(CityPopTheme.accentSecondary)
            
            VStack(alignment: .leading, spacing: 2) {
                Text("Background sync enabled")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundColor(CityPopTheme.textPrimary)
                Text("After Quick Start completes, we'll automatically sync older messages while you use Minna.")
                    .font(.system(size: 10))
                    .foregroundColor(CityPopTheme.textMuted)
            }
        }
        .padding(12)
        .background(CityPopTheme.accentSecondary.opacity(0.1))
        .cornerRadius(8)
    }
    
    // MARK: - Error View
    
    private func errorView(_ message: String) -> some View {
        VStack(spacing: 16) {
            Image(systemName: "exclamationmark.triangle.fill")
                .font(.system(size: 32))
                .foregroundColor(CityPopTheme.error)
            
            Text("Discovery failed")
                .font(.system(size: 14, weight: .medium))
                .foregroundColor(CityPopTheme.textPrimary)
            
            Text(message)
                .font(.system(size: 12))
                .foregroundColor(CityPopTheme.textSecondary)
                .multilineTextAlignment(.center)
            
            Button(action: runDiscovery) {
                Text("Try Again")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundColor(CityPopTheme.accent)
            }
            .buttonStyle(.plain)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 40)
    }
    
    // MARK: - Footer
    
    private var footer: some View {
        HStack {
            Button(action: onCancel) {
                Text("Cancel")
                    .font(.system(size: 13, weight: .medium))
                    .foregroundColor(CityPopTheme.textSecondary)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
            }
            .buttonStyle(.plain)
            
            Spacer()
            
            if case .discovered = discoveryState {
                Button(action: { onStartSync(selectedMode) }) {
                    Text(selectedMode == .quick ? "Start Quick Sync" : "Start Full Sync")
                        .font(.system(size: 13, weight: .medium))
                        .foregroundColor(.white)
                        .padding(.horizontal, 20)
                        .padding(.vertical, 8)
                        .background(CityPopTheme.accentSecondary)
                        .cornerRadius(6)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(20)
        .background(CityPopTheme.surface)
    }
    
    // MARK: - Discovery
    
    private func runDiscovery() {
        discoveryState = .loading
        
        // Run the discover CLI command
        engine.runDiscovery(for: provider) { result in
            DispatchQueue.main.async {
                switch result {
                case .success(let discoveryResult):
                    self.discoveryState = .discovered(discoveryResult)
                case .failure(let error):
                    self.discoveryState = .error(error.localizedDescription)
                }
            }
        }
    }
}

// MARK: - Preview

#if DEBUG
#Preview {
    FirstSyncSheet(
        provider: .slack,
        engine: SignalProviderWrapper(MockSignalEngine(state: .sunny)),
        onStartSync: { mode in print("Start sync: \(mode)") },
        onCancel: {}
    )
}
#endif

