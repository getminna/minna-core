import SwiftUI

/// Configuration sheet for entering OAuth credentials
/// "Sovereign Mode" - users provide their own Client ID/Secret
struct ProviderConfigSheet: View {
    let provider: Provider
    let onComplete: () -> Void
    let onCancel: () -> Void
    
    @StateObject private var oauthManager = LocalOAuthManager.shared
    @State private var clientId: String = ""
    @State private var clientSecret: String = ""
    @State private var isLoading = false
    @State private var errorMessage: String?
    @State private var showingHelp = false
    
    private var isValid: Bool {
        !clientId.trimmingCharacters(in: .whitespaces).isEmpty &&
        !clientSecret.trimmingCharacters(in: .whitespaces).isEmpty
    }
    
    var body: some View {
        VStack(spacing: 0) {
            // Header
            header
            
            Divider()
            
            // Content
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    explanationSection
                    credentialsSection
                    
                    if let error = errorMessage {
                        errorBanner(error)
                    }
                }
                .padding(24)
            }
            
            Divider()
            
            // Footer
            footer
        }
        .frame(width: 480, height: 520)
        .background(CityPopTheme.background)
        .onAppear {
            loadExistingCredentials()
        }
    }
    
    // MARK: - Header
    
    private var header: some View {
        HStack {
            VStack(alignment: .leading, spacing: 4) {
                Text("\(provider.displayName) Configuration")
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundColor(CityPopTheme.textPrimary)
                
                Text("Sovereign Mode — Your keys, your data")
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
    
    // MARK: - Explanation
    
    private var explanationSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                Image(systemName: "lock.shield")
                    .font(.system(size: 14))
                    .foregroundColor(CityPopTheme.success)
                
                Text("Why provide your own credentials?")
                    .font(.system(size: 13, weight: .medium))
                    .foregroundColor(CityPopTheme.textPrimary)
            }
            
            Text("Minna runs 100% locally on your machine. By using your own OAuth credentials, Google talks directly to you — not through any third-party server. This ensures total privacy and sovereignty over your data.")
                .font(.system(size: 12))
                .foregroundColor(CityPopTheme.textSecondary)
                .lineSpacing(4)
            
            Button(action: { showingHelp = true }) {
                HStack(spacing: 4) {
                    Image(systemName: "questionmark.circle")
                        .font(.system(size: 11))
                    Text("How to get credentials (3 steps)")
                        .font(.system(size: 11, weight: .medium))
                }
                .foregroundColor(CityPopTheme.accent)
            }
            .buttonStyle(.plain)
            .sheet(isPresented: $showingHelp) {
                CredentialsHelpSheet(provider: provider) {
                    showingHelp = false
                }
            }
        }
        .padding(16)
        .background(CityPopTheme.surface)
        .cornerRadius(8)
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .stroke(CityPopTheme.border, lineWidth: 1)
        )
    }
    
    // MARK: - Credentials Input
    
    private var credentialsSection: some View {
        VStack(alignment: .leading, spacing: 16) {
            // Client ID
            VStack(alignment: .leading, spacing: 6) {
                Text("Client ID")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundColor(CityPopTheme.textSecondary)
                
                TextField("e.g., 123456789-abc123.apps.googleusercontent.com", text: $clientId)
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
            
            // Client Secret
            VStack(alignment: .leading, spacing: 6) {
                Text("Client Secret")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundColor(CityPopTheme.textSecondary)
                
                SecureField("e.g., GOCSPX-xxxxxxxxxxxxx", text: $clientSecret)
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
            
            // Redirect URI info
            VStack(alignment: .leading, spacing: 4) {
                Text("Redirect URI (add this to your Google app)")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundColor(CityPopTheme.textMuted)
                
                HStack {
                    Text("http://127.0.0.1:8847/callback")
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundColor(CityPopTheme.textSecondary)
                    
                    Button(action: copyRedirectURI) {
                        Image(systemName: "doc.on.doc")
                            .font(.system(size: 10))
                            .foregroundColor(CityPopTheme.textMuted)
                    }
                    .buttonStyle(.plain)
                }
                .padding(8)
                .background(CityPopTheme.divider.opacity(0.5))
                .cornerRadius(4)
            }
        }
    }
    
    // MARK: - Error Banner
    
    private func errorBanner(_ message: String) -> some View {
        HStack(spacing: 8) {
            Image(systemName: "exclamationmark.triangle.fill")
                .font(.system(size: 12))
                .foregroundColor(CityPopTheme.error)
            
            Text(message)
                .font(.system(size: 12))
                .foregroundColor(CityPopTheme.error)
            
            Spacer()
        }
        .padding(12)
        .background(CityPopTheme.error.opacity(0.1))
        .cornerRadius(6)
    }
    
    // MARK: - Footer
    
    private var footer: some View {
        HStack {
            if oauthManager.hasCredentials(for: provider) {
                Button(action: clearCredentials) {
                    Text("Clear Credentials")
                        .font(.system(size: 12))
                        .foregroundColor(CityPopTheme.error)
                }
                .buttonStyle(.plain)
            }
            
            Spacer()
            
            Button(action: onCancel) {
                Text("Cancel")
                    .font(.system(size: 13, weight: .medium))
                    .foregroundColor(CityPopTheme.textSecondary)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
            }
            .buttonStyle(.plain)
            
            Button(action: saveAndConnect) {
                HStack(spacing: 6) {
                    if isLoading {
                        ProgressView()
                            .scaleEffect(0.7)
                            .frame(width: 12, height: 12)
                    }
                    Text(isLoading ? "Connecting..." : "Save & Connect")
                        .font(.system(size: 13, weight: .medium))
                }
                .foregroundColor(.white)
                .padding(.horizontal, 16)
                .padding(.vertical, 8)
                .background(isValid ? CityPopTheme.accent : CityPopTheme.textMuted)
                .cornerRadius(6)
            }
            .buttonStyle(.plain)
            .disabled(!isValid || isLoading)
        }
        .padding(20)
        .background(CityPopTheme.surface)
    }
    
    // MARK: - Actions
    
    private func loadExistingCredentials() {
        if let creds = oauthManager.loadCredentials(for: provider) {
            clientId = creds.clientId
            clientSecret = creds.clientSecret
        }
    }
    
    private func copyRedirectURI() {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString("http://127.0.0.1:8847/callback", forType: .string)
    }
    
    private func clearCredentials() {
        oauthManager.clearCredentials(for: provider)
        clientId = ""
        clientSecret = ""
        MinnaEngineManager.shared.providerStates[provider] = .idle
    }
    
    private func saveAndConnect() {
        guard isValid else { return }
        
        isLoading = true
        errorMessage = nil
        
        // Save credentials
        oauthManager.saveCredentials(
            clientId: clientId.trimmingCharacters(in: .whitespaces),
            clientSecret: clientSecret.trimmingCharacters(in: .whitespaces),
            for: provider
        )
        
        // Start OAuth flow
        oauthManager.startGoogleOAuth { result in
            DispatchQueue.main.async {
                isLoading = false
                
                switch result {
                case .success:
                    MinnaEngineManager.shared.providerStates[provider] = .active
                    MinnaEngineManager.shared.addSyncEvent(for: provider, type: .connected, message: "Connected successfully")
                    onComplete()
                    
                case .failure(let error):
                    errorMessage = error.localizedDescription
                }
            }
        }
    }
}

// MARK: - Help Sheet

struct CredentialsHelpSheet: View {
    let provider: Provider
    let onDismiss: () -> Void
    
    var body: some View {
        VStack(spacing: 0) {
            // Header
            HStack {
                Text("Setting up \(provider.displayName) OAuth")
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundColor(CityPopTheme.textPrimary)
                
                Spacer()
                
                Button(action: onDismiss) {
                    Image(systemName: "xmark")
                        .font(.system(size: 12, weight: .medium))
                        .foregroundColor(CityPopTheme.textMuted)
                }
                .buttonStyle(.plain)
            }
            .padding(20)
            .background(CityPopTheme.surface)
            
            Divider()
            
            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    stepView(
                        number: 1,
                        title: "Create a Google Cloud Project",
                        description: "Go to console.cloud.google.com and create a new project (or use an existing one).",
                        link: "https://console.cloud.google.com/projectcreate"
                    )
                    
                    stepView(
                        number: 2,
                        title: "Enable APIs & Create Credentials",
                        description: "Enable the Calendar API and Gmail API. Then go to 'Credentials' and create an OAuth 2.0 Client ID (Desktop app type).",
                        link: "https://console.cloud.google.com/apis/credentials"
                    )
                    
                    stepView(
                        number: 3,
                        title: "Configure Redirect URI",
                        description: "Add this redirect URI to your OAuth client:\n\nhttp://127.0.0.1:8847/callback",
                        link: nil
                    )
                    
                    // Important note
                    VStack(alignment: .leading, spacing: 8) {
                        HStack(spacing: 6) {
                            Image(systemName: "info.circle.fill")
                                .foregroundColor(CityPopTheme.accent)
                            Text("Important")
                                .font(.system(size: 12, weight: .semibold))
                        }
                        
                        Text("Your credentials are stored locally in macOS Keychain and never leave your machine. Minna has no access to your Client Secret.")
                            .font(.system(size: 12))
                            .foregroundColor(CityPopTheme.textSecondary)
                    }
                    .padding(12)
                    .background(CityPopTheme.accent.opacity(0.1))
                    .cornerRadius(8)
                }
                .padding(24)
            }
        }
        .frame(width: 500, height: 480)
        .background(CityPopTheme.background)
    }
    
    private func stepView(number: Int, title: String, description: String, link: String?) -> some View {
        HStack(alignment: .top, spacing: 12) {
            // Step number
            Text("\(number)")
                .font(.system(size: 12, weight: .bold))
                .foregroundColor(.white)
                .frame(width: 24, height: 24)
                .background(CityPopTheme.accent)
                .cornerRadius(12)
            
            VStack(alignment: .leading, spacing: 6) {
                Text(title)
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundColor(CityPopTheme.textPrimary)
                
                Text(description)
                    .font(.system(size: 12))
                    .foregroundColor(CityPopTheme.textSecondary)
                    .lineSpacing(4)
                
                if let link = link, let url = URL(string: link) {
                    Button(action: { NSWorkspace.shared.open(url) }) {
                        HStack(spacing: 4) {
                            Text("Open in browser")
                                .font(.system(size: 11, weight: .medium))
                            Image(systemName: "arrow.up.right")
                                .font(.system(size: 9))
                        }
                        .foregroundColor(CityPopTheme.accent)
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }
}

// MARK: - Preview

#Preview {
    ProviderConfigSheet(
        provider: .googleWorkspace,
        onComplete: {},
        onCancel: {}
    )
}

