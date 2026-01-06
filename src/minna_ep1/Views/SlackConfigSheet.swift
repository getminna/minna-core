import SwiftUI

/// Slack configuration using App Manifest pattern
/// User creates their own Slack App and provides tokens directly
struct SlackConfigSheet: View {
    let onComplete: () -> Void
    let onCancel: () -> Void
    
    @State private var currentStep: SetupStep = .manifest
    @State private var botToken: String = ""
    @State private var userToken: String = ""
    @State private var isValidating = false
    @State private var validationError: String?
    @State private var showCopiedFeedback = false
    
    enum SetupStep {
        case manifest
        case tokens
    }
    
    // MARK: - Slack App Manifest
    
    private let slackManifest = """
display_information:
  name: Minna AI
  description: Local-first AI context engine
  background_color: "#1a1a2e"
features:
  bot_user:
    display_name: Minna AI
    always_online: false
oauth_config:
  scopes:
    user:
      - channels:history
      - channels:read
      - groups:history
      - groups:read
      - im:history
      - im:read
      - mpim:history
      - mpim:read
      - users:read
      - search:read
      - team:read
    bot:
      - channels:history
      - channels:read
      - groups:history
      - groups:read
      - im:history
      - im:read
      - mpim:history
      - mpim:read
      - users:read
settings:
  org_deploy_enabled: false
  socket_mode_enabled: false
  token_rotation_enabled: false
"""
    
    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            
            if currentStep == .manifest {
                manifestStep
            } else {
                tokenStep
            }
            
            Divider()
            footer
        }
        .frame(width: 520, height: 580)
        .background(CityPopTheme.background)
        .onAppear {
            loadExistingTokens()
        }
    }
    
    // MARK: - Header
    
    private var header: some View {
        HStack {
            VStack(alignment: .leading, spacing: 4) {
                Text("Slack Configuration")
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundColor(CityPopTheme.textPrimary)
                
                Text("Sovereign Mode — Create your own Slack App")
                    .font(.system(size: 12))
                    .foregroundColor(CityPopTheme.textSecondary)
            }
            
            Spacer()
            
            // Step indicator
            HStack(spacing: 8) {
                stepDot(step: 1, label: "Manifest", isActive: currentStep == .manifest)
                Rectangle().fill(CityPopTheme.border).frame(width: 20, height: 1)
                stepDot(step: 2, label: "Tokens", isActive: currentStep == .tokens)
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
    
    private func stepDot(step: Int, label: String, isActive: Bool) -> some View {
        VStack(spacing: 4) {
            Circle()
                .fill(isActive ? CityPopTheme.accent : CityPopTheme.border)
                .frame(width: 24, height: 24)
                .overlay(
                    Text("\(step)")
                        .font(.system(size: 11, weight: .bold))
                        .foregroundColor(isActive ? .white : CityPopTheme.textMuted)
                )
            
            Text(label)
                .font(.system(size: 10))
                .foregroundColor(isActive ? CityPopTheme.textPrimary : CityPopTheme.textMuted)
        }
    }
    
    // MARK: - Manifest Step
    
    private var manifestStep: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                // Why section
                VStack(alignment: .leading, spacing: 8) {
                    HStack(spacing: 6) {
                        Image(systemName: "lock.shield")
                            .foregroundColor(CityPopTheme.success)
                        Text("Why create your own app?")
                            .font(.system(size: 13, weight: .medium))
                    }
                    
                    Text("By creating your own Slack App, you maintain full control over your data. Minna never sees your Slack credentials — they stay on your machine.")
                        .font(.system(size: 12))
                        .foregroundColor(CityPopTheme.textSecondary)
                        .lineSpacing(4)
                }
                .padding(16)
                .background(CityPopTheme.surface)
                .cornerRadius(8)
                .overlay(RoundedRectangle(cornerRadius: 8).stroke(CityPopTheme.border, lineWidth: 1))
                
                // Instructions
                VStack(alignment: .leading, spacing: 16) {
                    instructionRow(number: 1, text: "Copy the manifest below")
                    instructionRow(number: 2, text: "Go to api.slack.com/apps → Create New App → From an app manifest")
                    instructionRow(number: 3, text: "Paste the manifest and create your app")
                    instructionRow(number: 4, text: "Click the green \"Install to Workspace\" button")
                }
                
                // Manifest box
                VStack(alignment: .leading, spacing: 8) {
                    HStack {
                        Text("App Manifest (YAML)")
                            .font(.system(size: 11, weight: .medium))
                            .foregroundColor(CityPopTheme.textMuted)
                        
                        Spacer()
                        
                        Button(action: copyManifest) {
                            HStack(spacing: 4) {
                                Image(systemName: showCopiedFeedback ? "checkmark" : "doc.on.doc")
                                    .font(.system(size: 10))
                                Text(showCopiedFeedback ? "Copied!" : "Copy")
                                    .font(.system(size: 11, weight: .medium))
                            }
                            .foregroundColor(showCopiedFeedback ? CityPopTheme.success : CityPopTheme.accent)
                        }
                        .buttonStyle(.plain)
                    }
                    
                    ScrollView {
                        Text(slackManifest)
                            .font(.system(size: 10, design: .monospaced))
                            .foregroundColor(CityPopTheme.textSecondary)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(12)
                    }
                    .frame(height: 160)
                    .background(CityPopTheme.surface)
                    .cornerRadius(6)
                    .overlay(RoundedRectangle(cornerRadius: 6).stroke(CityPopTheme.border, lineWidth: 1))
                }
                
                // Open Slack button
                Button(action: openSlackApps) {
                    HStack(spacing: 6) {
                        Text("Open Slack Apps")
                            .font(.system(size: 12, weight: .medium))
                        Image(systemName: "arrow.up.right")
                            .font(.system(size: 10))
                    }
                    .foregroundColor(CityPopTheme.accent)
                }
                .buttonStyle(.plain)
            }
            .padding(24)
        }
    }
    
    private func instructionRow(number: Int, text: String) -> some View {
        HStack(alignment: .top, spacing: 12) {
            Text("\(number)")
                .font(.system(size: 10, weight: .bold))
                .foregroundColor(.white)
                .frame(width: 20, height: 20)
                .background(CityPopTheme.textMuted)
                .cornerRadius(10)
            
            Text(text)
                .font(.system(size: 12))
                .foregroundColor(CityPopTheme.textPrimary)
        }
    }
    
    // MARK: - Token Step
    
    private var tokenStep: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                // Instructions
                VStack(alignment: .leading, spacing: 8) {
                    Text("After installing your app, find the tokens in:")
                        .font(.system(size: 12))
                        .foregroundColor(CityPopTheme.textSecondary)
                    
                    Text("OAuth & Permissions → OAuth Tokens for Your Workspace")
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundColor(CityPopTheme.textPrimary)
                        .padding(8)
                        .background(CityPopTheme.surface)
                        .cornerRadius(4)
                }
                
                // User Token (primary)
                VStack(alignment: .leading, spacing: 6) {
                    HStack {
                        Text("User OAuth Token")
                            .font(.system(size: 12, weight: .medium))
                            .foregroundColor(CityPopTheme.textSecondary)
                        
                        Text("(required)")
                            .font(.system(size: 10))
                            .foregroundColor(CityPopTheme.accent)
                    }
                    
                    Text("Starts with xoxp-")
                        .font(.system(size: 10))
                        .foregroundColor(CityPopTheme.textMuted)
                    
                    SecureField("xoxp-...", text: $userToken)
                        .textFieldStyle(.plain)
                        .font(.system(size: 12, design: .monospaced))
                        .padding(10)
                        .background(CityPopTheme.surface)
                        .cornerRadius(6)
                        .overlay(
                            RoundedRectangle(cornerRadius: 6)
                                .stroke(userToken.isEmpty ? CityPopTheme.border : CityPopTheme.success.opacity(0.5), lineWidth: 1)
                        )
                }
                
                // Validation error
                if let error = validationError {
                    HStack(spacing: 6) {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .foregroundColor(CityPopTheme.error)
                        Text(error)
                            .font(.system(size: 12))
                            .foregroundColor(CityPopTheme.error)
                    }
                    .padding(12)
                    .background(CityPopTheme.error.opacity(0.1))
                    .cornerRadius(6)
                }
                
                // Security note
                VStack(alignment: .leading, spacing: 6) {
                    HStack(spacing: 6) {
                        Image(systemName: "lock.fill")
                            .font(.system(size: 10))
                            .foregroundColor(CityPopTheme.success)
                        Text("Stored securely in macOS Keychain")
                            .font(.system(size: 11, weight: .medium))
                            .foregroundColor(CityPopTheme.success)
                    }
                    
                    Text("Your tokens never leave your machine. Minna syncs directly from Slack to your local database.")
                        .font(.system(size: 11))
                        .foregroundColor(CityPopTheme.textMuted)
                }
                .padding(12)
                .background(CityPopTheme.success.opacity(0.1))
                .cornerRadius(8)
            }
            .padding(24)
        }
    }
    
    // MARK: - Footer
    
    private var footer: some View {
        HStack {
            if currentStep == .tokens {
                Button(action: clearTokens) {
                    Text("Clear Tokens")
                        .font(.system(size: 12))
                        .foregroundColor(CityPopTheme.error)
                }
                .buttonStyle(.plain)
            }
            
            Spacer()
            
            if currentStep == .manifest {
                Button(action: { currentStep = .tokens }) {
                    Text("I've created my app →")
                        .font(.system(size: 13, weight: .medium))
                        .foregroundColor(.white)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 8)
                        .background(CityPopTheme.accent)
                        .cornerRadius(6)
                }
                .buttonStyle(.plain)
            } else {
                Button(action: { currentStep = .manifest }) {
                    Text("← Back")
                        .font(.system(size: 13, weight: .medium))
                        .foregroundColor(CityPopTheme.textSecondary)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 8)
                }
                .buttonStyle(.plain)
                
                Button(action: saveAndConnect) {
                    HStack(spacing: 6) {
                        if isValidating {
                            ProgressView()
                                .scaleEffect(0.7)
                                .frame(width: 12, height: 12)
                        }
                        Text(isValidating ? "Validating..." : "Connect")
                    }
                    .font(.system(size: 13, weight: .medium))
                    .foregroundColor(.white)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
                    .background(isValid ? CityPopTheme.accent : CityPopTheme.textMuted)
                    .cornerRadius(6)
                }
                .buttonStyle(.plain)
                .disabled(!isValid || isValidating)
            }
        }
        .padding(20)
        .background(CityPopTheme.surface)
    }
    
    // MARK: - Computed Properties
    
    private var isValid: Bool {
        // Need at least the user token
        userToken.hasPrefix("xoxp-") && userToken.count > 20
    }
    
    // MARK: - Actions
    
    private func loadExistingTokens() {
        let tokens = CredentialManager.shared.loadSlackTokens()
        botToken = tokens.bot ?? ""
        userToken = tokens.user ?? ""
        
        // If tokens exist, go directly to token step
        if !userToken.isEmpty {
            currentStep = .tokens
        }
    }
    
    private func copyManifest() {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(slackManifest, forType: .string)
        
        showCopiedFeedback = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
            showCopiedFeedback = false
        }
    }
    
    private func openSlackApps() {
        if let url = URL(string: "https://api.slack.com/apps") {
            NSWorkspace.shared.open(url)
        }
    }
    
    private func clearTokens() {
        CredentialManager.shared.clearSlackTokens()
        botToken = ""
        userToken = ""
        MinnaEngineManager.shared.providerStates[.slack] = .idle
    }
    
    private func saveAndConnect() {
        guard isValid else { return }
        
        isValidating = true
        validationError = nil
        
        // Validate token by calling Slack API
        validateSlackToken(userToken) { success, teamName, error in
            DispatchQueue.main.async {
                isValidating = false
                
                if success, let teamName = teamName {
                    // Save tokens
                    CredentialManager.shared.saveSlackTokens(
                        botToken: botToken.isEmpty ? nil : botToken,
                        userToken: userToken
                    )
                    
                    MinnaEngineManager.shared.providerStates[.slack] = .active
                    MinnaEngineManager.shared.addSyncEvent(
                        for: .slack,
                        type: .connected,
                        message: "Connected to \(teamName)"
                    )
                    
                    onComplete()
                } else {
                    validationError = error ?? "Validation failed"
                }
            }
        }
    }
    
    private func validateSlackToken(_ token: String, completion: @escaping (Bool, String?, String?) -> Void) {
        // Completion: (success: Bool, teamName: String?, error: String?)
        var request = URLRequest(url: URL(string: "https://slack.com/api/auth.test")!)
        request.httpMethod = "POST"
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        request.setValue("application/x-www-form-urlencoded", forHTTPHeaderField: "Content-Type")
        
        URLSession.shared.dataTask(with: request) { data, response, error in
            if let error = error {
                completion(false, nil, "Network error: \(error.localizedDescription)")
                return
            }
            
            guard let data = data,
                  let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
                completion(false, nil, "Invalid response from Slack")
                return
            }
            
            if json["ok"] as? Bool == true {
                let teamName = json["team"] as? String ?? "your workspace"
                completion(true, teamName, nil)
            } else {
                let slackError = json["error"] as? String ?? "Unknown error"
                completion(false, nil, "Slack error: \(slackError)")
            }
        }.resume()
    }
}

// MARK: - Preview

#Preview {
    SlackConfigSheet(onComplete: {}, onCancel: {})
}

