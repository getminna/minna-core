import SwiftUI

/// Navigation sections
enum ControlSection: String, CaseIterable {
    case sources = "Sources"
    case briefing = "Briefing"
    case settings = "Settings"
    
    var icon: String {
        switch self {
        case .sources: return "arrow.triangle.2.circlepath"
        case .briefing: return "sun.horizon"
        case .settings: return "gearshape"
        }
    }
}

/// Main control center - Linear-inspired clean layout
struct ControlCenterView: View {
    @ObservedObject var engine = MinnaEngineManager.shared
    @StateObject private var oauthManager = LocalOAuthManager.shared
    @State private var selectedSection: ControlSection = .sources
    @State private var showingConfigSheet: Provider? = nil
    
    var body: some View {
        HStack(spacing: 0) {
            sidebar
            
            Rectangle()
                .fill(CityPopTheme.border)
                .frame(width: 1)
            
            mainContent
        }
        .frame(minWidth: 720, minHeight: 480)
        .background(CityPopTheme.background)
    }
    
    // MARK: - Sidebar (Linear-style minimal)
    
    private var sidebar: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Logo - using cropped wordmark, ~2/3 sidebar width (120px)
            Group {
                if let wordmarkURL = Bundle.main.url(forResource: "minna_wordmark_cropped", withExtension: "png"),
                   let wordmarkData = try? Data(contentsOf: wordmarkURL),
                   let nsImage = NSImage(data: wordmarkData) {
                    Image(nsImage: nsImage)
                        .resizable()
                        .aspectRatio(contentMode: .fit)
                        .frame(width: 120)
                } else {
                    Text("Minna")
                        .font(.system(size: 24, weight: .bold))
                        .foregroundColor(CityPopTheme.textPrimary)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 16)
            .padding(.top, 20)
            .padding(.bottom, 24)
            
            // Navigation
            VStack(spacing: 2) {
                ForEach(ControlSection.allCases, id: \.self) { section in
                    SidebarNavItem(
                        title: section.rawValue,
                        icon: section.icon,
                        isSelected: selectedSection == section
                    ) {
                        withAnimation(.easeOut(duration: 0.15)) {
                            selectedSection = section
                        }
                    }
                }
            }
            .padding(.horizontal, 8)
            
            Spacer()
            
            // Status footer
            statusFooter
        }
        .frame(width: 180)
        .background(CityPopTheme.sidebarBackground)
    }
    
    private var statusFooter: some View {
        HStack(spacing: 8) {
            Circle()
                .fill(engine.isAnySyncing ? CityPopTheme.syncing : CityPopTheme.success)
                .frame(width: 6, height: 6)
            
            Text(engine.isAnySyncing ? "Syncing..." : "Ready")
                .font(.system(size: 11, weight: .medium))
                .foregroundColor(CityPopTheme.textMuted)
            
            Spacer()
            
            Text("v0.1")
                .font(.system(size: 10, weight: .medium))
                .foregroundColor(CityPopTheme.textMuted.opacity(0.6))
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
    }
    
    // MARK: - Main Content
    
    @ViewBuilder
    private var mainContent: some View {
        switch selectedSection {
        case .sources:
            sourcesView
        case .briefing:
            BriefingView()
        case .settings:
            settingsView
        }
    }
    
    // MARK: - Sources View
    
    private var sourcesView: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Header
            VStack(alignment: .leading, spacing: 4) {
                Text("Data Sources")
                    .font(.system(size: 18, weight: .semibold))
                    .foregroundColor(CityPopTheme.textPrimary)
                
                Text("Connect and sync your workspace tools")
                    .font(.system(size: 13))
                    .foregroundColor(CityPopTheme.textSecondary)
            }
            .padding(.horizontal, 24)
            .padding(.top, 24)
            .padding(.bottom, 20)
            
            // Provider list
            ScrollView {
                VStack(spacing: 12) {
                    ForEach(Provider.allCases) { provider in
                        ProviderCard(
                            provider: provider,
                            status: engine.providerStates[provider] ?? .idle,
                            syncHistory: engine.syncHistory[provider] ?? [],
                            syncProgress: engine.syncProgressData[provider]?.toSyncProgress(),
                            onSync: { handleConnect(provider: provider) },
                            onCancel: { engine.cancelSync(for: provider) },
                            onConfigure: { showingConfigSheet = provider }
                        )
                    }
                }
                .padding(.horizontal, 24)
                .padding(.bottom, 24)
            }
        }
        .background(CityPopTheme.background)
        .sheet(item: $showingConfigSheet) { provider in
            providerConfigSheet(for: provider)
        }
    }
    
    // MARK: - Provider Config Sheets
    
    @ViewBuilder
    private func providerConfigSheet(for provider: Provider) -> some View {
        switch provider {
        case .slack:
            SlackConfigSheet(
                onComplete: {
                    showingConfigSheet = nil
                    engine.triggerSync(for: provider)
                },
                onCancel: {
                    showingConfigSheet = nil
                }
            )
            
        case .googleWorkspace:
            ProviderConfigSheet(
                provider: provider,
                onComplete: {
                    showingConfigSheet = nil
                    engine.triggerSync(for: provider)
                },
                onCancel: {
                    showingConfigSheet = nil
                }
            )
            
        case .github:
            GitHubConfigSheet(
                onComplete: {
                    showingConfigSheet = nil
                    engine.triggerSync(for: provider)
                },
                onCancel: {
                    showingConfigSheet = nil
                }
            )
        }
    }
    
    // MARK: - Connect Handler
    
    private func handleConnect(provider: Provider) {
        // All providers now use Sovereign Mode - show config sheet if not configured
        if CredentialManager.shared.isReady(for: provider) {
            // Already configured - just sync
            engine.triggerSync(for: provider)
        } else {
            // Show provider-specific config sheet
            showingConfigSheet = provider
        }
    }
    
    // MARK: - Settings View
    
    private var settingsView: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Header
            VStack(alignment: .leading, spacing: 4) {
                Text("Settings")
                    .font(.system(size: 18, weight: .semibold))
                    .foregroundColor(CityPopTheme.textPrimary)
                
                Text("Configure your Minna experience")
                    .font(.system(size: 13))
                    .foregroundColor(CityPopTheme.textSecondary)
            }
            .padding(.horizontal, 24)
            .padding(.top, 24)
            .padding(.bottom, 20)
            
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    // About section
                    VStack(alignment: .leading, spacing: 12) {
                        Text("About")
                            .font(.system(size: 12, weight: .medium))
                            .foregroundColor(CityPopTheme.textMuted)
                            .textCase(.uppercase)
                        
                        VStack(spacing: 0) {
                            SettingsRow(label: "Version", value: "0.1.0")
                            Rectangle().fill(CityPopTheme.divider).frame(height: 1)
                            SettingsRow(label: "Engine Status", value: "Running", statusColor: CityPopTheme.success)
                        }
                        .background(CityPopTheme.surface)
                        .cornerRadius(8)
                        .overlay(
                            RoundedRectangle(cornerRadius: 8)
                                .stroke(CityPopTheme.border, lineWidth: 1)
                        )
                    }
                }
                .padding(.horizontal, 24)
                .padding(.bottom, 24)
            }
        }
        .background(CityPopTheme.background)
    }
}

// MARK: - Sidebar Nav Item

struct SidebarNavItem: View {
    let title: String
    let icon: String
    let isSelected: Bool
    let action: () -> Void
    
    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                Image(systemName: icon)
                    .font(.system(size: 14, weight: .medium))
                    .frame(width: 20)
                    .foregroundColor(isSelected ? CityPopTheme.accent : CityPopTheme.textSecondary)
                
                Text(title)
                    .font(.system(size: 13, weight: isSelected ? .semibold : .regular))
                    .foregroundColor(isSelected ? CityPopTheme.textPrimary : CityPopTheme.textSecondary)
                
                Spacer()
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .background(isSelected ? CityPopTheme.surface : Color.clear)
            .cornerRadius(6)
        }
        .buttonStyle(.plain)
    }
}

// MARK: - Provider Card (Linear-style)

struct ProviderCard: View {
    let provider: Provider
    let status: SyncStatus
    let syncHistory: [SyncEvent]
    let syncProgress: SyncProgress?
    let onSync: () -> Void
    let onCancel: () -> Void
    var onConfigure: (() -> Void)? = nil
    
    @State private var isExpanded = false
    @StateObject private var oauthManager = LocalOAuthManager.shared
    
    var body: some View {
        VStack(spacing: 0) {
            // Main row
            HStack(spacing: 14) {
                // Provider icon
                ProviderIconSimple(provider: provider)
                
                // Info
                VStack(alignment: .leading, spacing: 4) {
                    Text(provider.displayName)
                        .font(.system(size: 14, weight: .medium))
                        .foregroundColor(CityPopTheme.textPrimary)
                    
                    // Status with gradient dot + gray text
                    HStack(spacing: 6) {
                        StatusDot(status: status)
                        
                        Text(statusText)
                            .font(.system(size: 12))
                            .foregroundColor(CityPopTheme.textSecondary)
                    }
                }
                
                Spacer()
                
                // Expand button (if has history)
                if !syncHistory.isEmpty {
                    Button(action: { withAnimation(.easeOut(duration: 0.15)) { isExpanded.toggle() } }) {
                        Image(systemName: isExpanded ? "chevron.up" : "chevron.down")
                            .font(.system(size: 11, weight: .medium))
                            .foregroundColor(CityPopTheme.textMuted)
                    }
                    .buttonStyle(.plain)
                }
                
                // Action button
                actionButton
            }
            .padding(16)
            
            // Progress section (when syncing)
            if status.isSyncing {
                progressSection
            }
            
            // History section (when expanded)
            if isExpanded && !syncHistory.isEmpty {
                historySection
            }
        }
        .background(CityPopTheme.surface)
        .cornerRadius(8)
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .stroke(CityPopTheme.border, lineWidth: 1)
        )
    }
    
    private var progressSection: some View {
        VStack(spacing: 0) {
            Rectangle().fill(CityPopTheme.divider).frame(height: 1)
            
            VStack(alignment: .leading, spacing: 8) {
                if let progress = syncProgress {
                    // Progress bar with City Pop sunset gradient
                    GeometryReader { geo in
                        ZStack(alignment: .leading) {
                            RoundedRectangle(cornerRadius: 2)
                                .fill(CityPopTheme.divider)
                                .frame(height: 4)
                            
                            RoundedRectangle(cornerRadius: 2)
                                .fill(CityPopTheme.progressGradient)
                                .frame(width: max(geo.size.width * progress.percentage, 4), height: 4)
                        }
                    }
                    .frame(height: 4)
                    
                    HStack {
                        Text(progress.currentAction)
                            .font(.system(size: 12))
                            .foregroundColor(CityPopTheme.textSecondary)
                        
                        Spacer()
                        
                        Text("\(progress.documentsProcessed) docs")
                            .font(.system(size: 11, weight: .medium, design: .monospaced))
                            .foregroundColor(CityPopTheme.textMuted)
                    }
                } else {
                    HStack(spacing: 8) {
                        ProgressView()
                            .scaleEffect(0.7)
                        
                        Text("Syncing...")
                            .font(.system(size: 12))
                            .foregroundColor(CityPopTheme.textSecondary)
                    }
                }
            }
            .padding(16)
        }
    }
    
    private var historySection: some View {
        VStack(spacing: 0) {
            Rectangle().fill(CityPopTheme.divider).frame(height: 1)
            
            VStack(spacing: 0) {
                ForEach(Array(syncHistory.prefix(5))) { event in
                    HStack(spacing: 12) {
                        Circle()
                            .fill(eventColor(for: event.type))
                            .frame(width: 6, height: 6)
                        
                        Text(event.message)
                            .font(.system(size: 12))
                            .foregroundColor(CityPopTheme.textSecondary)
                        
                        Spacer()
                        
                        Text(event.relativeTimeString)
                            .font(.system(size: 11))
                            .foregroundColor(CityPopTheme.textMuted)
                    }
                    .padding(.vertical, 8)
                    .padding(.horizontal, 16)
                }
            }
            .background(CityPopTheme.background.opacity(0.5))
        }
    }
    
    private func eventColor(for type: SyncEvent.EventType) -> Color {
        switch type {
        case .initialSync, .deltaSync: return CityPopTheme.success
        case .error: return CityPopTheme.error
        case .connected: return CityPopTheme.accentCyan
        }
    }
    
    @ViewBuilder
    private var actionButton: some View {
        if status.isSyncing {
            SimpleButton(title: "Stop", style: .secondary) { onCancel() }
        } else if status.isActive {
            HStack(spacing: 8) {
                // Show configure gear for BYO-Key providers
                if provider == .googleWorkspace, let onConfigure = onConfigure {
                    Button(action: onConfigure) {
                        Image(systemName: "gearshape")
                            .font(.system(size: 12))
                            .foregroundColor(CityPopTheme.textMuted)
                    }
                    .buttonStyle(.plain)
                }
                SimpleButton(title: "Sync", style: .secondary) { onSync() }
            }
        } else {
            SimpleButton(title: "Connect", style: .primary) { onSync() }
        }
    }
    
    private var statusText: String {
        switch status {
        case .syncing(let progress): return progress ?? "Syncing..."
        case .error: return "Error"
        case .idle: return "Not connected"
        case .active: return "Connected"
        }
    }
    
    private var statusColor: Color {
        switch status {
        case .syncing: return CityPopTheme.syncing
        case .error: return CityPopTheme.error
        case .idle: return CityPopTheme.textMuted
        case .active: return CityPopTheme.success
        }
    }
}

// MARK: - Status Dot (Gradient)

struct StatusDot: View {
    let status: SyncStatus
    
    var body: some View {
        Circle()
            .fill(dotGradient)
            .frame(width: 8, height: 8)
    }
    
    private var dotGradient: LinearGradient {
        switch status {
        case .active:
            return CityPopTheme.statusConnectedGradient
        case .syncing:
            return CityPopTheme.statusSyncingGradient
        case .error:
            return CityPopTheme.statusErrorGradient
        case .idle:
            // Gray gradient for idle
            return LinearGradient(
                colors: [CityPopTheme.textMuted, CityPopTheme.textMuted.opacity(0.7)],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
        }
    }
}

// MARK: - Provider Icon (1px Line Glyphs)

struct ProviderIconSimple: View {
    let provider: Provider
    
    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 8)
                .fill(CityPopTheme.providerColor(for: provider.displayName).opacity(0.08))
                .frame(width: 40, height: 40)
            
            Image(systemName: iconName)
                .font(.system(size: 16, weight: .light)) // Thin 1px line weight
                .foregroundColor(CityPopTheme.providerColor(for: provider.displayName))
        }
    }
    
    private var iconName: String {
        switch provider {
        case .slack: return "bubble.left.and.bubble.right"  // Conversation/channels
        case .googleWorkspace: return "calendar"             // Calendar is core to GWS
        case .github: return "arrow.triangle.branch"         // Git branching
        }
    }
}

// MARK: - Simple Button

struct SimpleButton: View {
    let title: String
    let style: ButtonStyle
    let action: () -> Void
    
    enum ButtonStyle {
        case primary
        case secondary
    }
    
    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: 12, weight: .medium))
                .foregroundColor(style == .primary ? .white : CityPopTheme.textSecondary)
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
                .background(
                    RoundedRectangle(cornerRadius: 6)
                        .fill(style == .primary ? CityPopTheme.accent : Color.clear)
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 6)
                        .stroke(style == .primary ? Color.clear : CityPopTheme.border, lineWidth: 1)
                )
        }
        .buttonStyle(.plain)
    }
}

// MARK: - Settings Row

struct SettingsRow: View {
    let label: String
    let value: String
    var statusColor: Color? = nil
    
    var body: some View {
        HStack {
            Text(label)
                .font(.system(size: 13))
                .foregroundColor(CityPopTheme.textSecondary)
            
            Spacer()
            
            HStack(spacing: 6) {
                if let color = statusColor {
                    Circle()
                        .fill(color)
                        .frame(width: 6, height: 6)
                }
                
                Text(value)
                    .font(.system(size: 13))
                    .foregroundColor(CityPopTheme.textMuted)
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
    }
}

// MARK: - Preview

struct ControlCenterView_Previews: PreviewProvider {
    static var previews: some View {
        ControlCenterView()
    }
}
