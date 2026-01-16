import SwiftUI
#if canImport(Inject)
import Inject
#endif

// MARK: - MCP Server Catalog

struct MCPServerTemplate: Identifiable {
    let id: String
    let name: String
    let description: String
    let icon: String
    let npmPackage: String
    let envVars: [EnvVarTemplate]
    let docsUrl: String
    
    struct EnvVarTemplate {
        let name: String
        let label: String
        let placeholder: String
        let isSecret: Bool
    }
}

/// Category grouping for MCP servers with City Pop colors
struct MCPCategory: Identifiable {
    let id: String
    let name: String
    let color: Color
    let servers: [MCPServerTemplate]
}

/// City Pop color palette for categories
struct MCPCategoryColors {
    static let projectManagement = Color(red: 0.98, green: 0.32, blue: 0.5)   // Hot pink
    static let documentation = Color(red: 0.58, green: 0.38, blue: 0.82)      // Purple
    static let design = Color(red: 1.0, green: 0.55, blue: 0.4)               // Coral
    static let data = Color(red: 0.0, green: 0.78, blue: 0.82)                // Cyan
    static let communication = Color(red: 0.92, green: 0.62, blue: 0.15)      // Gold
    static let development = Color(red: 0.25, green: 0.48, blue: 0.85)        // Blue
    static let cloud = Color(red: 0.18, green: 0.70, blue: 0.58)              // Teal
    static let content = Color(red: 0.85, green: 0.45, blue: 0.65)            // Rose
}

/// Catalog of MCP servers organized by category
let mcpCategories: [MCPCategory] = [
    MCPCategory(
        id: "project-management",
        name: "Project Management",
        color: MCPCategoryColors.projectManagement,
        servers: [
            MCPServerTemplate(
                id: "atlassian",
                name: "Atlassian",
                description: "Jira, Confluence, and more via Rovo",
                icon: "a.square",
                npmPackage: "@anthropic/remote-mcp-server-atlassian",
                envVars: [],  // Uses OAuth via Rovo
                docsUrl: "https://atlassian.com/platform/remote-mcp-server"
            ),
            MCPServerTemplate(
                id: "linear",
                name: "Linear",
                description: "Issues, projects, and roadmaps",
                icon: "chart.bar.doc.horizontal",
                npmPackage: "mcp-server-linear",
                envVars: [
                    .init(name: "LINEAR_API_KEY", label: "API Key", placeholder: "lin_api_...", isSecret: true)
                ],
                docsUrl: "https://linear.app/settings/api"
            )
        ]
    ),
    
    MCPCategory(
        id: "documentation",
        name: "Documentation & Knowledge",
        color: MCPCategoryColors.documentation,
        servers: [
            MCPServerTemplate(
                id: "notion",
                name: "Notion",
                description: "Pages, databases, and wikis",
                icon: "doc.text",
                npmPackage: "mcp-server-notion",
                envVars: [
                    .init(name: "NOTION_API_KEY", label: "Integration Token", placeholder: "secret_...", isSecret: true)
                ],
                docsUrl: "https://www.notion.so/my-integrations"
            ),
            MCPServerTemplate(
                id: "obsidian",
                name: "Obsidian",
                description: "Markdown notes and knowledge base",
                icon: "brain.head.profile",
                npmPackage: "mcp-obsidian",
                envVars: [
                    .init(name: "OBSIDIAN_VAULT_PATH", label: "Vault Path", placeholder: "/path/to/vault", isSecret: false)
                ],
                docsUrl: "https://obsidian.md/"
            )
        ]
    ),
    
    MCPCategory(
        id: "communication",
        name: "Communication",
        color: MCPCategoryColors.communication,
        servers: [
            MCPServerTemplate(
                id: "slack",
                name: "Slack",
                description: "Channels, messages, and threads",
                icon: "number.square",
                npmPackage: "@modelcontextprotocol/server-slack",
                envVars: [
                    .init(name: "SLACK_BOT_TOKEN", label: "Bot Token", placeholder: "xoxb-...", isSecret: true),
                    .init(name: "SLACK_TEAM_ID", label: "Team ID (optional)", placeholder: "T0123456789", isSecret: false)
                ],
                docsUrl: "https://github.com/modelcontextprotocol/servers/tree/main/src/slack"
            )
        ]
    ),
    
    MCPCategory(
        id: "development",
        name: "Development",
        color: MCPCategoryColors.development,
        servers: [
            MCPServerTemplate(
                id: "github",
                name: "GitHub",
                description: "Repos, issues, and PRs",
                icon: "arrow.triangle.branch",
                npmPackage: "@modelcontextprotocol/server-github",
                envVars: [
                    .init(name: "GITHUB_PERSONAL_ACCESS_TOKEN", label: "Personal Access Token", placeholder: "ghp_...", isSecret: true)
                ],
                docsUrl: "https://github.com/settings/tokens"
            ),
            MCPServerTemplate(
                id: "gitlab",
                name: "GitLab",
                description: "Projects, issues, and MRs",
                icon: "arrow.triangle.branch",
                npmPackage: "@modelcontextprotocol/server-gitlab",
                envVars: [
                    .init(name: "GITLAB_PERSONAL_ACCESS_TOKEN", label: "Personal Access Token", placeholder: "glpat-...", isSecret: true),
                    .init(name: "GITLAB_API_URL", label: "GitLab URL (optional)", placeholder: "https://gitlab.com", isSecret: false)
                ],
                docsUrl: "https://gitlab.com/-/user_settings/personal_access_tokens"
            ),
            MCPServerTemplate(
                id: "sentry",
                name: "Sentry",
                description: "Error tracking and monitoring",
                icon: "exclamationmark.triangle",
                npmPackage: "@modelcontextprotocol/server-sentry",
                envVars: [
                    .init(name: "SENTRY_AUTH_TOKEN", label: "Auth Token", placeholder: "sntrys_...", isSecret: true),
                    .init(name: "SENTRY_ORGANIZATION_SLUG", label: "Organization Slug", placeholder: "my-org", isSecret: false)
                ],
                docsUrl: "https://sentry.io/settings/account/api/auth-tokens/"
            )
        ]
    ),
    
    MCPCategory(
        id: "content",
        name: "Files & Storage",
        color: MCPCategoryColors.content,
        servers: [
            MCPServerTemplate(
                id: "google-drive",
                name: "Google Drive",
                description: "Cloud files and folders",
                icon: "icloud",
                npmPackage: "@modelcontextprotocol/server-gdrive",
                envVars: [],  // Uses OAuth
                docsUrl: "https://github.com/modelcontextprotocol/servers/tree/main/src/gdrive"
            ),
            MCPServerTemplate(
                id: "dropbox",
                name: "Dropbox",
                description: "Files and sharing",
                icon: "shippingbox",
                npmPackage: "mcp-server-dropbox",
                envVars: [
                    .init(name: "DROPBOX_ACCESS_TOKEN", label: "Access Token", placeholder: "sl...", isSecret: true)
                ],
                docsUrl: "https://www.dropbox.com/developers/apps"
            ),
            MCPServerTemplate(
                id: "box",
                name: "Box",
                description: "Enterprise content management",
                icon: "archivebox",
                npmPackage: "mcp-server-box",
                envVars: [
                    .init(name: "BOX_CLIENT_ID", label: "Client ID", placeholder: "...", isSecret: false),
                    .init(name: "BOX_CLIENT_SECRET", label: "Client Secret", placeholder: "...", isSecret: true)
                ],
                docsUrl: "https://developer.box.com/guides/authentication/"
            )
        ]
    )
]

/// Special template for custom MCP servers
let customServerTemplate = MCPServerTemplate(
    id: "custom",
    name: "Custom Server",
    description: "Configure any MCP server from the registry",
    icon: "terminal",
    npmPackage: "",
    envVars: [],
    docsUrl: "https://github.com/modelcontextprotocol/servers"
)

/// Flat list of all servers for backward compatibility
let mcpServerCatalog: [MCPServerTemplate] = mcpCategories.flatMap { $0.servers }

/// URL to browse the full MCP server registry
let mcpRegistryURL = "https://github.com/modelcontextprotocol/servers"

// MARK: - Parsed MCP Config

/// Represents a parsed MCP server configuration from pasted JSON
struct ParsedMCPConfig {
    let serverName: String
    let command: String
    let args: [String]
    var envVars: [ParsedEnvVar]
    
    struct ParsedEnvVar: Identifiable {
        let id = UUID()
        let key: String
        var value: String
        let needsInput: Bool
        
        /// Convert env var key to human-readable label
        var humanReadableLabel: String {
            key
                .replacingOccurrences(of: "_", with: " ")
                .split(separator: " ")
                .map { word in
                    let lower = word.lowercased()
                    // Keep common acronyms uppercase
                    if ["api", "url", "id", "oauth", "jwt", "ssh"].contains(lower) {
                        return word.uppercased()
                    }
                    return word.capitalized
                }
                .joined(separator: " ")
        }
    }
    
    /// Extract server name from package/args
    static func extractServerName(from args: [String]) -> String {
        // Try to find package name in args
        for arg in args {
            if arg.hasPrefix("@") || arg.contains("mcp") || arg.contains("server") {
                // Extract name from package like "@modelcontextprotocol/server-github" or "mcp-server-github"
                let name = arg
                    .components(separatedBy: "/").last ?? arg
                let cleaned = name
                    .replacingOccurrences(of: "server-", with: "")
                    .replacingOccurrences(of: "mcp-", with: "")
                    .replacingOccurrences(of: "-mcp", with: "")
                return cleaned.isEmpty ? "custom" : cleaned
            }
        }
        return "custom"
    }
}

/// Parse MCP JSON config string into structured format
func parseMCPConfig(_ jsonString: String) -> ParsedMCPConfig? {
    guard let data = jsonString.data(using: .utf8),
          let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
        return nil
    }
    
    // Extract command
    let command = json["command"] as? String ?? "npx"
    
    // Extract args
    let args = json["args"] as? [String] ?? []
    
    // Extract env vars
    var envVars: [ParsedMCPConfig.ParsedEnvVar] = []
    if let env = json["env"] as? [String: String] {
        for (key, value) in env.sorted(by: { $0.key < $1.key }) {
            let needsInput = isPlaceholderValue(value)
            envVars.append(ParsedMCPConfig.ParsedEnvVar(
                key: key,
                value: needsInput ? "" : value,
                needsInput: needsInput
            ))
        }
    }
    
    let serverName = ParsedMCPConfig.extractServerName(from: args)
    
    return ParsedMCPConfig(
        serverName: serverName,
        command: command,
        args: args,
        envVars: envVars
    )
}

/// Check if a value is a placeholder that needs user input
private func isPlaceholderValue(_ value: String) -> Bool {
    let trimmed = value.trimmingCharacters(in: .whitespaces)
    
    // Empty or whitespace only
    if trimmed.isEmpty { return true }
    
    // Contains angle brackets (e.g., "<your-token>")
    if trimmed.contains("<") && trimmed.contains(">") { return true }
    
    // Common placeholder patterns
    let placeholderPatterns = [
        "YOUR_", "your-", "your_",
        "xxx", "XXX", "...",
        "REPLACE", "replace",
        "INSERT", "insert",
        "TOKEN_HERE", "KEY_HERE"
    ]
    for pattern in placeholderPatterns {
        if trimmed.contains(pattern) { return true }
    }
    
    return false
}

// MARK: - Preflight Check

/// Result of checking if a command runtime is available
struct PreflightResult {
    let isAvailable: Bool
    let command: String
    let installInstructions: String?
}

/// Check if the required command/runtime is installed on the system
func preflightCheck(command: String) -> PreflightResult {
    // Extract the base command (first word, handles paths like /usr/bin/docker)
    let baseCommand = command.components(separatedBy: "/").last ?? command
    
    // Run `which` to check if command exists
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/which")
    process.arguments = [baseCommand]
    
    let pipe = Pipe()
    process.standardOutput = pipe
    process.standardError = pipe
    
    do {
        try process.run()
        process.waitUntilExit()
        
        if process.terminationStatus == 0 {
            return PreflightResult(isAvailable: true, command: baseCommand, installInstructions: nil)
        }
    } catch {
        // If we can't run `which`, assume it's not available
    }
    
    // Command not found - provide install instructions
    let instructions = installInstructions(for: baseCommand)
    return PreflightResult(isAvailable: false, command: baseCommand, installInstructions: instructions)
}

/// Get installation instructions for common MCP server runtimes
private func installInstructions(for command: String) -> String {
    switch command.lowercased() {
    case "npx", "npm", "node":
        return "Install Node.js from nodejs.org or run: brew install node"
    case "uvx", "uv":
        return "Install uv from astral.sh or run: brew install uv"
    case "docker":
        return "Install Docker Desktop from docker.com"
    case "python", "python3":
        return "Install Python from python.org or run: brew install python"
    case "pip", "pip3":
        return "Install Python from python.org (pip is included)"
    case "go":
        return "Install Go from go.dev or run: brew install go"
    case "cargo", "rustc":
        return "Install Rust from rustup.rs"
    case "deno":
        return "Install Deno from deno.land or run: brew install deno"
    case "bun":
        return "Install Bun from bun.sh or run: brew install oven-sh/bun/bun"
    default:
        return "Install \(command) to use this MCP server"
    }
}

// MARK: - Setup Wizard

struct MCPSetupWizard: View {
    let onComplete: () -> Void
    let onCancel: () -> Void
    
    @State private var step: WizardStep = .selectServer
    @State private var selectedTemplate: MCPServerTemplate?
    @State private var envValues: [String: String] = [:]
    @State private var isInstalling = false
    @State private var installError: String?
    @State private var searchText = ""
    
    // Custom server state (paste config flow)
    @State private var pastedConfig = ""
    @State private var parsedConfig: ParsedMCPConfig?
    @State private var parseError: String?
    @State private var missingRuntime: PreflightResult?
    @State private var customEnvValues: [String: String] = [:]
    
    enum WizardStep {
        case selectServer
        case enterCredentials
        case customServer        // Paste config
        case customServerSecrets // Fill in secrets
        case installing
        case complete
    }
    
    var body: some View {
        VStack(spacing: 0) {
            // Header
            wizardHeader
            
            Divider()
            
            // Content
            switch step {
            case .selectServer:
                serverSelectionView
            case .enterCredentials:
                if let template = selectedTemplate {
                    credentialsView(for: template)
                }
            case .customServer:
                customServerPasteView
            case .customServerSecrets:
                if let config = parsedConfig {
                    customServerSecretsView(for: config)
                }
            case .installing:
                installingView
            case .complete:
                completeView
            }
        }
        .frame(width: 520, height: 480)
        .background(CityPopTheme.background)
        #if canImport(Inject)
        .enableInjection()
        #endif
    }

    #if canImport(Inject)
    @ObserveInjection var forceRedraw
    #endif
    
    // MARK: - Header
    
    private var wizardHeader: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text("Set Up MCP Source")
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundColor(CityPopTheme.textPrimary)
                
                Text(stepSubtitle)
                    .font(.system(size: 12))
                    .foregroundColor(CityPopTheme.textSecondary)
            }
            
            Spacer()
            
            // Step indicator
            HStack(spacing: 4) {
                ForEach(0..<4) { i in
                    Circle()
                        .fill(i <= stepIndex ? CityPopTheme.accent : CityPopTheme.border)
                        .frame(width: 6, height: 6)
                }
            }
            
            Button(action: onCancel) {
                Image(systemName: "xmark")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundColor(CityPopTheme.textMuted)
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 16)
    }
    
    private var stepSubtitle: String {
        switch step {
        case .selectServer: return "Choose a service to connect"
        case .enterCredentials: return "Enter your API credentials"
        case .customServer: return "Paste your server config"
        case .customServerSecrets: return "Enter your credentials"
        case .installing: return "Setting up the connection..."
        case .complete: return "Ready to sync!"
        }
    }
    
    private var stepIndex: Int {
        switch step {
        case .selectServer: return 0
        case .enterCredentials, .customServer: return 1
        case .customServerSecrets, .installing: return 2
        case .complete: return 3
        }
    }
    
    private var serverDisplayName: String {
        if let template = selectedTemplate {
            return template.name
        }
        if let config = parsedConfig {
            return config.serverName.capitalized
        }
        return "Server"
    }
    
    private var hasConfigError: Bool {
        parseError != nil || missingRuntime != nil
    }
    
    // MARK: - Server Selection
    
    private var serverSelectionView: some View {
        VStack(spacing: 0) {
            // Search
            HStack(spacing: 10) {
                Image(systemName: "magnifyingglass")
                    .font(.system(size: 13))
                    .foregroundColor(CityPopTheme.textMuted)
                
                TextField("Search services...", text: $searchText)
                    .textFieldStyle(.plain)
                    .font(.system(size: 13))
            }
            .padding(10)
            .background(CityPopTheme.surface)
            .cornerRadius(8)
            .overlay(
                RoundedRectangle(cornerRadius: 8)
                    .stroke(CityPopTheme.border, lineWidth: 1)
            )
            .padding(.horizontal, 20)
            .padding(.top, 16)
            
            // Server grid by category
            ScrollView {
                VStack(spacing: 20) {
                    ForEach(filteredCategories) { category in
                        CategorySection(category: category) { template in
                            selectedTemplate = template
                            envValues = [:]
                            step = .enterCredentials
                        }
                    }
                    
                    // Custom Server + Registry
                    Divider()
                        .padding(.top, 4)
                    
                    // Custom server button
                    Button {
                        pastedConfig = ""
                        parsedConfig = nil
                        parseError = nil
                        missingRuntime = nil
                        customEnvValues = [:]
                        step = .customServer
                    } label: {
                        HStack(spacing: 10) {
                            ZStack {
                                RoundedRectangle(cornerRadius: 8)
                                    .fill(CityPopTheme.textMuted.opacity(0.1))
                                    .frame(width: 34, height: 34)
                                
                                Image(systemName: "terminal")
                                    .font(.system(size: 15, weight: .light))
                                    .foregroundColor(CityPopTheme.textSecondary)
                            }
                            
                            VStack(alignment: .leading, spacing: 2) {
                                Text("Custom Server")
                                    .font(.system(size: 13, weight: .medium))
                                    .foregroundColor(CityPopTheme.textPrimary)
                                Text("Configure any server from the registry")
                                    .font(.system(size: 11))
                                    .foregroundColor(CityPopTheme.textSecondary)
                            }
                            
                            Spacer()
                            
                            Image(systemName: "chevron.right")
                                .font(.system(size: 11, weight: .medium))
                                .foregroundColor(CityPopTheme.textMuted)
                        }
                        .padding(12)
                        .background(CityPopTheme.surface)
                        .cornerRadius(10)
                        .overlay(
                            RoundedRectangle(cornerRadius: 10)
                                .stroke(CityPopTheme.border, lineWidth: 1)
                        )
                    }
                    .buttonStyle(.plain)
                    
                    // Browse registry link
                    Link(destination: URL(string: mcpRegistryURL)!) {
                        HStack(spacing: 6) {
                            Image(systemName: "globe")
                                .font(.system(size: 12))
                            Text("Browse full MCP registry on GitHub")
                                .font(.system(size: 12))
                            Image(systemName: "arrow.up.right")
                                .font(.system(size: 10))
                        }
                        .foregroundColor(CityPopTheme.accent)
                    }
                    .buttonStyle(.plain)
                    .padding(.top, 4)
                }
                .padding(20)
            }
        }
    }
    
    private var filteredCategories: [MCPCategory] {
        if searchText.isEmpty {
            return mcpCategories
        }
        
        // Filter categories to only include those with matching servers
        return mcpCategories.compactMap { category in
            let matchingServers = category.servers.filter {
                $0.name.localizedCaseInsensitiveContains(searchText) ||
                $0.description.localizedCaseInsensitiveContains(searchText)
            }
            
            if matchingServers.isEmpty {
                return nil
            }
            
            return MCPCategory(
                id: category.id,
                name: category.name,
                color: category.color,
                servers: matchingServers
            )
        }
    }
    
    // MARK: - Credentials Entry
    
    private func credentialsView(for template: MCPServerTemplate) -> some View {
        VStack(spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    // Service info
                    HStack(spacing: 14) {
                        ZStack {
                            RoundedRectangle(cornerRadius: 10)
                                .fill(CityPopTheme.accent.opacity(0.1))
                                .frame(width: 48, height: 48)
                            
                            Image(systemName: template.icon)
                                .font(.system(size: 20, weight: .light))
                                .foregroundColor(CityPopTheme.accent)
                        }
                        
                        VStack(alignment: .leading, spacing: 2) {
                            Text(template.name)
                                .font(.system(size: 16, weight: .semibold))
                                .foregroundColor(CityPopTheme.textPrimary)
                            
                            Text(template.description)
                                .font(.system(size: 13))
                                .foregroundColor(CityPopTheme.textSecondary)
                        }
                        
                        Spacer()
                    }
                    .padding(.bottom, 8)
                    
                    // Credentials fields
                    VStack(alignment: .leading, spacing: 16) {
                        ForEach(template.envVars, id: \.name) { envVar in
                            VStack(alignment: .leading, spacing: 6) {
                                Text(envVar.label)
                                    .font(.system(size: 12, weight: .medium))
                                    .foregroundColor(CityPopTheme.textSecondary)
                                
                                if envVar.isSecret {
                                    SecureField(envVar.placeholder, text: binding(for: envVar.name))
                                        .textFieldStyle(.plain)
                                        .font(.system(size: 13, design: .monospaced))
                                        .padding(10)
                                        .background(CityPopTheme.surface)
                                        .cornerRadius(6)
                                        .overlay(
                                            RoundedRectangle(cornerRadius: 6)
                                                .stroke(CityPopTheme.border, lineWidth: 1)
                                        )
                                } else {
                                    TextField(envVar.placeholder, text: binding(for: envVar.name))
                                        .textFieldStyle(.plain)
                                        .font(.system(size: 13))
                                        .padding(10)
                                        .background(CityPopTheme.surface)
                                        .cornerRadius(6)
                                        .overlay(
                                            RoundedRectangle(cornerRadius: 6)
                                                .stroke(CityPopTheme.border, lineWidth: 1)
                                        )
                                }
                            }
                        }
                    }
                    
                    // Help link
                    Link(destination: URL(string: template.docsUrl)!) {
                        HStack(spacing: 6) {
                            Image(systemName: "arrow.up.right.circle")
                                .font(.system(size: 12))
                            Text("How to get your \(template.name) credentials")
                                .font(.system(size: 12))
                        }
                        .foregroundColor(CityPopTheme.accent)
                    }
                    
                    if let error = installError {
                        HStack(spacing: 8) {
                            Image(systemName: "exclamationmark.triangle")
                                .foregroundColor(CityPopTheme.error)
                            Text(error)
                                .font(.system(size: 12))
                                .foregroundColor(CityPopTheme.error)
                        }
                        .padding(12)
                        .background(CityPopTheme.error.opacity(0.1))
                        .cornerRadius(8)
                    }
                }
                .padding(20)
            }
            
            Divider()
            
            // Footer buttons
            HStack {
                Button(action: { step = .selectServer }) {
                    Text("Back")
                        .font(.system(size: 13, weight: .medium))
                        .foregroundColor(CityPopTheme.textSecondary)
                }
                .buttonStyle(.plain)
                
                Spacer()
                
                SimpleButton(title: "Connect \(template.name)", style: .primary) {
                    installServer(template)
                }
                .disabled(!allFieldsFilled)
            }
            .padding(16)
        }
    }
    
    private func binding(for key: String) -> Binding<String> {
        Binding(
            get: { envValues[key] ?? "" },
            set: { envValues[key] = $0 }
        )
    }
    
    private var allFieldsFilled: Bool {
        guard let template = selectedTemplate else { return false }
        return template.envVars.allSatisfy { env in
            let value = envValues[env.name] ?? ""
            return !value.isEmpty
        }
    }
    
    // MARK: - Custom Server (Paste Config)
    
    private var customServerPasteView: some View {
        VStack(spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    // Instructions
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Paste the server configuration from the MCP registry or README:")
                            .font(.system(size: 13))
                            .foregroundColor(CityPopTheme.textSecondary)
                        
                        Link(destination: URL(string: mcpRegistryURL)!) {
                            HStack(spacing: 4) {
                                Image(systemName: "arrow.up.right.circle")
                                    .font(.system(size: 11))
                                Text("Browse MCP Registry")
                                    .font(.system(size: 12))
                            }
                            .foregroundColor(CityPopTheme.accent)
                        }
                    }
                    
                    // Config text area
                    ZStack(alignment: .topLeading) {
                        TextEditor(text: $pastedConfig)
                            .font(.system(size: 12, design: .monospaced))
                            .scrollContentBackground(.hidden)
                            .padding(8)
                        
                        if pastedConfig.isEmpty {
                            Text("""
                            {
                              "command": "npx",
                              "args": ["-y", "@modelcontextprotocol/server-github"],
                              "env": {
                                "GITHUB_PERSONAL_ACCESS_TOKEN": "<your-token>"
                              }
                            }
                            """)
                            .font(.system(size: 12, design: .monospaced))
                            .foregroundColor(CityPopTheme.textMuted.opacity(0.5))
                            .padding(12)
                            .allowsHitTesting(false)
                        }
                    }
                    .frame(height: 180)
                    .background(CityPopTheme.surface)
                    .cornerRadius(8)
                    .overlay(
                        RoundedRectangle(cornerRadius: 8)
                            .stroke(hasConfigError ? CityPopTheme.error : CityPopTheme.border, lineWidth: 1)
                    )
                    
                    // Parse error
                    if let error = parseError {
                        HStack(spacing: 6) {
                            Image(systemName: "exclamationmark.triangle")
                                .font(.system(size: 11))
                            Text(error)
                                .font(.system(size: 12))
                        }
                        .foregroundColor(CityPopTheme.error)
                    }
                    
                    // Missing runtime error
                    if let runtime = missingRuntime {
                        VStack(alignment: .leading, spacing: 8) {
                            HStack(spacing: 8) {
                                Image(systemName: "exclamationmark.triangle.fill")
                                    .font(.system(size: 14))
                                    .foregroundColor(CityPopTheme.warning)
                                
                                Text("Missing Runtime: \(runtime.command)")
                                    .font(.system(size: 13, weight: .semibold))
                                    .foregroundColor(CityPopTheme.textPrimary)
                            }
                            
                            Text("This server requires \"\(runtime.command)\" which is not installed on your Mac.")
                                .font(.system(size: 12))
                                .foregroundColor(CityPopTheme.textSecondary)
                            
                            if let instructions = runtime.installInstructions {
                                HStack(spacing: 6) {
                                    Image(systemName: "arrow.right.circle")
                                        .font(.system(size: 11))
                                    Text(instructions)
                                        .font(.system(size: 12, design: .monospaced))
                                }
                                .foregroundColor(CityPopTheme.accent)
                                .padding(.top, 2)
                            }
                        }
                        .padding(12)
                        .background(CityPopTheme.warning.opacity(0.1))
                        .cornerRadius(8)
                        .overlay(
                            RoundedRectangle(cornerRadius: 8)
                                .stroke(CityPopTheme.warning.opacity(0.3), lineWidth: 1)
                        )
                    }
                    
                    // Example format hint
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Expected format:")
                            .font(.system(size: 11, weight: .medium))
                            .foregroundColor(CityPopTheme.textMuted)
                        
                        Text("JSON with \"command\", \"args\", and optionally \"env\" fields")
                            .font(.system(size: 11))
                            .foregroundColor(CityPopTheme.textMuted)
                    }
                    .padding(.top, 4)
                }
                .padding(20)
            }
            
            Divider()
            
            // Footer
            HStack {
                Button(action: { step = .selectServer }) {
                    Text("Back")
                        .font(.system(size: 13, weight: .medium))
                        .foregroundColor(CityPopTheme.textSecondary)
                }
                .buttonStyle(.plain)
                
                Spacer()
                
                SimpleButton(title: "Parse Config", style: .primary) {
                    parseAndContinue()
                }
                .disabled(pastedConfig.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
            .padding(16)
        }
    }
    
    private func parseAndContinue() {
        parseError = nil
        missingRuntime = nil
        
        let trimmed = pastedConfig.trimmingCharacters(in: .whitespacesAndNewlines)
        
        guard !trimmed.isEmpty else {
            parseError = "Please paste a configuration"
            return
        }
        
        guard let config = parseMCPConfig(trimmed) else {
            parseError = "Invalid JSON format. Please check your configuration."
            return
        }
        
        // Check if config has at least command and args
        guard !config.command.isEmpty, !config.args.isEmpty else {
            parseError = "Configuration must include \"command\" and \"args\""
            return
        }
        
        // Preflight check: verify the runtime is installed
        let preflight = preflightCheck(command: config.command)
        guard preflight.isAvailable else {
            missingRuntime = preflight
            return
        }
        
        parsedConfig = config
        
        // Initialize env values for secrets
        customEnvValues = [:]
        for envVar in config.envVars where envVar.needsInput {
            customEnvValues[envVar.key] = ""
        }
        
        // If there are secrets to fill, go to secrets view; otherwise install directly
        if config.envVars.contains(where: { $0.needsInput }) {
            step = .customServerSecrets
        } else {
            installCustomServer()
        }
    }
    
    // MARK: - Custom Server Secrets
    
    private func customServerSecretsView(for config: ParsedMCPConfig) -> some View {
        VStack(spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    // Server info summary
                    HStack(spacing: 14) {
                        ZStack {
                            RoundedRectangle(cornerRadius: 10)
                                .fill(CityPopTheme.accent.opacity(0.1))
                                .frame(width: 48, height: 48)
                            
                            Image(systemName: "terminal")
                                .font(.system(size: 20, weight: .light))
                                .foregroundColor(CityPopTheme.accent)
                        }
                        
                        VStack(alignment: .leading, spacing: 4) {
                            Text(config.serverName.capitalized)
                                .font(.system(size: 16, weight: .semibold))
                                .foregroundColor(CityPopTheme.textPrimary)
                            
                            // Show command summary
                            Text("\(config.command) \(config.args.joined(separator: " "))")
                                .font(.system(size: 11, design: .monospaced))
                                .foregroundColor(CityPopTheme.textMuted)
                                .lineLimit(1)
                                .truncationMode(.middle)
                        }
                        
                        Spacer()
                        
                        // Checkmark to show config is valid
                        Image(systemName: "checkmark.circle.fill")
                            .font(.system(size: 16))
                            .foregroundColor(CityPopTheme.success)
                    }
                    .padding(.bottom, 8)
                    
                    // Credentials section
                    if config.envVars.contains(where: { $0.needsInput }) {
                        VStack(alignment: .leading, spacing: 16) {
                            Text("Enter your credentials:")
                                .font(.system(size: 13, weight: .medium))
                                .foregroundColor(CityPopTheme.textSecondary)
                            
                            ForEach(config.envVars.filter { $0.needsInput }) { envVar in
                                VStack(alignment: .leading, spacing: 6) {
                                    Text(envVar.humanReadableLabel)
                                        .font(.system(size: 12, weight: .medium))
                                        .foregroundColor(CityPopTheme.textSecondary)
                                    
                                    SecureField("Enter \(envVar.humanReadableLabel.lowercased())...", text: customBinding(for: envVar.key))
                                        .textFieldStyle(.plain)
                                        .font(.system(size: 13, design: .monospaced))
                                        .padding(10)
                                        .background(CityPopTheme.surface)
                                        .cornerRadius(6)
                                        .overlay(
                                            RoundedRectangle(cornerRadius: 6)
                                                .stroke(CityPopTheme.border, lineWidth: 1)
                                        )
                                }
                            }
                        }
                    }
                    
                    // Show pre-filled env vars (non-secret)
                    let prefilledVars = config.envVars.filter { !$0.needsInput && !$0.value.isEmpty }
                    if !prefilledVars.isEmpty {
                        VStack(alignment: .leading, spacing: 8) {
                            Text("Pre-configured values:")
                                .font(.system(size: 11, weight: .medium))
                                .foregroundColor(CityPopTheme.textMuted)
                            
                            ForEach(prefilledVars) { envVar in
                                HStack {
                                    Text(envVar.key)
                                        .font(.system(size: 11, design: .monospaced))
                                        .foregroundColor(CityPopTheme.textMuted)
                                    Spacer()
                                    Text(envVar.value)
                                        .font(.system(size: 11, design: .monospaced))
                                        .foregroundColor(CityPopTheme.textSecondary)
                                        .lineLimit(1)
                                }
                            }
                        }
                        .padding(12)
                        .background(CityPopTheme.surface.opacity(0.5))
                        .cornerRadius(8)
                    }
                    
                    if let error = installError {
                        HStack(spacing: 8) {
                            Image(systemName: "exclamationmark.triangle")
                                .foregroundColor(CityPopTheme.error)
                            Text(error)
                                .font(.system(size: 12))
                                .foregroundColor(CityPopTheme.error)
                        }
                        .padding(12)
                        .background(CityPopTheme.error.opacity(0.1))
                        .cornerRadius(8)
                    }
                }
                .padding(20)
            }
            
            Divider()
            
            // Footer
            HStack {
                Button(action: { step = .customServer }) {
                    Text("Back")
                        .font(.system(size: 13, weight: .medium))
                        .foregroundColor(CityPopTheme.textSecondary)
                }
                .buttonStyle(.plain)
                
                Spacer()
                
                SimpleButton(title: "Add Server", style: .primary) {
                    installCustomServer()
                }
                .disabled(!allCustomFieldsFilled)
            }
            .padding(16)
        }
    }
    
    private func customBinding(for key: String) -> Binding<String> {
        Binding(
            get: { customEnvValues[key] ?? "" },
            set: { customEnvValues[key] = $0 }
        )
    }
    
    private var allCustomFieldsFilled: Bool {
        guard let config = parsedConfig else { return false }
        return config.envVars.filter { $0.needsInput }.allSatisfy { env in
            let value = customEnvValues[env.key] ?? ""
            return !value.isEmpty
        }
    }
    
    private func installCustomServer() {
        guard let config = parsedConfig else { return }
        
        isInstalling = true
        installError = nil
        step = .installing
        
        Task {
            do {
                try await writeCustomConfig(config: config)
                
                await MainActor.run {
                    isInstalling = false
                    // Set selectedTemplate for the complete view
                    selectedTemplate = MCPServerTemplate(
                        id: config.serverName,
                        name: config.serverName.capitalized,
                        description: "Custom MCP server",
                        icon: "terminal",
                        npmPackage: config.args.joined(separator: " "),
                        envVars: [],
                        docsUrl: mcpRegistryURL
                    )
                    step = .complete
                }
            } catch {
                await MainActor.run {
                    isInstalling = false
                    installError = error.localizedDescription
                    step = .customServerSecrets
                }
            }
        }
    }
    
    private func writeCustomConfig(config: ParsedMCPConfig) async throws {
        let configDir = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent(".minna")
        let configPath = configDir.appendingPathComponent("mcp_sources.json")
        
        // Ensure directory exists
        try FileManager.default.createDirectory(at: configDir, withIntermediateDirectories: true)
        
        // Load existing config or create new
        var existingConfig: [String: Any] = [
            "version": "1.0",
            "description": "Minna MCP sources - managed by Minna Setup Wizard",
            "servers": [String: Any]()
        ]
        
        if FileManager.default.fileExists(atPath: configPath.path) {
            let data = try Data(contentsOf: configPath)
            if let existing = try JSONSerialization.jsonObject(with: data) as? [String: Any] {
                existingConfig = existing
            }
        }
        
        // Merge env vars: pre-filled values + user-entered values
        var finalEnv: [String: String] = [:]
        for envVar in config.envVars {
            if envVar.needsInput {
                finalEnv[envVar.key] = customEnvValues[envVar.key] ?? ""
            } else {
                finalEnv[envVar.key] = envVar.value
            }
        }
        
        // Add new server
        var servers = existingConfig["servers"] as? [String: Any] ?? [:]
        servers[config.serverName] = [
            "command": config.command,
            "args": config.args,
            "env": finalEnv,
            "enabled": true
        ]
        existingConfig["servers"] = servers
        
        // Write back
        let data = try JSONSerialization.data(withJSONObject: existingConfig, options: [.prettyPrinted, .sortedKeys])
        try data.write(to: configPath)
    }
    
    // MARK: - Installing
    
    private var installingView: some View {
        VStack(spacing: 24) {
            Spacer()
            
            ProgressView()
                .scaleEffect(1.5)
            
            VStack(spacing: 8) {
                Text("Setting Up \(serverDisplayName)")
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundColor(CityPopTheme.textPrimary)
                
                Text("Writing configuration and testing connection...")
                    .font(.system(size: 13))
                    .foregroundColor(CityPopTheme.textSecondary)
            }
            
            Spacer()
        }
    }
    
    // MARK: - Complete
    
    private var completeView: some View {
        VStack(spacing: 24) {
            Spacer()
            
            ZStack {
                Circle()
                    .fill(CityPopTheme.success.opacity(0.15))
                    .frame(width: 80, height: 80)
                
                Image(systemName: "checkmark.circle.fill")
                    .font(.system(size: 44))
                    .foregroundColor(CityPopTheme.success)
            }
            
            VStack(spacing: 8) {
                Text("\(selectedTemplate?.name ?? "Server") Connected!")
                    .font(.system(size: 18, weight: .semibold))
                    .foregroundColor(CityPopTheme.textPrimary)
                
                Text("Your MCP source is ready to sync.")
                    .font(.system(size: 13))
                    .foregroundColor(CityPopTheme.textSecondary)
            }
            
            Spacer()
            
            SimpleButton(title: "Done", style: .primary) {
                onComplete()
            }
            .padding(.bottom, 20)
        }
    }
    
    // MARK: - Install Logic
    
    private func installServer(_ template: MCPServerTemplate) {
        isInstalling = true
        installError = nil
        step = .installing
        
        Task {
            do {
                // Write to ~/.minna/mcp_sources.json
                try await writeMinnaConfig(template: template)
                
                await MainActor.run {
                    isInstalling = false
                    step = .complete
                }
            } catch {
                await MainActor.run {
                    isInstalling = false
                    installError = error.localizedDescription
                    step = .enterCredentials
                }
            }
        }
    }
    
    private func writeMinnaConfig(template: MCPServerTemplate) async throws {
        let configDir = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent(".minna")
        let configPath = configDir.appendingPathComponent("mcp_sources.json")
        
        // Ensure directory exists
        try FileManager.default.createDirectory(at: configDir, withIntermediateDirectories: true)
        
        // Load existing config or create new
        var config: [String: Any] = [
            "version": "1.0",
            "description": "Minna MCP sources - managed by Minna Setup Wizard",
            "servers": [String: Any]()
        ]
        
        if FileManager.default.fileExists(atPath: configPath.path) {
            let data = try Data(contentsOf: configPath)
            if let existing = try JSONSerialization.jsonObject(with: data) as? [String: Any] {
                config = existing
            }
        }
        
        // Add new server
        var servers = config["servers"] as? [String: Any] ?? [:]
        servers[template.id] = [
            "command": "npx",
            "args": ["-y", template.npmPackage],
            "env": envValues,
            "enabled": true
        ]
        config["servers"] = servers
        
        // Write back
        let data = try JSONSerialization.data(withJSONObject: config, options: [.prettyPrinted, .sortedKeys])
        try data.write(to: configPath)
    }
}

// MARK: - Category Section

struct CategorySection: View {
    let category: MCPCategory
    let onSelect: (MCPServerTemplate) -> Void
    
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            // Category header with color accent
            HStack(spacing: 8) {
                RoundedRectangle(cornerRadius: 2)
                    .fill(category.color)
                    .frame(width: 3, height: 14)
                
                Text(category.name.uppercased())
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundColor(category.color)
                    .tracking(0.8)
            }
            
            // Server grid for this category
            LazyVGrid(columns: [
                GridItem(.flexible(), spacing: 10),
                GridItem(.flexible(), spacing: 10)
            ], spacing: 10) {
                ForEach(category.servers) { template in
                    ServerTemplateCard(template: template, accentColor: category.color) {
                        onSelect(template)
                    }
                }
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

// MARK: - Server Template Card

struct ServerTemplateCard: View {
    let template: MCPServerTemplate
    let accentColor: Color
    let onSelect: () -> Void
    
    @State private var isHovered = false
    
    var body: some View {
        Button(action: onSelect) {
            VStack(alignment: .leading, spacing: 10) {
                ZStack {
                    RoundedRectangle(cornerRadius: 8)
                        .fill(accentColor.opacity(0.1))
                        .frame(width: 34, height: 34)
                    
                    Image(systemName: template.icon)
                        .font(.system(size: 15, weight: .light))
                        .foregroundColor(accentColor)
                }
                
                VStack(alignment: .leading, spacing: 2) {
                    Text(template.name)
                        .font(.system(size: 13, weight: .medium))
                        .foregroundColor(CityPopTheme.textPrimary)
                    
                    Text(template.description)
                        .font(.system(size: 10))
                        .foregroundColor(CityPopTheme.textSecondary)
                        .lineLimit(2)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(12)
            .background(isHovered ? accentColor.opacity(0.05) : CityPopTheme.background)
            .cornerRadius(10)
            .overlay(
                RoundedRectangle(cornerRadius: 10)
                    .stroke(isHovered ? accentColor.opacity(0.5) : CityPopTheme.border, lineWidth: 1)
            )
        }
        .buttonStyle(.plain)
        .onHover { isHovered = $0 }
        #if canImport(Inject)
        .enableInjection()
        #endif
    }

    #if canImport(Inject)
    @ObserveInjection var forceRedraw
    #endif
}

// MARK: - Preview

struct MCPSetupWizard_Previews: PreviewProvider {
    static var previews: some View {
        MCPSetupWizard(
            onComplete: {},
            onCancel: {}
        )
    }
}

