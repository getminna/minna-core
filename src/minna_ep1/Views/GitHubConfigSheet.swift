import SwiftUI
import AppKit
#if canImport(Inject)
import Inject
#endif

/// GitHub configuration with progressive disclosure
/// Two-step wizard: Setup (5 collapsible steps) → Token input
struct GitHubConfigSheet: View {
    let onComplete: () -> Void
    let onCancel: () -> Void
    
    // Wizard state
    @State private var currentStep: SetupStep = .setup
    
    // Progressive disclosure state
    @State private var completedSetupSteps: Set<Int> = []
    @State private var currentSetupStep: Int = 1
    @State private var expandedSteps: Set<Int> = []
    
    // Token state
    @State private var pat: String = ""
    @State private var isValidating = false
    @State private var validationError: String?
    @State private var validatedUser: String?
    
    enum SetupStep {
        case setup
        case token
    }
    
    private var isValid: Bool {
        (pat.hasPrefix("github_pat_") || pat.hasPrefix("ghp_")) && pat.count > 20
    }
    
    private var allSetupStepsComplete: Bool {
        completedSetupSteps.count >= 5
    }
    
    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            
            if currentStep == .setup {
                setupStepView
            } else {
                tokenStepView
            }
            
            Divider()
            footer
        }
        .frame(width: 560, height: 640)
        .background(CityPopTheme.background)
        .onAppear {
            loadExistingPAT()
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
                Text("GitHub Configuration")
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundColor(CityPopTheme.textPrimary)
                
                Text("Sovereign Mode — Personal Access Token")
                    .font(.system(size: 12))
                    .foregroundColor(CityPopTheme.textSecondary)
            }
            
            Spacer()
            
            // Step indicator
            HStack(spacing: 8) {
                stepDot(step: 1, label: "Setup", isActive: currentStep == .setup, isComplete: allSetupStepsComplete)
                Rectangle().fill(CityPopTheme.border).frame(width: 20, height: 1)
                stepDot(step: 2, label: "Token", isActive: currentStep == .token, isComplete: false)
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
    
    private func stepDot(step: Int, label: String, isActive: Bool, isComplete: Bool) -> some View {
        VStack(spacing: 4) {
            Circle()
                .fill(isComplete ? CityPopTheme.success : (isActive ? CityPopTheme.accent : CityPopTheme.border))
                .frame(width: 24, height: 24)
                .overlay(
                    Group {
                        if isComplete {
                            Image(systemName: "checkmark")
                                .font(.system(size: 11, weight: .bold))
                                .foregroundColor(.white)
                        } else {
                            Text("\(step)")
                                .font(.system(size: 11, weight: .bold))
                                .foregroundColor(isActive ? .white : CityPopTheme.textMuted)
                        }
                    }
                )
            
            Text(label)
                .font(.system(size: 10))
                .foregroundColor(isActive || isComplete ? CityPopTheme.textPrimary : CityPopTheme.textMuted)
        }
    }
    
    // MARK: - Setup Step View (Progressive Disclosure)
    
    private var setupStepView: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                // Why section
                whySection
                
                // Progressive steps
                ForEach(1...5, id: \.self) { stepNumber in
                    if shouldShowStep(stepNumber) {
                        stepView(for: stepNumber)
                            .transition(.opacity.combined(with: .move(edge: .top)))
                    }
                }
            }
            .padding(24)
            .animation(.easeInOut(duration: 0.3), value: currentSetupStep)
            .animation(.easeInOut(duration: 0.3), value: completedSetupSteps)
            .animation(.easeInOut(duration: 0.3), value: expandedSteps)
        }
    }
    
    private func shouldShowStep(_ step: Int) -> Bool {
        step <= currentSetupStep || completedSetupSteps.contains(step)
    }
    
    @ViewBuilder
    private func stepView(for step: Int) -> some View {
        let isCompleted = completedSetupSteps.contains(step)
        let isCurrent = step == currentSetupStep && !isCompleted
        let isExpanded = expandedSteps.contains(step)
        
        if isCompleted && !isExpanded {
            collapsedStepView(step: step)
        } else {
            activeStepView(step: step, isCurrent: isCurrent)
        }
    }
    
    private func collapsedStepView(step: Int) -> some View {
        HStack {
            Image(systemName: "checkmark.circle.fill")
                .font(.system(size: 14))
                .foregroundColor(CityPopTheme.success)
            
            Text("\(step). \(stepTitle(for: step))")
                .font(.system(size: 13, weight: .medium))
                .foregroundColor(CityPopTheme.textPrimary)
            
            Spacer()
            
            Button(action: { expandedSteps.insert(step) }) {
                Text("Show")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundColor(CityPopTheme.accent)
            }
            .buttonStyle(.plain)
        }
        .padding(12)
        .background(CityPopTheme.success.opacity(0.05))
        .cornerRadius(8)
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(CityPopTheme.success.opacity(0.2), lineWidth: 1))
    }
    
    @ViewBuilder
    private func activeStepView(step: Int, isCurrent: Bool) -> some View {
        let isExpanded = expandedSteps.contains(step)
        let isCompleted = completedSetupSteps.contains(step)
        
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .top, spacing: 12) {
                // Step number badge
                Text("\(step)")
                    .font(.system(size: 10, weight: .bold))
                    .foregroundColor(.white)
                    .frame(width: 20, height: 20)
                    .background(isCurrent ? CityPopTheme.accent : CityPopTheme.success)
                    .cornerRadius(10)
                
                VStack(alignment: .leading, spacing: 8) {
                    Text(stepTitle(for: step))
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundColor(CityPopTheme.textPrimary)
                    
                    // Instructions
                    VStack(alignment: .leading, spacing: 4) {
                        ForEach(stepInstructions(for: step), id: \.self) { instruction in
                            Text(instruction)
                                .font(.system(size: 12))
                                .foregroundColor(CityPopTheme.textSecondary)
                        }
                    }
                    
                    // Step 4 special: Permissions list
                    if step == 4 {
                        permissionsList
                    }
                    
                    // Step 5 special: Keychain reassurance
                    if step == 5 {
                        keychainNote
                    }
                    
                    // Link button
                    if let link = stepLink(for: step) {
                        Button(action: { NSWorkspace.shared.open(link.url) }) {
                            HStack(spacing: 4) {
                                Text(link.title)
                                    .font(.system(size: 11, weight: .medium))
                                Image(systemName: "arrow.up.right")
                                    .font(.system(size: 9))
                            }
                            .foregroundColor(CityPopTheme.accent)
                        }
                        .buttonStyle(.plain)
                    }
                    
                    // Action buttons
                    HStack {
                        Spacer()
                        
                        if isExpanded && isCompleted {
                            Button(action: { expandedSteps.remove(step) }) {
                                Text("Hide")
                                    .font(.system(size: 11, weight: .medium))
                                    .foregroundColor(CityPopTheme.textSecondary)
                                    .padding(.horizontal, 12)
                                    .padding(.vertical, 6)
                            }
                            .buttonStyle(.plain)
                        }
                        
                        if isCurrent {
                            Button(action: { markStepComplete(step) }) {
                                Text("Mark as Done")
                                    .font(.system(size: 11, weight: .medium))
                                    .foregroundColor(.white)
                                    .padding(.horizontal, 12)
                                    .padding(.vertical, 6)
                                    .background(CityPopTheme.accent)
                                    .cornerRadius(4)
                            }
                            .buttonStyle(.plain)
                        }
                    }
                    .padding(.top, 4)
                }
            }
        }
        .padding(16)
        .background(CityPopTheme.surface)
        .cornerRadius(8)
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .stroke(isCurrent ? CityPopTheme.accent.opacity(0.3) : CityPopTheme.border, lineWidth: 1)
        )
    }
    
    private var permissionsList: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach([
                ("Contents", "Read access to code and files"),
                ("Issues", "Read access to issues"),
                ("Pull requests", "Read access to PRs and comments"),
                ("Metadata", "Read access to repo metadata (usually auto-selected)")
            ], id: \.0) { scope, description in
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
        }
        .padding(12)
        .background(CityPopTheme.background)
        .cornerRadius(6)
    }
    
    private var keychainNote: some View {
        HStack(spacing: 8) {
            Image(systemName: "lock.shield.fill")
                .font(.system(size: 14))
                .foregroundColor(CityPopTheme.success)
            
            VStack(alignment: .leading, spacing: 2) {
                Text("Token stored securely")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundColor(CityPopTheme.success)
                Text("Your token is saved in macOS Keychain. You can view it anytime in Minna's connector settings.")
                    .font(.system(size: 10))
                    .foregroundColor(CityPopTheme.textMuted)
            }
        }
        .padding(10)
        .background(CityPopTheme.success.opacity(0.1))
        .cornerRadius(6)
    }
    
    private var whySection: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 6) {
                Image(systemName: "key.fill")
                    .foregroundColor(CityPopTheme.success)
                Text("Why a Personal Access Token?")
                    .font(.system(size: 13, weight: .medium))
            }
            
            Text("PATs are the standard for developer tools. They give you granular control over what Minna can access — specific repos, read-only permissions. You can revoke access anytime from GitHub.")
                .font(.system(size: 12))
                .foregroundColor(CityPopTheme.textSecondary)
                .lineSpacing(4)
        }
        .padding(16)
        .background(CityPopTheme.surface)
        .cornerRadius(8)
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(CityPopTheme.border, lineWidth: 1))
    }
    
    // MARK: - Step Data
    
    private func stepTitle(for step: Int) -> String {
        switch step {
        case 1: return "Go to GitHub Token Settings"
        case 2: return "Configure Token Basics"
        case 3: return "Set Repository Access"
        case 4: return "Add Repository Permissions"
        case 5: return "Generate & Copy Token"
        default: return ""
        }
    }
    
    private func stepInstructions(for step: Int) -> [String] {
        switch step {
        case 1:
            return [
                "Go to GitHub → Settings (click your avatar)",
                "Scroll down to \"Developer settings\" in the sidebar",
                "Select \"Personal access tokens\" → \"Fine-grained tokens\"",
                "Click \"Generate new token\""
            ]
        case 2:
            return [
                "Token name: \"Minna AI\" (or any name you'll recognize)",
                "Expiration: 90 days recommended (or longer)",
                "Resource owner: Your account or org with the repos you want to sync"
            ]
        case 3:
            return [
                "Choose \"All repositories\" for easiest setup",
                "Or \"Only select repositories\" to limit access to specific repos"
            ]
        case 4:
            return [
                "Expand \"Repository permissions\" (NOT Account permissions)",
                "Set these permissions to \"Read-only\":"
            ]
        case 5:
            return [
                "Click the green \"Generate token\" button",
                "Copy the token immediately — GitHub only shows it once!"
            ]
        default:
            return []
        }
    }
    
    private func stepLink(for step: Int) -> (title: String, url: URL)? {
        switch step {
        case 1:
            return ("Open GitHub Token Settings", URL(string: "https://github.com/settings/tokens?type=beta")!)
        default:
            return nil
        }
    }
    
    private func markStepComplete(_ step: Int) {
        withAnimation {
            completedSetupSteps.insert(step)
            if step < 5 {
                currentSetupStep = step + 1
            }
        }
    }
    
    // MARK: - Token Step View
    
    private var tokenStepView: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                // Instructions
                VStack(alignment: .leading, spacing: 12) {
                    Text("Paste your token below:")
                        .font(.system(size: 13, weight: .medium))
                        .foregroundColor(CityPopTheme.textPrimary)
                    
                    Text("The token starts with github_pat_ (fine-grained) or ghp_ (classic)")
                        .font(.system(size: 12))
                        .foregroundColor(CityPopTheme.textSecondary)
                }
                
                // Token input
                VStack(alignment: .leading, spacing: 6) {
                    Text("Personal Access Token")
                        .font(.system(size: 12, weight: .medium))
                        .foregroundColor(CityPopTheme.textSecondary)
                    
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
                            validatedUser = nil
                            validationError = nil
                        }
                }
                
                // Error/Success banners
                if let error = validationError {
                    errorBanner(error)
                }
                
                if let user = validatedUser {
                    successBanner(user)
                }
                
                // Security note
                securityNote
            }
            .padding(24)
        }
    }
    
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
            
            Text("Your PAT is encrypted by the system and never leaves your machine. You can view or copy it from Minna's connector settings, or revoke it anytime from GitHub.")
                .font(.system(size: 11))
                .foregroundColor(CityPopTheme.textMuted)
                .lineSpacing(3)
        }
        .padding(12)
        .background(CityPopTheme.success.opacity(0.1))
        .cornerRadius(8)
    }
    
    // MARK: - Footer
    
    private var footer: some View {
        HStack {
            if currentStep == .token && CredentialManager.shared.loadGitHubPAT() != nil {
                Button(action: clearPAT) {
                    Text("Remove Token")
                        .font(.system(size: 12))
                        .foregroundColor(CityPopTheme.error)
                }
                .buttonStyle(.plain)
            }
            
            Spacer()
            
            if currentStep == .setup {
                Button(action: { currentStep = .token }) {
                    Text("I've created my token →")
                        .font(.system(size: 13, weight: .medium))
                        .foregroundColor(.white)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 8)
                        .background(allSetupStepsComplete ? CityPopTheme.accent : CityPopTheme.textMuted)
                        .cornerRadius(6)
                }
                .buttonStyle(.plain)
                .disabled(!allSetupStepsComplete)
            } else {
                Button(action: { currentStep = .setup }) {
                    Text("← Back")
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
        }
        .padding(20)
        .background(CityPopTheme.surface)
    }
    
    // MARK: - Actions
    
    private func loadExistingPAT() {
        if let existingPAT = CredentialManager.shared.loadGitHubPAT() {
            pat = existingPAT
            // Mark all steps complete and go to token step
            completedSetupSteps = [1, 2, 3, 4, 5]
            currentSetupStep = 5
            currentStep = .token
            // Validate existing PAT
            validatePAT(existingPAT)
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
