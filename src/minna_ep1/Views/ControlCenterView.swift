import SwiftUI
import AppKit
#if canImport(Inject)
import Inject
#endif

/// Navigation sections
enum ControlSection: String, CaseIterable {
    case sources = "Sources"
    case settings = "Settings"
    
    var icon: String {
        switch self {
        case .sources: return "antenna.radiowaves.left.and.right"
        case .settings: return "gearshape"
        }
    }
}

/// Main control center - Linear-inspired clean layout
struct ControlCenterView: View {
    @ObservedObject var engine: SignalProviderWrapper
    @StateObject private var oauthManager = LocalOAuthManager.shared
    @StateObject private var mcpManager = MCPSourcesManager.shared
    @State private var selectedSection: ControlSection = .sources
    @State private var showingConfigSheet: Provider? = nil
    @State private var showingSettingsSheet: Provider? = nil
    @State private var showingFirstSyncSheet: Provider? = nil
    @State private var showingMCPSetupWizard = false
    
    init(engine: any SignalProvider = RealSignalEngine()) {
        self.engine = SignalProviderWrapper(engine)
    }
    
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
        #if canImport(Inject)
        .enableInjection()
        #endif
    }

    #if canImport(Inject)
    @ObserveInjection var forceRedraw
    #endif
    
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
            
            // Sources list
            ScrollView {
                VStack(spacing: 24) {
                    // MCP-First: Discovered Sources Section
                    DiscoveredSourcesSection(showingSetupWizard: $showingMCPSetupWizard)
                    
                    // Fallback to manual configuration
                    ManualConfigFallbackCard {
                        withAnimation(.easeOut(duration: 0.15)) {
                            selectedSection = .settings
                        }
                    }
                }
                .padding(.horizontal, 24)
                .padding(.bottom, 24)
            }
        }
        .background(CityPopTheme.background)
        .onAppear {
            // Auto-discover MCP sources on view load
            if mcpManager.discoveredServers.isEmpty && !mcpManager.isDiscovering {
                mcpManager.discover()
            }
        }
        .sheet(isPresented: $showingMCPSetupWizard) {
            MCPSetupWizard(
                onComplete: {
                    showingMCPSetupWizard = false
                    mcpManager.discover() // Refresh after setup
                },
                onCancel: {
                    showingMCPSetupWizard = false
                }
            )
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
                    // For Slack, show first sync experience instead of immediate sync
                    showingFirstSyncSheet = provider
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
                    engine.triggerSync(for: provider, sinceDays: nil, mode: "full")
                },
                onCancel: {
                    showingConfigSheet = nil
                }
            )
            
        case .github:
            GitHubConfigSheet(
                onComplete: {
                    showingConfigSheet = nil
                    engine.triggerSync(for: provider, sinceDays: nil, mode: "full")
                },
                onCancel: {
                    showingConfigSheet = nil
                }
            )
        
        case .cursor:
            // Local AI tool doesn't need config sheet - reads local files
            // This case shouldn't be reached, but handle gracefully
            EmptyView()
        }
    }
    
    // MARK: - Connect Handler
    
    private func handleConnect(provider: Provider) {
        // Cursor doesn't need auth - sync directly from local files
        if !provider.requiresAuth {
            engine.triggerSync(for: provider, sinceDays: nil, mode: "full")
            return
        }
        
        // All providers now use Sovereign Mode - show config sheet if not configured
        if CredentialManager.shared.isReady(for: provider) {
            // Already configured - check if this is first sync
            let isFirstSync = engine.syncHistory[provider]?.isEmpty ?? true
            if isFirstSync {
                // First time sync - show first sync sheet
                showingFirstSyncSheet = provider
            } else {
                // Already synced before - just sync
                engine.triggerSync(for: provider, sinceDays: nil, mode: "full")
            }
        } else {
            // Show provider-specific config sheet
            showingConfigSheet = provider
        }
    }
    
    // MARK: - MCP Config Helper
    
    private func mcpConfigJSON(for app: String) -> String {
        let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let socketPath = appSupport.appendingPathComponent("Minna/mcp.sock").path
        
        // Escape the path for JSON
        let escapedPath = socketPath.replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "\"", with: "\\\"")
        
        return """
{
  "mcpServers": {
    "minna": {
      "command": "nc",
      "args": [
        "-U",
        "\(escapedPath)"
      ]
    }
  }
}
"""
    }
    
    // MARK: - Settings View
    
    private var settingsView: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Header
            VStack(alignment: .leading, spacing: 4) {
                Text("Settings 🔥")
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
                VStack(alignment: .leading, spacing: 24) {
                    // Connected Sources section
                    VStack(alignment: .leading, spacing: 12) {
                        SourcesSectionHeader(
                            title: "Connected Sources",
                            subtitle: "Manual API key configuration"
                        )
                        
                        VStack(spacing: 8) {
                            ForEach(Provider.allCases) { provider in
                                ProviderCard(
                                    provider: provider,
                                    status: engine.providerStates[provider] ?? .idle,
                                    syncHistory: engine.syncHistory[provider] ?? [],
                                    syncProgress: engine.syncProgressData[provider]?.toSyncProgress(),
                                    onSync: { handleConnect(provider: provider) },
                                    onCancel: { engine.cancelSync(for: provider) },
                                    onSettings: { showingSettingsSheet = provider }
                                )
                            }
                        }
                    }
                    
                    // MCP Server Setup section
                    VStack(alignment: .leading, spacing: 12) {
                        SourcesSectionHeader(
                            title: "MCP Server Setup",
                            subtitle: "Use Minna in Cursor, Claude, and other MCP clients"
                        )
                        
                        VStack(alignment: .leading, spacing: 16) {
                            // Cursor setup
                            MCPSetupCard(
                                appName: "Cursor",
                                configPath: "~/.cursor/mcp.json",
                                configContent: mcpConfigJSON(for: "Cursor")
                            )
                            
                            // Claude setup
                            MCPSetupCard(
                                appName: "Claude Desktop",
                                configPath: "~/Library/Application Support/Claude/claude_desktop_config.json",
                                configContent: mcpConfigJSON(for: "Claude")
                            )
                        }
                    }
                    
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
        .sheet(item: $showingConfigSheet) { provider in
            providerConfigSheet(for: provider)
        }
        .sheet(item: $showingSettingsSheet) { provider in
            ConnectorSettingsSheet(
                provider: provider,
                onDisconnect: {
                    showingSettingsSheet = nil
                },
                onDone: {
                    showingSettingsSheet = nil
                }
            )
        }
        .sheet(item: $showingFirstSyncSheet) { provider in
            FirstSyncSheet(
                provider: provider,
                engine: engine,
                onStartSync: { mode in
                    showingFirstSyncSheet = nil
                    switch mode {
                    case .quick:
                        engine.triggerSync(for: provider, sinceDays: 7, mode: "quick")
                    case .full:
                        engine.triggerSync(for: provider, sinceDays: nil, mode: "full")
                    }
                },
                onCancel: {
                    showingFirstSyncSheet = nil
                }
            )
        }
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
        #if canImport(Inject)
        .enableInjection()
        #endif
    }

    #if canImport(Inject)
    @ObserveInjection var forceRedraw
    #endif
}

// MARK: - Provider Card (Linear-style)

struct ProviderCard: View {
    let provider: Provider
    let status: SyncStatus
    let syncHistory: [SyncEvent]
    let syncProgress: SyncProgress?
    let onSync: () -> Void
    let onCancel: () -> Void
    var onSettings: (() -> Void)? = nil
    
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
        #if canImport(Inject)
        .enableInjection()
        #endif
    }

    #if canImport(Inject)
    @ObserveInjection var forceRedraw
    #endif
    
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
            SimpleButton(title: "Stop", style: .tertiary) { onCancel() }
        } else if status.isActive {
            HStack(spacing: 8) {
                // Settings gear for all connected providers
                if let onSettings = onSettings {
                    Button(action: onSettings) {
                        Image(systemName: "gearshape")
                            .font(.system(size: 12))
                            .foregroundColor(CityPopTheme.textMuted)
                    }
                    .buttonStyle(.plain)
                }
                SimpleButton(title: "Sync", style: .secondary) { onSync() }
            }
        } else if case .error = status {
            // Error state - show gear to manage credentials + Connect to retry
            HStack(spacing: 8) {
                if let onSettings = onSettings {
                    Button(action: onSettings) {
                        Image(systemName: "gearshape")
                            .font(.system(size: 12))
                            .foregroundColor(CityPopTheme.textMuted)
                    }
                    .buttonStyle(.plain)
                }
                SimpleButton(title: "Connect", style: .primary) { onSync() }
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
        #if canImport(Inject)
        .enableInjection()
        #endif
    }

    #if canImport(Inject)
    @ObserveInjection var forceRedraw
    #endif
    
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
        #if canImport(Inject)
        .enableInjection()
        #endif
    }

    #if canImport(Inject)
    @ObserveInjection var forceRedraw
    #endif
    
    private var iconName: String {
        switch provider {
        case .slack: return "bubble.left.and.bubble.right"  // Conversation/channels
        case .googleWorkspace: return "calendar"             // Calendar is core to GWS
        case .github: return "arrow.triangle.branch"         // Git branching
        case .cursor: return "brain.head.profile"            // AI brain
        }
    }
}

// MARK: - Simple Button

struct SimpleButton: View {
    let title: String
    let style: ButtonStyle
    let action: () -> Void
    
    enum ButtonStyle {
        case primary      // Pink filled - "Connect" 
        case secondary    // Teal filled - "Sync"
        case tertiary     // Outlined - "Stop", "Cancel"
        case destructive  // Red - "Disconnect"
    }
    
    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: 12, weight: .medium))
                .foregroundColor(foregroundColor)
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
                .background(
                    RoundedRectangle(cornerRadius: 6)
                        .fill(backgroundColor)
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 6)
                        .stroke(borderColor, lineWidth: style == .tertiary ? 1 : 0)
                )
        }
        .buttonStyle(.plain)
        #if canImport(Inject)
        .enableInjection()
        #endif
    }

    #if canImport(Inject)
    @ObserveInjection var forceRedraw
    #endif
    
    private var foregroundColor: Color {
        switch style {
        case .primary, .secondary:
            return .white
        case .tertiary:
            return CityPopTheme.textSecondary
        case .destructive:
            return CityPopTheme.error
        }
    }
    
    private var backgroundColor: Color {
        switch style {
        case .primary:
            return CityPopTheme.accent
        case .secondary:
            return CityPopTheme.accentSecondary
        case .tertiary:
            return CityPopTheme.surface
        case .destructive:
            return CityPopTheme.error.opacity(0.1)
        }
    }
    
    private var borderColor: Color {
        switch style {
        case .tertiary:
            return CityPopTheme.border
        default:
            return Color.clear
        }
    }
}

// MARK: - MCP Setup Card

struct MCPSetupCard: View {
    let appName: String
    let configPath: String
    let configContent: String
    
    @State private var isExpanded = false
    @State private var copiedToClipboard = false
    
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            // Header
            HStack {
                VStack(alignment: .leading, spacing: 4) {
                    Text(appName)
                        .font(.system(size: 14, weight: .semibold))
                        .foregroundColor(CityPopTheme.textPrimary)
                    
                    Text(configPath)
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundColor(CityPopTheme.textMuted)
                }
                
                Spacer()
                
                Button(action: { withAnimation { isExpanded.toggle() } }) {
                    Image(systemName: isExpanded ? "chevron.up" : "chevron.down")
                        .font(.system(size: 11, weight: .medium))
                        .foregroundColor(CityPopTheme.textMuted)
                }
                .buttonStyle(.plain)
            }
            .padding(16)
            
            // Expanded content
            if isExpanded {
                VStack(alignment: .leading, spacing: 12) {
                    Text("Add this to your MCP config file:")
                        .font(.system(size: 12))
                        .foregroundColor(CityPopTheme.textSecondary)
                    
                    // Config code block
                    ScrollView(.horizontal, showsIndicators: false) {
                        Text(configContent)
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundColor(CityPopTheme.textPrimary)
                            .padding(12)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(CityPopTheme.surface)
                            .cornerRadius(6)
                            .textSelection(.enabled)
                    }
                    
                    // Copy button
                    Button(action: copyToClipboard) {
                        HStack(spacing: 6) {
                            Image(systemName: copiedToClipboard ? "checkmark" : "doc.on.doc")
                                .font(.system(size: 11))
                            Text(copiedToClipboard ? "Copied!" : "Copy Config")
                                .font(.system(size: 12, weight: .medium))
                        }
                        .foregroundColor(copiedToClipboard ? CityPopTheme.success : CityPopTheme.accent)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 6)
                        .background(CityPopTheme.accent.opacity(0.1))
                        .cornerRadius(6)
                    }
                    .buttonStyle(.plain)
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 16)
            }
        }
        .background(CityPopTheme.surface)
        .cornerRadius(8)
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .stroke(CityPopTheme.border, lineWidth: 1)
        )
        #if canImport(Inject)
        .enableInjection()
        #endif
    }

    #if canImport(Inject)
    @ObserveInjection var forceRedraw
    #endif
    
    private func copyToClipboard() {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(configContent, forType: .string)
        
        copiedToClipboard = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 2.0) {
            copiedToClipboard = false
        }
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
        #if canImport(Inject)
        .enableInjection()
        #endif
    }

    #if canImport(Inject)
    @ObserveInjection var forceRedraw
    #endif
}

// MARK: - Preview

#if DEBUG
struct ControlCenterView_Previews: PreviewProvider {
    static var previews: some View {
        ControlCenterView(engine: MockSignalEngine(state: .sunny) as any SignalProvider)
            .previewDisplayName("Sunny State")
        
        ControlCenterView(engine: MockSignalEngine(state: .indexing) as any SignalProvider)
            .previewDisplayName("Indexing State")
        
        ControlCenterView(engine: MockSignalEngine(state: .welcome) as any SignalProvider)
            .previewDisplayName("Welcome State")
        
        ControlCenterView(engine: MockSignalEngine(state: .error) as any SignalProvider)
            .previewDisplayName("Error State")
    }
}
#endif