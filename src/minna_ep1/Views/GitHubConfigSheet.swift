import SwiftUI

/// GitHub configuration using Personal Access Token (PAT) pattern
/// The standard approach for CLI/local developer tools
struct GitHubConfigSheet: View {
    let onComplete: () -> Void
    let onCancel: () -> Void
    
    @State private var pat: String = ""
    @State private var isValidating = false
    @State private var validationError: String?
    @State private var validatedUser: String?
    
    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    whySection
                    instructionsSection
                    patInputSection
                    
                    if let error = validationError {
                        errorBanner(error)
                    }
                    
                    if let user = validatedUser {
                        successBanner(user)
                    }
                    
                    securityNote
                }
                .padding(24)
            }
            
            Divider()
            footer
        }
        .frame(width: 480, height: 540)
        .background(CityPopTheme.background)
        .onAppear {
            loadExistingPAT()
        }
    }
    
    // MARK: - Header
    
    private var header: some View {
        HStack {
            VStack(alignment: .leading, spacing: 4) {
                Text("GitHub Configuration")
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundColor(CityPopTheme.textPrimary)
                
                Text("Sovereign Mode — Personal Access Token")
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
    
    // MARK: - Why Section
    
    private var whySection: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 6) {
                Image(systemName: "key.fill")
                    .foregroundColor(CityPopTheme.success)
                Text("Why a Personal Access Token?")
                    .font(.system(size: 13, weight: .medium))
            }
            
            Text("PATs are the standard for developer tools. They give you granular control over what Minna can access — specific repos, orgs, or just your profile. You can revoke access anytime from GitHub settings.")
                .font(.system(size: 12))
                .foregroundColor(CityPopTheme.textSecondary)
                .lineSpacing(4)
        }
        .padding(16)
        .background(CityPopTheme.surface)
        .cornerRadius(8)
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(CityPopTheme.border, lineWidth: 1))
    }
    
    // MARK: - Instructions
    
    private var instructionsSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Create a Fine-grained Personal Access Token:")
                .font(.system(size: 12, weight: .medium))
                .foregroundColor(CityPopTheme.textPrimary)
            
            VStack(alignment: .leading, spacing: 10) {
                instructionRow(number: 1, text: "Go to GitHub → Settings → Developer settings")
                instructionRow(number: 2, text: "Select \"Personal access tokens\" → \"Fine-grained tokens\"")
                instructionRow(number: 3, text: "Create new token with these permissions:")
                
                // Required scopes
                VStack(alignment: .leading, spacing: 4) {
                    scopeRow("Contents", description: "Read access to code and files")
                    scopeRow("Issues", description: "Read access to issues")
                    scopeRow("Pull requests", description: "Read access to PRs and comments")
                    scopeRow("Metadata", description: "Read access to repo metadata")
                }
                .padding(.leading, 32)
            }
            
            Button(action: openGitHubTokens) {
                HStack(spacing: 4) {
                    Text("Open GitHub Token Settings")
                        .font(.system(size: 12, weight: .medium))
                    Image(systemName: "arrow.up.right")
                        .font(.system(size: 10))
                }
                .foregroundColor(CityPopTheme.accent)
            }
            .buttonStyle(.plain)
        }
    }
    
    private func instructionRow(number: Int, text: String) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Text("\(number)")
                .font(.system(size: 9, weight: .bold))
                .foregroundColor(.white)
                .frame(width: 18, height: 18)
                .background(CityPopTheme.textMuted)
                .cornerRadius(9)
            
            Text(text)
                .font(.system(size: 12))
                .foregroundColor(CityPopTheme.textSecondary)
        }
    }
    
    private func scopeRow(_ scope: String, description: String) -> some View {
        HStack(spacing: 8) {
            Image(systemName: "checkmark.circle.fill")
                .font(.system(size: 10))
                .foregroundColor(CityPopTheme.success)
            
            Text(scope)
                .font(.system(size: 11, weight: .medium, design: .monospaced))
                .foregroundColor(CityPopTheme.textPrimary)
            
            Text("— \(description)")
                .font(.system(size: 11))
                .foregroundColor(CityPopTheme.textMuted)
        }
    }
    
    // MARK: - PAT Input
    
    private var patInputSection: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Personal Access Token")
                .font(.system(size: 12, weight: .medium))
                .foregroundColor(CityPopTheme.textSecondary)
            
            Text("Starts with github_pat_ or ghp_")
                .font(.system(size: 10))
                .foregroundColor(CityPopTheme.textMuted)
            
            SecureField("github_pat_...", text: $pat)
                .textFieldStyle(.plain)
                .font(.system(size: 12, design: .monospaced))
                .padding(10)
                .background(CityPopTheme.surface)
                .cornerRadius(6)
                .overlay(
                    RoundedRectangle(cornerRadius: 6)
                        .stroke(validatedUser != nil ? CityPopTheme.success : CityPopTheme.border, lineWidth: 1)
                )
                .onChange(of: pat) { _, _ in
                    // Clear validation when PAT changes
                    validatedUser = nil
                    validationError = nil
                }
        }
    }
    
    // MARK: - Banners
    
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
    
    private func successBanner(_ username: String) -> some View {
        HStack(spacing: 8) {
            Image(systemName: "checkmark.circle.fill")
                .font(.system(size: 12))
                .foregroundColor(CityPopTheme.success)
            
            Text("Authenticated as @\(username)")
                .font(.system(size: 12, weight: .medium))
                .foregroundColor(CityPopTheme.success)
            
            Spacer()
        }
        .padding(12)
        .background(CityPopTheme.success.opacity(0.1))
        .cornerRadius(6)
    }
    
    // MARK: - Security Note
    
    private var securityNote: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Image(systemName: "lock.fill")
                    .font(.system(size: 10))
                    .foregroundColor(CityPopTheme.success)
                Text("Stored in macOS Keychain")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundColor(CityPopTheme.success)
            }
            
            Text("Your PAT is encrypted by the system and never leaves your machine. You can revoke it anytime from GitHub.")
                .font(.system(size: 11))
                .foregroundColor(CityPopTheme.textMuted)
        }
        .padding(12)
        .background(CityPopTheme.success.opacity(0.1))
        .cornerRadius(8)
    }
    
    // MARK: - Footer
    
    private var footer: some View {
        HStack {
            if CredentialManager.shared.loadGitHubPAT() != nil {
                Button(action: clearPAT) {
                    Text("Remove Token")
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
            
            Button(action: validateAndSave) {
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
        .padding(20)
        .background(CityPopTheme.surface)
    }
    
    // MARK: - Computed Properties
    
    private var isValid: Bool {
        (pat.hasPrefix("github_pat_") || pat.hasPrefix("ghp_")) && pat.count > 20
    }
    
    // MARK: - Actions
    
    private func loadExistingPAT() {
        if let existingPAT = CredentialManager.shared.loadGitHubPAT() {
            pat = existingPAT
            // Validate existing PAT
            validatePAT(existingPAT)
        }
    }
    
    private func openGitHubTokens() {
        if let url = URL(string: "https://github.com/settings/tokens?type=beta") {
            NSWorkspace.shared.open(url)
        }
    }
    
    private func clearPAT() {
        CredentialManager.shared.clearGitHubPAT()
        pat = ""
        validatedUser = nil
        MinnaEngineManager.shared.providerStates[.github] = .idle
    }
    
    private func validateAndSave() {
        guard isValid else { return }
        validatePAT(pat)
    }
    
    private func validatePAT(_ token: String) {
        isValidating = true
        validationError = nil
        validatedUser = nil
        
        var request = URLRequest(url: URL(string: "https://api.github.com/user")!)
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        request.setValue("2022-11-28", forHTTPHeaderField: "X-GitHub-Api-Version")
        
        URLSession.shared.dataTask(with: request) { data, response, error in
            DispatchQueue.main.async {
                isValidating = false
                
                if let error = error {
                    validationError = "Network error: \(error.localizedDescription)"
                    return
                }
                
                guard let httpResponse = response as? HTTPURLResponse else {
                    validationError = "Invalid response"
                    return
                }
                
                if httpResponse.statusCode == 401 {
                    validationError = "Invalid or expired token"
                    return
                }
                
                if httpResponse.statusCode != 200 {
                    validationError = "GitHub API error (status \(httpResponse.statusCode))"
                    return
                }
                
                guard let data = data,
                      let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                      let username = json["login"] as? String else {
                    validationError = "Could not parse GitHub response"
                    return
                }
                
                // Success!
                validatedUser = username
                
                // Save to Keychain
                CredentialManager.shared.saveGitHubPAT(token)
                
                MinnaEngineManager.shared.providerStates[.github] = .active
                MinnaEngineManager.shared.addSyncEvent(
                    for: .github,
                    type: .connected,
                    message: "Connected as @\(username)"
                )
                
                // Auto-close after brief delay to show success
                DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
                    onComplete()
                }
            }
        }.resume()
    }
}

// MARK: - Preview

#Preview {
    GitHubConfigSheet(onComplete: {}, onCancel: {})
}

