import SwiftUI
#if canImport(Inject)
import Inject
#endif

// MARK: - MCP Server Model

/// Represents a discovered MCP server from Claude, Cursor, or Minna config
struct MCPServer: Identifiable, Codable {
    let name: String
    let command: String
    let args: [String]
    let env: [String: String]
    let source: String  // "claude_desktop", "cursor", or "minna"
    var enabled: Bool
    var lastSyncDate: Date?
    var documentCount: Int?
    
    var id: String { name }
    
    var displayName: String {
        name.capitalized
            .replacingOccurrences(of: "_", with: " ")
            .replacingOccurrences(of: "-", with: " ")
    }
    
    var sourceLabel: String {
        switch source {
        case "claude_desktop": return "Claude"
        case "cursor": return "Cursor"
        case "minna": return "Minna"
        default: return source.capitalized
        }
    }
    
    var sourceIcon: String {
        switch source {
        case "claude_desktop": return "message"
        case "cursor": return "brain.head.profile"
        case "minna": return "sparkles"
        default: return "server.rack"
        }
    }
    
    var sourceColor: Color {
        switch source {
        case "claude_desktop": return CityPopTheme.accentCoral
        case "cursor": return CityPopTheme.accentCyan
        case "minna": return CityPopTheme.accent
        default: return CityPopTheme.textSecondary
        }
    }
}

// MARK: - MCP Sources Manager

/// Manages discovery and sync of MCP sources
class MCPSourcesManager: ObservableObject {
    static let shared = MCPSourcesManager()
    
    @Published var discoveredServers: [MCPServer] = []
    @Published var isDiscovering = false
    @Published var lastDiscoveryError: String?
    @Published var syncingServer: String? = nil
    
    private init() {}
    
    /// Run discovery to find MCP servers
    func discover() {
        isDiscovering = true
        lastDiscoveryError = nil
        
        Task {
            do {
                let result = try await runMCPCommand(["mcp", "list"])
                
                // Parse MINNA_RESULT line
                if let resultLine = result.components(separatedBy: "\n")
                    .first(where: { $0.hasPrefix("MINNA_RESULT:") }) {
                    
                    let jsonString = String(resultLine.dropFirst("MINNA_RESULT:".count))
                    if let data = jsonString.data(using: .utf8),
                       let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
                        
                        await MainActor.run {
                            self.parseDiscoveryResult(json)
                            self.isDiscovering = false
                        }
                    }
                } else {
                    await MainActor.run {
                        self.isDiscovering = false
                        self.lastDiscoveryError = "No MCP servers found"
                    }
                }
            } catch {
                await MainActor.run {
                    self.isDiscovering = false
                    self.lastDiscoveryError = error.localizedDescription
                }
            }
        }
    }
    
    private func parseDiscoveryResult(_ json: [String: Any]) {
        guard let servers = json["servers"] as? [[String: Any]] else { return }
        
        discoveredServers = servers.compactMap { serverDict -> MCPServer? in
            guard let name = serverDict["name"] as? String else { return nil }
            return MCPServer(
                name: name,
                command: serverDict["command"] as? String ?? "",
                args: serverDict["args"] as? [String] ?? [],
                env: serverDict["env"] as? [String: String] ?? [:],
                source: serverDict["source"] as? String ?? "minna",
                enabled: serverDict["enabled"] as? Bool ?? true,
                lastSyncDate: nil,
                documentCount: nil
            )
        }
    }
    
    /// Sync a specific MCP server
    func syncServer(_ serverName: String) {
        syncingServer = serverName
        
        Task {
            do {
                _ = try await runMCPCommand(["mcp", "sync", serverName])
                await MainActor.run {
                    self.syncingServer = nil
                    // Update last sync date
                    if let index = self.discoveredServers.firstIndex(where: { $0.name == serverName }) {
                        self.discoveredServers[index].lastSyncDate = Date()
                    }
                }
            } catch {
                await MainActor.run {
                    self.syncingServer = nil
                    self.lastDiscoveryError = "Sync failed: \(error.localizedDescription)"
                }
            }
        }
    }
    
    /// Sync all MCP servers
    func syncAll() {
        syncingServer = "all"
        
        Task {
            do {
                _ = try await runMCPCommand(["mcp", "sync"])
                await MainActor.run {
                    self.syncingServer = nil
                }
            } catch {
                await MainActor.run {
                    self.syncingServer = nil
                    self.lastDiscoveryError = "Sync failed: \(error.localizedDescription)"
                }
            }
        }
    }
    
    /// Remove a server (only works for Minna-configured servers)
    func removeServer(_ serverName: String) {
        // TODO: Call Python backend to remove from ~/.minna/mcp_sources.json
        discoveredServers.removeAll { $0.name == serverName }
    }
    
    private func runMCPCommand(_ args: [String]) async throws -> String {
        let process = Process()
        let pipe = Pipe()
        
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = ["python", "-m", "minna.cli"] + args
        process.standardOutput = pipe
        process.standardError = pipe
        
        if let srcPath = Bundle.main.resourcePath {
            process.currentDirectoryURL = URL(fileURLWithPath: srcPath).deletingLastPathComponent()
        }
        
        try process.run()
        process.waitUntilExit()
        
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        return String(data: data, encoding: .utf8) ?? ""
    }
}

// MARK: - Section Header

struct SourcesSectionHeader: View {
    let title: String
    let subtitle: String?
    let action: (() -> Void)?
    let actionLabel: String?
    
    init(title: String, subtitle: String? = nil, action: (() -> Void)? = nil, actionLabel: String? = nil) {
        self.title = title
        self.subtitle = subtitle
        self.action = action
        self.actionLabel = actionLabel
    }
    
    var body: some View {
        HStack(alignment: .bottom) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundColor(CityPopTheme.textMuted)
                    .textCase(.uppercase)
                
                if let subtitle = subtitle {
                    Text(subtitle)
                        .font(.system(size: 11))
                        .foregroundColor(CityPopTheme.textMuted.opacity(0.7))
                }
            }
            
            Spacer()
            
            if let action = action, let label = actionLabel {
                Button(action: action) {
                    Text(label)
                        .font(.system(size: 11, weight: .medium))
                        .foregroundColor(CityPopTheme.accent)
                }
                .buttonStyle(.plain)
            }
        }
        #if canImport(Inject)
        .enableInjection()
        #endif
    }

    #if canImport(Inject)
    @ObserveInjection var forceRedraw
    #endif
}

// MARK: - Discovered Sources Section (Redesigned)

struct DiscoveredSourcesSection: View {
    @ObservedObject var mcpManager = MCPSourcesManager.shared
    @Binding var showingSetupWizard: Bool
    @State private var selectedServer: MCPServer? = nil
    
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            // Header with explanation and Add button
            mcpHeader
            
            // Content: Table or Empty State
            if mcpManager.isDiscovering {
                loadingState
            } else if mcpManager.discoveredServers.isEmpty {
                emptyState
            } else {
                serverTable
            }
        }
        .sheet(item: $selectedServer) { server in
            MCPServerDetailSheet(
                server: server,
                onSync: { mcpManager.syncServer(server.name) },
                onRemove: {
                    mcpManager.removeServer(server.name)
                    selectedServer = nil
                },
                onClose: { selectedServer = nil }
            )
        }
        #if canImport(Inject)
        .enableInjection()
        #endif
    }

    #if canImport(Inject)
    @ObserveInjection var forceRedraw
    #endif
    
    // MARK: - Header
    
    private var mcpHeader: some View {
        HStack {
            Text("MCP Sources")
                .font(.system(size: 12, weight: .semibold))
                .foregroundColor(CityPopTheme.textMuted)
                .textCase(.uppercase)
            
            Spacer()
            
            // Add Source button
            Button(action: { showingSetupWizard = true }) {
                HStack(spacing: 5) {
                    Image(systemName: "plus")
                        .font(.system(size: 11, weight: .medium))
                    Text("Add Source")
                        .font(.system(size: 12, weight: .medium))
                }
                .foregroundColor(.white)
                .padding(.horizontal, 12)
                .padding(.vertical, 7)
                .background(CityPopTheme.accent)
                .cornerRadius(6)
            }
            .buttonStyle(.plain)
        }
    }
    
    // MARK: - Loading State
    
    private var loadingState: some View {
        HStack(spacing: 12) {
            ProgressView()
                .scaleEffect(0.8)
            Text("Scanning for MCP servers...")
                .font(.system(size: 13))
                .foregroundColor(CityPopTheme.textSecondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 32)
    }
    
    // MARK: - Empty State
    
    private var emptyState: some View {
        VStack(spacing: 20) {
            Image(systemName: "antenna.radiowaves.left.and.right")
                .font(.system(size: 32, weight: .light))
                .foregroundColor(CityPopTheme.textMuted)
            
            VStack(spacing: 8) {
                Text("No MCP Sources Detected")
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundColor(CityPopTheme.textPrimary)
                
                Text("If you have MCP servers configured in Claude Desktop or Cursor, Minna will automatically discover them. You can also add sources directly using the button above.")
                    .font(.system(size: 13))
                    .foregroundColor(CityPopTheme.textSecondary)
                    .multilineTextAlignment(.center)
                    .lineSpacing(2)
                    .padding(.horizontal, 16)
            }
            
            Button(action: { mcpManager.discover() }) {
                HStack(spacing: 6) {
                    Image(systemName: "arrow.clockwise")
                        .font(.system(size: 11))
                    Text("Scan Again")
                        .font(.system(size: 12, weight: .medium))
                }
                .foregroundColor(CityPopTheme.accent)
            }
            .buttonStyle(.plain)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 40)
        .padding(.horizontal, 24)
        .background(CityPopTheme.surface)
        .cornerRadius(8)
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .stroke(CityPopTheme.border, lineWidth: 1)
        )
    }
    
    // MARK: - Server Table
    
    private var serverTable: some View {
        VStack(spacing: 0) {
            // Table header
            HStack(spacing: 0) {
                Text("Source")
                    .frame(width: 180, alignment: .leading)
                Text("Origin")
                    .frame(width: 80, alignment: .leading)
                Text("Status")
                    .frame(width: 100, alignment: .leading)
                Spacer()
                Text("Last Sync")
                    .frame(width: 100, alignment: .trailing)
            }
            .font(.system(size: 11, weight: .medium))
            .foregroundColor(CityPopTheme.textMuted)
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .background(CityPopTheme.background)
            
            Divider()
            
            // Table rows
            ForEach(mcpManager.discoveredServers) { server in
                MCPServerTableRow(
                    server: server,
                    isSyncing: mcpManager.syncingServer == server.name || mcpManager.syncingServer == "all"
                )
                .contentShape(Rectangle())
                .onTapGesture {
                    selectedServer = server
                }
                
                if server.id != mcpManager.discoveredServers.last?.id {
                    Divider()
                }
            }
            
            // Table footer with Sync All
            if mcpManager.discoveredServers.count > 1 {
                Divider()
                HStack {
                    Button(action: { mcpManager.discover() }) {
                        HStack(spacing: 4) {
                            Image(systemName: "arrow.clockwise")
                                .font(.system(size: 10))
                            Text("Refresh")
                                .font(.system(size: 11, weight: .medium))
                        }
                        .foregroundColor(CityPopTheme.textMuted)
                    }
                    .buttonStyle(.plain)
                    
                    Spacer()
                    
                    Button(action: { mcpManager.syncAll() }) {
                        HStack(spacing: 4) {
                            Image(systemName: "arrow.triangle.2.circlepath")
                                .font(.system(size: 10))
                            Text("Sync All")
                                .font(.system(size: 11, weight: .medium))
                        }
                        .foregroundColor(CityPopTheme.accentSecondary)
                    }
                    .buttonStyle(.plain)
                    .disabled(mcpManager.syncingServer != nil)
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 10)
                .background(CityPopTheme.background)
            }
        }
        .background(CityPopTheme.surface)
        .cornerRadius(8)
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .stroke(CityPopTheme.border, lineWidth: 1)
        )
    }
}

// MARK: - Table Row

struct MCPServerTableRow: View {
    let server: MCPServer
    let isSyncing: Bool
    
    @State private var isHovered = false
    
    var body: some View {
        HStack(spacing: 0) {
            // Source name with icon
            HStack(spacing: 10) {
                Image(systemName: server.sourceIcon)
                    .font(.system(size: 14, weight: .light))
                    .foregroundColor(server.sourceColor)
                    .frame(width: 20)
                
                Text(server.displayName)
                    .font(.system(size: 13, weight: .medium))
                    .foregroundColor(CityPopTheme.textPrimary)
            }
            .frame(width: 180, alignment: .leading)
            
            // Origin badge
            Text(server.sourceLabel)
                .font(.system(size: 10, weight: .medium))
                .foregroundColor(server.sourceColor)
                .padding(.horizontal, 6)
                .padding(.vertical, 2)
                .background(server.sourceColor.opacity(0.1))
                .cornerRadius(4)
                .frame(width: 80, alignment: .leading)
            
            // Status
            HStack(spacing: 6) {
                if isSyncing {
                    ProgressView()
                        .scaleEffect(0.5)
                    Text("Syncing...")
                        .font(.system(size: 12))
                        .foregroundColor(CityPopTheme.syncing)
                } else if server.enabled {
                    Circle()
                        .fill(CityPopTheme.success)
                        .frame(width: 6, height: 6)
                    Text("Ready")
                        .font(.system(size: 12))
                        .foregroundColor(CityPopTheme.textSecondary)
                } else {
                    Circle()
                        .fill(CityPopTheme.textMuted)
                        .frame(width: 6, height: 6)
                    Text("Disabled")
                        .font(.system(size: 12))
                        .foregroundColor(CityPopTheme.textMuted)
                }
            }
            .frame(width: 100, alignment: .leading)
            
            Spacer()
            
            // Last sync
            Text(lastSyncText)
                .font(.system(size: 12))
                .foregroundColor(CityPopTheme.textMuted)
                .frame(width: 100, alignment: .trailing)
            
            // Chevron
            Image(systemName: "chevron.right")
                .font(.system(size: 10, weight: .medium))
                .foregroundColor(CityPopTheme.textMuted.opacity(isHovered ? 1 : 0.5))
                .padding(.leading, 12)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .background(isHovered ? CityPopTheme.background : Color.clear)
        .onHover { isHovered = $0 }
        #if canImport(Inject)
        .enableInjection()
        #endif
    }

    #if canImport(Inject)
    @ObserveInjection var forceRedraw
    #endif
    
    private var lastSyncText: String {
        guard let date = server.lastSyncDate else { return "Never" }
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .abbreviated
        return formatter.localizedString(for: date, relativeTo: Date())
    }
}

// MARK: - Server Detail Sheet

struct MCPServerDetailSheet: View {
    let server: MCPServer
    let onSync: () -> Void
    let onRemove: () -> Void
    let onClose: () -> Void
    
    @ObservedObject var mcpManager = MCPSourcesManager.shared
    
    var body: some View {
        VStack(spacing: 0) {
            // Header
            HStack {
                HStack(spacing: 12) {
                    ZStack {
                        RoundedRectangle(cornerRadius: 10)
                            .fill(server.sourceColor.opacity(0.1))
                            .frame(width: 44, height: 44)
                        
                        Image(systemName: server.sourceIcon)
                            .font(.system(size: 20, weight: .light))
                            .foregroundColor(server.sourceColor)
                    }
                    
                    VStack(alignment: .leading, spacing: 2) {
                        Text(server.displayName)
                            .font(.system(size: 16, weight: .semibold))
                            .foregroundColor(CityPopTheme.textPrimary)
                        
                        HStack(spacing: 6) {
                            Text("MCP Server")
                                .font(.system(size: 12))
                                .foregroundColor(CityPopTheme.textSecondary)
                            
                            Text("•")
                                .foregroundColor(CityPopTheme.textMuted)
                            
                            Text("from \(server.sourceLabel)")
                                .font(.system(size: 12))
                                .foregroundColor(server.sourceColor)
                        }
                    }
                }
                
                Spacer()
                
                Button(action: onClose) {
                    Image(systemName: "xmark")
                        .font(.system(size: 12, weight: .medium))
                        .foregroundColor(CityPopTheme.textMuted)
                        .padding(8)
                        .background(CityPopTheme.surface)
                        .cornerRadius(6)
                }
                .buttonStyle(.plain)
            }
            .padding(20)
            
            Divider()
            
            // Content
            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    // Status card
                    statusCard
                    
                    // Sync history (placeholder)
                    syncHistorySection
                    
                    // Connection info
                    connectionInfoSection
                    
                    // Actions
                    actionsSection
                }
                .padding(20)
            }
        }
        .frame(width: 480, height: 520)
        .background(CityPopTheme.background)
        #if canImport(Inject)
        .enableInjection()
        #endif
    }

    #if canImport(Inject)
    @ObserveInjection var forceRedraw
    #endif
    
    // MARK: - Status Card
    
    private var statusCard: some View {
        HStack(spacing: 16) {
            // Status
            VStack(alignment: .leading, spacing: 4) {
                Text("Status")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundColor(CityPopTheme.textMuted)
                
                HStack(spacing: 6) {
                    Circle()
                        .fill(server.enabled ? CityPopTheme.success : CityPopTheme.textMuted)
                        .frame(width: 8, height: 8)
                    
                    Text(server.enabled ? "Active" : "Disabled")
                        .font(.system(size: 14, weight: .medium))
                        .foregroundColor(CityPopTheme.textPrimary)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            
            Divider()
                .frame(height: 40)
            
            // Last Sync
            VStack(alignment: .leading, spacing: 4) {
                Text("Last Sync")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundColor(CityPopTheme.textMuted)
                
                Text(server.lastSyncDate != nil ? formatDate(server.lastSyncDate!) : "Never synced")
                    .font(.system(size: 14, weight: .medium))
                    .foregroundColor(CityPopTheme.textPrimary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            
            Divider()
                .frame(height: 40)
            
            // Documents
            VStack(alignment: .leading, spacing: 4) {
                Text("Documents")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundColor(CityPopTheme.textMuted)
                
                Text(server.documentCount != nil ? "\(server.documentCount!)" : "—")
                    .font(.system(size: 14, weight: .medium))
                    .foregroundColor(CityPopTheme.textPrimary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(16)
        .background(CityPopTheme.surface)
        .cornerRadius(8)
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .stroke(CityPopTheme.border, lineWidth: 1)
        )
    }
    
    // MARK: - Sync History
    
    private var syncHistorySection: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Sync History")
                .font(.system(size: 12, weight: .semibold))
                .foregroundColor(CityPopTheme.textMuted)
                .textCase(.uppercase)
            
            VStack(spacing: 0) {
                // Placeholder history items
                ForEach(0..<3, id: \.self) { i in
                    HStack(spacing: 12) {
                        Circle()
                            .fill(i == 0 ? CityPopTheme.success : CityPopTheme.textMuted.opacity(0.3))
                            .frame(width: 6, height: 6)
                        
                        Text(i == 0 ? "Sync completed" : "Previous sync")
                            .font(.system(size: 13))
                            .foregroundColor(CityPopTheme.textSecondary)
                        
                        Spacer()
                        
                        Text(i == 0 ? "Just now" : "\(i * 2) days ago")
                            .font(.system(size: 12))
                            .foregroundColor(CityPopTheme.textMuted)
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                    
                    if i < 2 {
                        Divider()
                    }
                }
            }
            .background(CityPopTheme.surface)
            .cornerRadius(8)
            .overlay(
                RoundedRectangle(cornerRadius: 8)
                    .stroke(CityPopTheme.border, lineWidth: 1)
            )
        }
    }
    
    // MARK: - Connection Info
    
    private var connectionInfoSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Connection")
                .font(.system(size: 12, weight: .semibold))
                .foregroundColor(CityPopTheme.textMuted)
                .textCase(.uppercase)
            
            VStack(spacing: 0) {
                infoRow(label: "Server Type", value: "MCP Protocol")
                Divider()
                infoRow(label: "Command", value: server.command)
                Divider()
                infoRow(label: "Configured In", value: server.sourceLabel)
            }
            .background(CityPopTheme.surface)
            .cornerRadius(8)
            .overlay(
                RoundedRectangle(cornerRadius: 8)
                    .stroke(CityPopTheme.border, lineWidth: 1)
            )
        }
    }
    
    private func infoRow(label: String, value: String) -> some View {
        HStack {
            Text(label)
                .font(.system(size: 13))
                .foregroundColor(CityPopTheme.textSecondary)
            Spacer()
            Text(value)
                .font(.system(size: 13, design: .monospaced))
                .foregroundColor(CityPopTheme.textMuted)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
    }
    
    // MARK: - Actions
    
    private var actionsSection: some View {
        HStack(spacing: 12) {
            // Sync button
            Button(action: onSync) {
                HStack(spacing: 6) {
                    if mcpManager.syncingServer == server.name {
                        ProgressView()
                            .scaleEffect(0.7)
                    } else {
                        Image(systemName: "arrow.triangle.2.circlepath")
                            .font(.system(size: 12))
                    }
                    Text(mcpManager.syncingServer == server.name ? "Syncing..." : "Sync Now")
                        .font(.system(size: 13, weight: .medium))
                }
                .foregroundColor(.white)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 10)
                .background(CityPopTheme.accentSecondary)
                .cornerRadius(6)
            }
            .buttonStyle(.plain)
            .disabled(mcpManager.syncingServer != nil)
            
            // Remove button (only for Minna sources)
            if server.source == "minna" {
                Button(action: onRemove) {
                    HStack(spacing: 6) {
                        Image(systemName: "trash")
                            .font(.system(size: 12))
                        Text("Remove")
                            .font(.system(size: 13, weight: .medium))
                    }
                    .foregroundColor(CityPopTheme.error)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 10)
                    .background(CityPopTheme.error.opacity(0.1))
                    .cornerRadius(6)
                }
                .buttonStyle(.plain)
            }
        }
    }
    
    private func formatDate(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.dateStyle = .medium
        formatter.timeStyle = .short
        return formatter.string(from: date)
    }
}

// MARK: - Manual Config Fallback Card

struct ManualConfigFallbackCard: View {
    let onGoToSettings: () -> Void
    
    var body: some View {
        HStack(spacing: 14) {
            ZStack {
                RoundedRectangle(cornerRadius: 8)
                    .fill(CityPopTheme.textMuted.opacity(0.08))
                    .frame(width: 40, height: 40)
                
                Image(systemName: "key")
                    .font(.system(size: 16, weight: .light))
                    .foregroundColor(CityPopTheme.textMuted)
            }
            
            VStack(alignment: .leading, spacing: 2) {
                Text("Prefer manual setup?")
                    .font(.system(size: 13, weight: .medium))
                    .foregroundColor(CityPopTheme.textPrimary)
                
                Text("Configure sources with API keys in Settings")
                    .font(.system(size: 12))
                    .foregroundColor(CityPopTheme.textSecondary)
            }
            
            Spacer()
            
            Button(action: onGoToSettings) {
                HStack(spacing: 4) {
                    Text("Settings")
                        .font(.system(size: 12, weight: .medium))
                    Image(systemName: "arrow.right")
                        .font(.system(size: 10, weight: .medium))
                }
                .foregroundColor(CityPopTheme.accent)
            }
            .buttonStyle(.plain)
        }
        .padding(16)
        .background(CityPopTheme.surface.opacity(0.5))
        .cornerRadius(8)
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .stroke(CityPopTheme.border.opacity(0.5), lineWidth: 1)
        )
        #if canImport(Inject)
        .enableInjection()
        #endif
    }

    #if canImport(Inject)
    @ObserveInjection var forceRedraw
    #endif
}

// MARK: - Preview

struct MCPSourcesView_Previews: PreviewProvider {
    static var previews: some View {
        VStack(spacing: 20) {
            DiscoveredSourcesSection(
                showingSetupWizard: .constant(false)
            )
        }
        .padding()
        .background(CityPopTheme.background)
    }
}
