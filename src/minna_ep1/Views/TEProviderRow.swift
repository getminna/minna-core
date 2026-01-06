import SwiftUI

/// Expandable provider row with sync history accordion
struct TEProviderRow: View {
    let provider: Provider
    let status: SyncStatus
    let syncHistory: [SyncEvent]
    let syncProgress: SyncProgress?
    let onSync: () -> Void
    let onCancel: () -> Void
    
    @State private var isExpanded = false
    
    init(provider: Provider, status: SyncStatus, syncHistory: [SyncEvent], syncProgress: SyncProgress? = nil, onSync: @escaping () -> Void, onCancel: @escaping () -> Void) {
        self.provider = provider
        self.status = status
        self.syncHistory = syncHistory
        self.syncProgress = syncProgress
        self.onSync = onSync
        self.onCancel = onCancel
    }
    
    var body: some View {
        VStack(spacing: 0) {
            // Main row
            mainRow
            
            // Active sync progress
            if status.isSyncing {
                syncProgressSection
            }
            
            // Expandable history section
            if isExpanded && !syncHistory.isEmpty {
                historySection
            }
        }
        .background(CityPopTheme.surface)
    }
    
    // MARK: - Main Row
    
    private var mainRow: some View {
        HStack(spacing: 0) {
            // Subtle accent bar
            Rectangle()
                .fill(CityPopTheme.providerColor(for: provider.displayName))
                .frame(width: 3)
            
            HStack(spacing: 14) {
                // Provider icon (branded style)
                ProviderIcon(provider: provider, size: 40)
                
                // Provider info
                VStack(alignment: .leading, spacing: 3) {
                    Text(provider.displayName.uppercased())
                        .font(.system(size: 12, weight: .semibold, design: .default))
                        .tracking(1.2)
                        .foregroundColor(CityPopTheme.textPrimary)
                    
                    Text(provider.description)
                        .font(.system(size: 10, weight: .regular))
                        .foregroundColor(CityPopTheme.textMuted)
                }
                
                Spacer()
                
                // Status (only show if not syncing - progress shows instead)
                if !status.isSyncing {
                    HStack(spacing: 6) {
                        StatusIndicator(status: status)
                        
                        Text(statusText)
                            .font(.system(size: 10, weight: .medium))
                            .foregroundColor(CityPopTheme.textSecondary)
                    }
                }
                
                // Expand button (if has history)
                if !syncHistory.isEmpty || status.isActive {
                    Button(action: { withAnimation(.easeInOut(duration: 0.2)) { isExpanded.toggle() } }) {
                        Image(systemName: isExpanded ? "chevron.up" : "chevron.down")
                            .font(.system(size: 10, weight: .medium))
                            .foregroundColor(CityPopTheme.textMuted)
                            .frame(width: 24, height: 24)
                    }
                    .buttonStyle(.plain)
                }
                
                // Action button
                actionButton
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 12)
        }
    }
    
    // MARK: - Sync Progress Section
    
    private var syncProgressSection: some View {
        VStack(spacing: 0) {
            Rectangle()
                .fill(CityPopTheme.border)
                .frame(height: 1)
                .padding(.leading, 17)
            
            HStack(spacing: 12) {
                // Animated spinner
                ProgressView()
                    .scaleEffect(0.7)
                    .frame(width: 16, height: 16)
                    .tint(CityPopTheme.accent)
                
                if let progress = syncProgress {
                    SyncProgressView(progress: progress)
                } else {
                    // Fallback to status text
                    Text(statusText.uppercased())
                        .font(.system(size: 10, weight: .semibold, design: .monospaced))
                        .foregroundColor(CityPopTheme.accent)
                    Spacer()
                }
            }
            .padding(.horizontal, 14)
            .padding(.leading, 17)
            .padding(.vertical, 10)
            .background(CityPopTheme.surface)
        }
    }
    
    // MARK: - History Section (Accordion)
    
    private var historySection: some View {
        VStack(spacing: 0) {
            Rectangle()
                .fill(CityPopTheme.border)
                .frame(height: 1)
                .padding(.leading, 17)
            
            VStack(spacing: 0) {
                ForEach(syncHistory.prefix(5)) { event in
                    HStack(spacing: 0) {
                        Text(event.timeString)
                            .font(.system(size: 10, weight: .regular, design: .monospaced))
                            .foregroundColor(CityPopTheme.textMuted)
                            .frame(width: 44, alignment: .leading)
                        
                        Text(event.dateString)
                            .font(.system(size: 10, weight: .regular, design: .monospaced))
                            .foregroundColor(CityPopTheme.textMuted)
                            .frame(width: 48, alignment: .leading)
                        
                        Circle()
                            .fill(eventColor(for: event.type))
                            .frame(width: 4, height: 4)
                            .padding(.trailing, 8)
                        
                        Text(event.message)
                            .font(.system(size: 10, weight: .regular))
                            .foregroundColor(CityPopTheme.textSecondary)
                        
                        Spacer()
                    }
                    .padding(.vertical, 6)
                    .padding(.horizontal, 14)
                }
            }
            .padding(.leading, 17)
            .padding(.vertical, 4)
            .background(CityPopTheme.background.opacity(0.5))
        }
    }
    
    private func eventColor(for type: SyncEvent.EventType) -> Color {
        switch type {
        case .initialSync: return CityPopTheme.success
        case .deltaSync: return CityPopTheme.success.opacity(0.6)
        case .error: return CityPopTheme.error
        case .connected: return CityPopTheme.providerColor(for: provider.displayName)
        }
    }
    
    // MARK: - Action Button
    
    @ViewBuilder
    private var actionButton: some View {
        if status.isSyncing {
            ActionButton(title: "Stop", icon: nil, style: .destructive) {
                onCancel()
            }
        } else if status.isActive {
            ActionButton(title: "Sync", icon: "arrow.triangle.2.circlepath", style: .secondary) {
                onSync()
            }
        } else {
            ActionButton(title: "Connect", icon: nil, style: .primary) {
                onSync()
            }
        }
    }
    
    private var statusText: String {
        switch status {
        case .syncing(let progress):
            return progress ?? "Syncing..."
        case .error:
            return "Error"
        case .idle:
            return "Not connected"
        case .active:
            return "Connected"
        }
    }
}

// MARK: - Preview

struct TEProviderRow_Previews: PreviewProvider {
    static var previews: some View {
        VStack(spacing: 1) {
            TEProviderRow(
                provider: .slack,
                status: .syncing(progress: "Indexing messages..."),
                syncHistory: [],
                syncProgress: SyncProgress(documentsProcessed: 1247, totalDocuments: 3500, currentAction: "Indexing #engineering"),
                onSync: {},
                onCancel: {}
            )
            
            TEProviderRow(
                provider: .googleWorkspace,
                status: .active,
                syncHistory: [
                    SyncEvent(timestamp: Date(), type: .deltaSync, message: "Synced 42 emails"),
                    SyncEvent(timestamp: Date().addingTimeInterval(-3600), type: .initialSync, message: "Initial sync completed"),
                ],
                onSync: {},
                onCancel: {}
            )
            
            TEProviderRow(
                provider: .github,
                status: .idle,
                syncHistory: [],
                onSync: {},
                onCancel: {}
            )
        }
        .padding(20)
        .background(CityPopTheme.background)
    }
}
