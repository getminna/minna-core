import SwiftUI
import AppKit
import UniformTypeIdentifiers
#if canImport(Inject)
import Inject
#endif

/// Google Workspace configuration with progressive disclosure
/// Reveals one step at a time, with JSON drop zone for easy credential import
struct ProviderConfigSheet: View {
    let provider: Provider
    let onComplete: () -> Void
    let onCancel: () -> Void
    
    @StateObject private var oauthManager = LocalOAuthManager.shared
    @State private var currentStep: SetupStep = .setup
    @State private var clientId: String = ""
    @State private var clientSecret: String = ""
    @State private var isLoading = false
    @State private var errorMessage: String?
    @State private var showCopiedFeedback = false
    
    // Progressive disclosure state
    @State private var completedSetupSteps: Set<Int> = []
    @State private var currentSetupStep: Int = 1
    @State private var expandedSteps: Set<Int> = []  // For show/hide on completed steps
    
    // JSON drop zone state
    @State private var isDropTargeted = false
    @State private var jsonLoadSuccess: Bool? = nil
    
    enum SetupStep {
        case setup
        case credentials
    }
    
    private var isValid: Bool {
        !clientId.trimmingCharacters(in: .whitespaces).isEmpty &&
        !clientSecret.trimmingCharacters(in: .whitespaces).isEmpty
    }
    
    private var allSetupStepsComplete: Bool {
        completedSetupSteps.count >= 4
    }
    
    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            
            if currentStep == .setup {
                setupStepView
            } else {
                credentialsStepView
            }
            
            Divider()
            footer
        }
        .frame(width: 560, height: 640)
        .background(CityPopTheme.background)
        .onAppear {
            loadExistingCredentials()
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
                Text("Google Workspace Configuration")
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundColor(CityPopTheme.textPrimary)
                
                Text("Sovereign Mode — Your keys, your data")
                    .font(.system(size: 12))
                    .foregroundColor(CityPopTheme.textSecondary)
            }
            
            Spacer()
            
            // Step indicator
            HStack(spacing: 8) {
                stepDot(step: 1, label: "Setup", isActive: currentStep == .setup, isComplete: allSetupStepsComplete)
                Rectangle().fill(CityPopTheme.border).frame(width: 20, height: 1)
                stepDot(step: 2, label: "Credentials", isActive: currentStep == .credentials, isComplete: false)
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
                // Why section (always visible)
                whySection
                
                // Progressive steps
                ForEach(1...4, id: \.self) { stepNumber in
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
        // Show if: it's the current step, or it's completed, or it's next after all completed
        step <= currentSetupStep || completedSetupSteps.contains(step)
    }
    
    @ViewBuilder
    private func stepView(for step: Int) -> some View {
        let isCompleted = completedSetupSteps.contains(step)
        let isCurrent = step == currentSetupStep && !isCompleted
        let isExpanded = expandedSteps.contains(step)
        
        if isCompleted && !isExpanded {
            // Collapsed completed step
            collapsedStepView(step: step)
        } else {
            // Active or expanded step
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
            // Step header
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
                    
                    // Step 4 special: Redirect URI box
                    if step == 4 {
                        redirectURIBox
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
    
    private var redirectURIBox: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("http://127.0.0.1:8847/callback")
                    .font(.system(size: 12, design: .monospaced))
                    .foregroundColor(CityPopTheme.textPrimary)
                
                Spacer()
                
                Button(action: copyRedirectURI) {
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
            .padding(10)
            .background(CityPopTheme.background)
            .cornerRadius(6)
            .overlay(
                RoundedRectangle(cornerRadius: 6)
                    .stroke(CityPopTheme.accent.opacity(0.5), lineWidth: 1)
            )
            
            HStack(spacing: 4) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .font(.system(size: 10))
                Text("Must be 'Web application' type — Desktop apps don't support redirect URIs")
                    .font(.system(size: 10))
            }
            .foregroundColor(CityPopTheme.warning)
        }
        .padding(.top, 4)
    }
    
    private var whySection: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 6) {
                Image(systemName: "lock.shield")
                    .foregroundColor(CityPopTheme.success)
                Text("Why create your own Google Cloud project?")
                    .font(.system(size: 13, weight: .medium))
            }
            
            Text("By using your own OAuth credentials, Google talks directly to Minna on your machine — no third-party servers involved. Your calendar, email, and documents stay completely private.")
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
        case 1: return "Create a Google Cloud Project"
        case 2: return "Enable the required APIs"
        case 3: return "Configure OAuth consent screen"
        case 4: return "Create OAuth 2.0 credentials"
        default: return ""
        }
    }
    
    private func stepInstructions(for step: Int) -> [String] {
        switch step {
        case 1:
            return [
                "Go to the Google Cloud Console",
                "Click 'Select a project' → 'New Project'",
                "Name it something like 'Minna Local'",
                "Click 'Create'"
            ]
        case 2:
            return [
                "In your project, go to 'APIs & Services' → 'Library'",
                "Search for and enable each of these APIs:",
                "  • Google Calendar API",
                "  • Gmail API",
                "  • Google Drive API",
                "(Drive API gives access to Docs, Sheets, Slides, and Meet transcripts)"
            ]
        case 3:
            return [
                "Go to 'APIs & Services' → 'OAuth consent screen'",
                "Choose 'External' (even for personal use)",
                "Fill in the app name (e.g., 'Minna')",
                "Add your email as a test user",
                "Save and continue through the screens"
            ]
        case 4:
            return [
                "Go to 'APIs & Services' → 'Credentials'",
                "Click 'Create Credentials' → 'OAuth client ID'",
                "Choose 'Web application' (not Desktop!)",
                "Name it 'Minna Local'",
                "Under 'Authorized redirect URIs', click 'Add URI'",
                "Paste this exact URI:"
            ]
        default:
            return []
        }
    }
    
    private func stepLink(for step: Int) -> (title: String, url: URL)? {
        switch step {
        case 1:
            return ("Open Google Cloud Console", URL(string: "https://console.cloud.google.com/projectcreate")!)
        case 2:
            return ("Open API Library", URL(string: "https://console.cloud.google.com/apis/library")!)
        case 3:
            return ("Open OAuth Consent", URL(string: "https://console.cloud.google.com/apis/credentials/consent")!)
        case 4:
            return ("Open Credentials", URL(string: "https://console.cloud.google.com/apis/credentials")!)
        default:
            return nil
        }
    }
    
    private func markStepComplete(_ step: Int) {
        withAnimation {
            completedSetupSteps.insert(step)
            if step < 4 {
                currentSetupStep = step + 1
            }
        }
    }
    
    // MARK: - Credentials Step View
    
    private var credentialsStepView: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                // JSON Drop Zone
                jsonDropZone
                
                // Divider
                HStack {
                    Rectangle().fill(CityPopTheme.border).frame(height: 1)
                    Text("or enter manually")
                        .font(.system(size: 11))
                        .foregroundColor(CityPopTheme.textMuted)
                        .padding(.horizontal, 12)
                    Rectangle().fill(CityPopTheme.border).frame(height: 1)
                }
                
                // Manual entry fields
                manualEntryFields
                
                // Error message
                if let error = errorMessage {
                    errorBanner(error)
                }
                
                // Security note
                securityNote
                
                // Access list
                accessList
            }
            .padding(24)
        }
    }
    
    private var jsonDropZone: some View {
        VStack(spacing: 12) {
            ZStack {
                RoundedRectangle(cornerRadius: 8)
                    .strokeBorder(
                        style: StrokeStyle(lineWidth: 2, dash: [6, 3])
                    )
                    .foregroundColor(isDropTargeted ? CityPopTheme.accent : CityPopTheme.border)
                    .background(
                        RoundedRectangle(cornerRadius: 8)
                            .fill(isDropTargeted ? CityPopTheme.accent.opacity(0.05) : CityPopTheme.surface)
                    )
                
                VStack(spacing: 8) {
                    if jsonLoadSuccess == true {
                        Image(systemName: "checkmark.circle.fill")
                            .font(.system(size: 24))
                            .foregroundColor(CityPopTheme.success)
                        Text("Credentials loaded!")
                            .font(.system(size: 13, weight: .medium))
                            .foregroundColor(CityPopTheme.success)
                    } else {
                        Image(systemName: "doc.badge.arrow.up")
                            .font(.system(size: 24))
                            .foregroundColor(CityPopTheme.textMuted)
                        Text("Drop credentials.json here")
                            .font(.system(size: 13, weight: .medium))
                            .foregroundColor(CityPopTheme.textPrimary)
                        Text("or click to browse")
                            .font(.system(size: 11))
                            .foregroundColor(CityPopTheme.textMuted)
                    }
                }
            }
            .frame(height: 100)
            .onDrop(of: [.fileURL], isTargeted: $isDropTargeted) { providers in
                handleFileDrop(providers: providers)
            }
            .onTapGesture {
                openFilePicker()
            }
            
            if jsonLoadSuccess == false {
                Text("Could not parse credentials from file. Make sure it's the JSON file downloaded from Google.")
                    .font(.system(size: 11))
                    .foregroundColor(CityPopTheme.error)
            }
        }
    }
    
    private var manualEntryFields: some View {
        VStack(alignment: .leading, spacing: 16) {
            // Client ID
            VStack(alignment: .leading, spacing: 6) {
                Text("Client ID")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundColor(CityPopTheme.textSecondary)
                
                TextField("e.g., 123456789-abc123.apps.googleusercontent.com", text: $clientId)
                    .textFieldStyle(.plain)
                    .font(.system(size: 12, design: .monospaced))
                    .padding(10)
                    .background(CityPopTheme.surface)
                    .cornerRadius(6)
                    .overlay(
                        RoundedRectangle(cornerRadius: 6)
                            .stroke(clientId.isEmpty ? CityPopTheme.border : CityPopTheme.success.opacity(0.5), lineWidth: 1)
                    )
            }
            
            // Client Secret
            VStack(alignment: .leading, spacing: 6) {
                Text("Client Secret")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundColor(CityPopTheme.textSecondary)
                
                SecureField("e.g., GOCSPX-xxxxxxxxxxxxx", text: $clientSecret)
                    .textFieldStyle(.plain)
                    .font(.system(size: 12, design: .monospaced))
                    .padding(10)
                    .background(CityPopTheme.surface)
                    .cornerRadius(6)
                    .overlay(
                        RoundedRectangle(cornerRadius: 6)
                            .stroke(clientSecret.isEmpty ? CityPopTheme.border : CityPopTheme.success.opacity(0.5), lineWidth: 1)
                    )
            }
        }
    }
    
    private func errorBanner(_ message: String) -> some View {
        HStack(spacing: 6) {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundColor(CityPopTheme.error)
            Text(message)
                .font(.system(size: 12))
                .foregroundColor(CityPopTheme.error)
        }
        .padding(12)
        .background(CityPopTheme.error.opacity(0.1))
        .cornerRadius(6)
    }
    
    private var securityNote: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Image(systemName: "lock.fill")
                    .font(.system(size: 10))
                    .foregroundColor(CityPopTheme.success)
                Text("Stored securely in macOS Keychain")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundColor(CityPopTheme.success)
            }
            
            Text("Your credentials never leave your machine. When you click 'Save & Connect', your browser will open to Google's login page. After you authorize, Google redirects back to Minna running locally.")
                .font(.system(size: 11))
                .foregroundColor(CityPopTheme.textMuted)
                .lineSpacing(3)
        }
        .padding(12)
        .background(CityPopTheme.success.opacity(0.1))
        .cornerRadius(8)
    }
    
    private var accessList: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Minna will request read-only access to:")
                .font(.system(size: 11, weight: .medium))
                .foregroundColor(CityPopTheme.textSecondary)
            
            VStack(alignment: .leading, spacing: 4) {
                accessItem("Calendar events and meetings")
                accessItem("Email messages (Gmail)")
                accessItem("Documents, Sheets, and Slides (Drive)")
                accessItem("Meet transcripts (stored in Drive)")
            }
        }
        .padding(12)
        .background(CityPopTheme.surface)
        .cornerRadius(8)
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(CityPopTheme.border, lineWidth: 1))
    }
    
    private func accessItem(_ text: String) -> some View {
        HStack(spacing: 6) {
            Image(systemName: "checkmark.circle.fill")
                .font(.system(size: 10))
                .foregroundColor(CityPopTheme.success)
            Text(text)
                .font(.system(size: 11))
                .foregroundColor(CityPopTheme.textSecondary)
        }
    }
    
    // MARK: - Footer
    
    private var footer: some View {
        HStack {
            if currentStep == .credentials && oauthManager.hasCredentials(for: provider) {
                Button(action: clearCredentials) {
                    Text("Clear Credentials")
                        .font(.system(size: 12))
                        .foregroundColor(CityPopTheme.error)
                }
                .buttonStyle(.plain)
            }
            
            Spacer()
            
            if currentStep == .setup {
                Button(action: { currentStep = .credentials }) {
                    Text("I've created my credentials →")
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
                
                Button(action: saveAndConnect) {
                    HStack(spacing: 6) {
                        if isLoading {
                            ProgressView()
                                .scaleEffect(0.7)
                                .frame(width: 12, height: 12)
                        }
                        Text(isLoading ? "Connecting..." : "Save & Connect")
                    }
                    .font(.system(size: 13, weight: .medium))
                    .foregroundColor(.white)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
                    .background(isValid ? CityPopTheme.accent : CityPopTheme.textMuted)
                    .cornerRadius(6)
                }
                .buttonStyle(.plain)
                .disabled(!isValid || isLoading)
            }
        }
        .padding(20)
        .background(CityPopTheme.surface)
    }
    
    // MARK: - JSON Parsing
    
    private func handleFileDrop(providers: [NSItemProvider]) -> Bool {
        guard let provider = providers.first else { return false }
        
        provider.loadItem(forTypeIdentifier: UTType.fileURL.identifier, options: nil) { item, error in
            guard let data = item as? Data,
                  let url = URL(dataRepresentation: data, relativeTo: nil) else {
                DispatchQueue.main.async {
                    self.jsonLoadSuccess = false
                }
                return
            }
            
            parseGoogleCredentialsFile(at: url)
        }
        
        return true
    }
    
    private func openFilePicker() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.json]
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        panel.message = "Select your Google OAuth credentials JSON file"
        
        if panel.runModal() == .OK, let url = panel.url {
            parseGoogleCredentialsFile(at: url)
        }
    }
    
    private func parseGoogleCredentialsFile(at url: URL) {
        do {
            let data = try Data(contentsOf: url)
            if let json = try JSONSerialization.jsonObject(with: data) as? [String: Any] {
                // Google exports as {"web": {...}} or {"installed": {...}}
                let credentials = json["web"] as? [String: Any] ?? json["installed"] as? [String: Any] ?? json
                
                if let parsedClientId = credentials["client_id"] as? String,
                   let parsedClientSecret = credentials["client_secret"] as? String {
                    DispatchQueue.main.async {
                        self.clientId = parsedClientId
                        self.clientSecret = parsedClientSecret
                        self.jsonLoadSuccess = true
                        
                        // Auto-advance to credentials step if still on setup
                        if self.currentStep == .setup {
                            // Mark all setup steps as complete since they have the file
                            self.completedSetupSteps = [1, 2, 3, 4]
                            self.currentSetupStep = 4
                        }
                    }
                    return
                }
            }
            
            DispatchQueue.main.async {
                self.jsonLoadSuccess = false
            }
        } catch {
            DispatchQueue.main.async {
                self.jsonLoadSuccess = false
            }
        }
    }
    
    // MARK: - Actions
    
    private func loadExistingCredentials() {
        if let creds = oauthManager.loadCredentials(for: provider) {
            clientId = creds.clientId
            clientSecret = creds.clientSecret
            // If credentials exist, mark all steps complete and go to credentials step
            completedSetupSteps = [1, 2, 3, 4]
            currentSetupStep = 4
            currentStep = .credentials
        }
    }
    
    private func copyRedirectURI() {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString("http://127.0.0.1:8847/callback", forType: .string)
        
        showCopiedFeedback = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
            showCopiedFeedback = false
        }
    }
    
    private func clearCredentials() {
        oauthManager.clearCredentials(for: provider)
        clientId = ""
        clientSecret = ""
        jsonLoadSuccess = nil
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

// MARK: - Preview

#Preview {
    ProviderConfigSheet(
        provider: .googleWorkspace,
        onComplete: {},
        onCancel: {}
    )
}
