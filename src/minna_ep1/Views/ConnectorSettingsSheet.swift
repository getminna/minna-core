import SwiftUI
import AppKit
#if canImport(Inject)
import Inject
#endif

/// Unified settings sheet for viewing/updating/disconnecting connected providers
/// Shows masked credentials with update and disconnect options
struct ConnectorSettingsSheet: View {
    let provider: Provider
    let onDisconnect: () -> Void
    let onDone: () -> Void
    
    @State private var isEditing = false
    @State private var editedValue: String = ""
    @State private var editedSecondaryValue: String = ""  // For dual tokens (Slack)
    @State private var showDisconnectConfirm = false
    @State private var validationError: String?
    @State private var isValidating = false
    
    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    credentialsSection
                    
                    if let error = validationError {
                        errorBanner(error)
                    }
                }
                .padding(24)
            }
            
            Divider()
            footer
        }
        .frame(width: 420, height: provider == .googleWorkspace ? 340 : 280)
        .background(CityPopTheme.background)
        .alert("Disconnect \(provider.displayName)?", isPresented: $showDisconnectConfirm) {
            Button("Cancel", role: .cancel) { }
            Button("Disconnect", role: .destructive) {
                performDisconnect()
            }
        } message: {
            Text("This will remove your saved credentials. You'll need to set up the connection again to sync.")
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
            Text("\(provider.displayName) Connection")
                .font(.system(size: 16, weight: .semibold))
                .foregroundColor(CityPopTheme.textPrimary)
            
            Spacer()
            
            Button(action: onDone) {
                Image(systemName: "xmark")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundColor(CityPopTheme.textMuted)
            }
            .buttonStyle(.plain)
        }
        .padding(20)
        .background(CityPopTheme.surface)
    }
    
    // MARK: - Credentials Section
    
    @ViewBuilder
    private var credentialsSection: some View {
        switch provider {
        case .slack:
            slackCredentials
        case .googleWorkspace:
            googleCredentials
        case .github:
            githubCredentials
        case .cursor:
            localAICredentials
        }
    }
    
    // MARK: - Local AI Info (No Credentials)
    
    private var localAICredentials: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Local Data Source")
                .font(.system(size: 14, weight: .semibold))
                .foregroundColor(CityPopTheme.textPrimary)
            
            VStack(alignment: .leading, spacing: 8) {
                Label("No authentication required", systemImage: "checkmark.circle.fill")
                    .font(.system(size: 13))
                    .foregroundColor(CityPopTheme.success)
                
                Text(localAIDescription)
                    .font(.system(size: 12))
                    .foregroundColor(CityPopTheme.textMuted)
            }
            .padding(12)
            .background(CityPopTheme.surface)
            .cornerRadius(8)
        }
    }
    
    private var localAIDescription: String {
        switch provider {
        case .cursor:
            return "Cursor AI reads your local session history from ~/.cursor/plans and SQLite databases. All data stays on your machine."
        default:
            return "This provider reads local files. No authentication required."
        }
    }
    
    // MARK: - Slack Credentials
    
    private var slackCredentials: some View {
        VStack(alignment: .leading, spacing: 16) {
            let tokens = CredentialManager.shared.loadSlackTokens()
            
            if let userToken = tokens.user {
                credentialField(
                    label: "User Token",
                    value: userToken,
                    isEditing: $isEditing,
                    editedValue: $editedValue
                )
            }
            
            if tokens.user == nil {
                Text("No token configured")
                    .font(.system(size: 12))
                    .foregroundColor(CityPopTheme.textMuted)
            }
        }
    }
    
    // MARK: - Google Credentials
    
    private var googleCredentials: some View {
        VStack(alignment: .leading, spacing: 16) {
            if let creds = LocalOAuthManager.shared.loadCredentials(for: .googleWorkspace) {
                credentialField(
                    label: "Client ID",
                    value: creds.clientId,
                    isEditing: .constant(false),
                    editedValue: .constant("")
                )
                
                credentialField(
                    label: "Client Secret",
                    value: creds.clientSecret,
                    isEditing: .constant(false),
                    editedValue: .constant(""),
                    isSecret: true
                )
            }
            
            // Note about re-auth
            HStack(spacing: 6) {
                Image(systemName: "info.circle")
                    .font(.system(size: 11))
                    .foregroundColor(CityPopTheme.textMuted)
                Text("To update credentials, disconnect and reconnect.")
                    .font(.system(size: 11))
                    .foregroundColor(CityPopTheme.textMuted)
            }
        }
    }
    
    // MARK: - GitHub Credentials
    
    private var githubCredentials: some View {
        VStack(alignment: .leading, spacing: 16) {
            if let pat = CredentialManager.shared.loadGitHubPAT() {
                credentialField(
                    label: "Personal Access Token",
                    value: pat,
                    isEditing: $isEditing,
                    editedValue: $editedValue
                )
            } else {
                Text("No PAT configured")
                    .font(.system(size: 12))
                    .foregroundColor(CityPopTheme.textMuted)
            }
        }
    }
    
    // MARK: - Credential Field
    
    private func credentialField(
        label: String,
        value: String,
        isEditing: Binding<Bool>,
        editedValue: Binding<String>,
        isSecret: Bool = false
    ) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(label)
                .font(.system(size: 12, weight: .medium))
                .foregroundColor(CityPopTheme.textSecondary)
            
            if isEditing.wrappedValue {
                // Edit mode
                HStack {
                    if isSecret {
                        SecureField("Enter new value", text: editedValue)
                            .textFieldStyle(.plain)
                            .font(.system(size: 12, design: .monospaced))
                    } else {
                        TextField("Enter new value", text: editedValue)
                            .textFieldStyle(.plain)
                            .font(.system(size: 12, design: .monospaced))
                    }
                    
                    Button("Save") {
                        saveUpdatedCredential()
                    }
                    .font(.system(size: 11, weight: .medium))
                    .foregroundColor(CityPopTheme.accent)
                    .buttonStyle(.plain)
                    .disabled(editedValue.wrappedValue.isEmpty || isValidating)
                    
                    Button("Cancel") {
                        isEditing.wrappedValue = false
                        editedValue.wrappedValue = ""
                        validationError = nil
                    }
                    .font(.system(size: 11, weight: .medium))
                    .foregroundColor(CityPopTheme.textMuted)
                    .buttonStyle(.plain)
                }
                .padding(10)
                .background(CityPopTheme.surface)
                .cornerRadius(6)
                .overlay(
                    RoundedRectangle(cornerRadius: 6)
                        .stroke(CityPopTheme.accent.opacity(0.5), lineWidth: 1)
                )
            } else {
                // Display mode
                HStack {
                    Text(maskCredential(value))
                        .font(.system(size: 12, design: .monospaced))
                        .foregroundColor(CityPopTheme.textPrimary)
                    
                    Spacer()
                    
                    // Copy button - always available
                    CopyButton(value: value)
                    
                    // Only show update button for non-Google (Google requires full re-auth)
                    if provider != .googleWorkspace {
                        Button("Update") {
                            editedValue.wrappedValue = ""
                            isEditing.wrappedValue = true
                        }
                        .font(.system(size: 11, weight: .medium))
                        .foregroundColor(CityPopTheme.accent)
                        .buttonStyle(.plain)
                    }
                }
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
    
    // MARK: - Footer
    
    private var footer: some View {
        HStack {
            Button(action: { showDisconnectConfirm = true }) {
                Text("Disconnect")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundColor(CityPopTheme.error)
            }
            .buttonStyle(.plain)
            
            Spacer()
            
            Button(action: onDone) {
                Text("Done")
                    .font(.system(size: 13, weight: .medium))
                    .foregroundColor(.white)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
                    .background(CityPopTheme.accent)
                    .cornerRadius(6)
            }
            .buttonStyle(.plain)
        }
        .padding(20)
        .background(CityPopTheme.surface)
    }
    
    // MARK: - Error Banner
    
    private func errorBanner(_ message: String) -> some View {
        HStack(spacing: 6) {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundColor(CityPopTheme.error)
            Text(message)
                .font(.system(size: 12))
                .foregroundColor(CityPopTheme.error)
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(CityPopTheme.error.opacity(0.1))
        .cornerRadius(6)
    }
    
    // MARK: - Helpers
    
    private func maskCredential(_ value: String) -> String {
        // Show prefix and last 4 characters
        if value.count <= 8 {
            return String(repeating: "•", count: value.count)
        }
        
        let prefix: String
        if value.hasPrefix("xoxp-") {
            prefix = "xoxp-"
        } else if value.hasPrefix("xoxb-") {
            prefix = "xoxb-"
        } else if value.hasPrefix("github_pat_") {
            prefix = "github_pat_"
        } else if value.contains(".apps.googleusercontent.com") {
            // For Google Client ID, show first 8 and last part
            let parts = value.split(separator: "-")
            if let first = parts.first {
                return "\(first.prefix(8))•••••.apps.googleusercontent.com"
            }
            prefix = ""
        } else if value.hasPrefix("GOCSPX-") {
            prefix = "GOCSPX-"
        } else {
            prefix = String(value.prefix(4))
        }
        
        let lastFour = String(value.suffix(4))
        let middleLength = max(0, value.count - prefix.count - 4)
        let masked = String(repeating: "•", count: min(middleLength, 16))
        
        return "\(prefix)\(masked)\(lastFour)"
    }
    
    private func performDisconnect() {
        CredentialManager.shared.clearAllCredentials(for: provider)
        MinnaEngineManager.shared.providerStates[provider] = .idle
        MinnaEngineManager.shared.syncHistory[provider] = []
        onDisconnect()
    }
    
    private func saveUpdatedCredential() {
        guard !editedValue.isEmpty else { return }
        
        isValidating = true
        validationError = nil
        
        switch provider {
        case .slack:
            validateAndSaveSlackToken()
        case .github:
            validateAndSaveGitHubPAT()
        case .googleWorkspace:
            // Google requires full re-auth, shouldn't reach here
            break
        case .cursor:
            // Local AI tool has no credentials to update
            break
        }
    }
    
    private func validateAndSaveSlackToken() {
        let token = editedValue.trimmingCharacters(in: .whitespaces)
        
        guard token.hasPrefix("xoxp-") || token.hasPrefix("xoxb-") else {
            isValidating = false
            validationError = "Token must start with xoxp- or xoxb-"
            return
        }
        
        // Validate against Slack API
        var request = URLRequest(url: URL(string: "https://slack.com/api/auth.test")!)
        request.httpMethod = "POST"
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        
        URLSession.shared.dataTask(with: request) { data, _, error in
            DispatchQueue.main.async {
                isValidating = false
                
                if let error = error {
                    validationError = "Network error: \(error.localizedDescription)"
                    return
                }
                
                guard let data = data,
                      let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                      json["ok"] as? Bool == true else {
                    validationError = "Invalid token"
                    return
                }
                
                // Save the token
                if token.hasPrefix("xoxp-") {
                    CredentialManager.shared.saveSlackTokens(botToken: nil, userToken: token)
                } else {
                    CredentialManager.shared.saveSlackTokens(botToken: token, userToken: nil)
                }
                
                isEditing = false
                editedValue = ""
            }
        }.resume()
    }
    
    private func validateAndSaveGitHubPAT() {
        let pat = editedValue.trimmingCharacters(in: .whitespaces)
        
        guard pat.hasPrefix("github_pat_") || pat.hasPrefix("ghp_") else {
            isValidating = false
            validationError = "PAT must start with github_pat_ or ghp_"
            return
        }
        
        // Validate against GitHub API
        var request = URLRequest(url: URL(string: "https://api.github.com/user")!)
        request.setValue("Bearer \(pat)", forHTTPHeaderField: "Authorization")
        request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        
        URLSession.shared.dataTask(with: request) { data, response, error in
            DispatchQueue.main.async {
                isValidating = false
                
                if let error = error {
                    validationError = "Network error: \(error.localizedDescription)"
                    return
                }
                
                guard let httpResponse = response as? HTTPURLResponse,
                      httpResponse.statusCode == 200 else {
                    validationError = "Invalid PAT or insufficient permissions"
                    return
                }
                
                // Save the PAT
                CredentialManager.shared.saveGitHubPAT(pat)
                
                isEditing = false
                editedValue = ""
            }
        }.resume()
    }
}

// MARK: - Copy Button

/// A button that copies a value to clipboard with visual feedback
private struct CopyButton: View {
    let value: String
    
    @State private var showCopied = false
    
    var body: some View {
        Button(action: copyToClipboard) {
            HStack(spacing: 3) {
                Image(systemName: showCopied ? "checkmark" : "doc.on.doc")
                    .font(.system(size: 10))
                if showCopied {
                    Text("Copied")
                        .font(.system(size: 10))
                }
            }
            .foregroundColor(showCopied ? CityPopTheme.success : CityPopTheme.textMuted)
        }
        .buttonStyle(.plain)
        #if canImport(Inject)
        .enableInjection()
        #endif
    }

    #if canImport(Inject)
    @ObserveInjection var forceRedraw
    #endif
    
    private func copyToClipboard() {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(value, forType: .string)
        
        withAnimation {
            showCopied = true
        }
        
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) {
            withAnimation {
                showCopied = false
            }
        }
    }
}

// MARK: - Preview

#Preview {
    ConnectorSettingsSheet(
        provider: .slack,
        onDisconnect: {},
        onDone: {}
    )
}

