import Foundation
import AppKit

// MARK: - Auth Types

/// Supported authentication patterns for Sovereign Mode
enum AuthType {
    /// OAuth with user-provided Client ID/Secret (Google)
    case oauth(clientId: String, clientSecret: String)
    
    /// Single static token (GitHub PAT)
    case staticToken(token: String)
    
    /// Dual tokens (Slack Bot + User tokens)
    case dualToken(bot: String, user: String)
}

/// Credential status for a provider
enum CredentialStatus {
    case notConfigured
    case configured
    case expired
    case error(String)
}

// MARK: - Credential Manager

/// Manages all provider credentials in Keychain
/// Supports multiple auth patterns: OAuth, Static Tokens, Dual Tokens
class CredentialManager {
    static let shared = CredentialManager()
    
    private let keychainService = "minna_ai"
    
    // MARK: - Generic Credential Access
    
    /// Get credential status for a provider
    func status(for provider: Provider) -> CredentialStatus {
        switch provider {
        case .slack:
            // Slack needs both bot and user tokens
            let botToken = KeychainHelper.load(service: keychainService, account: "slack_bot_token")
            let userToken = KeychainHelper.load(service: keychainService, account: "slack_user_token")
            if botToken != nil || userToken != nil {
                return .configured
            }
            return .notConfigured
            
        case .googleWorkspace:
            // Google needs OAuth credentials + access token
            let accessToken = KeychainHelper.load(service: keychainService, account: "googleWorkspace_token")
            let clientId = KeychainHelper.load(service: keychainService, account: "googleWorkspace_client_id")
            if accessToken != nil && clientId != nil {
                return .configured
            } else if clientId != nil {
                return .expired // Has credentials but no token
            }
            return .notConfigured
            
        case .github:
            // GitHub just needs PAT
            let pat = KeychainHelper.load(service: keychainService, account: "github_pat")
            return pat != nil ? .configured : .notConfigured

        case .cursor:
            // Local AI tool reads local files - always configured
            return .configured
        }
    }
    
    /// Check if provider is ready to sync
    func isReady(for provider: Provider) -> Bool {
        // Cursor doesn't need auth
        if !provider.requiresAuth {
            return true
        }
        if case .configured = status(for: provider) {
            return true
        }
        return false
    }
    
    // MARK: - Slack Credentials
    
    func saveSlackTokens(botToken: String?, userToken: String?) {
        if let bot = botToken, !bot.isEmpty {
            KeychainHelper.save(token: bot, service: keychainService, account: "slack_bot_token")
        }
        if let user = userToken, !user.isEmpty {
            KeychainHelper.save(token: user, service: keychainService, account: "slack_user_token")
        }
    }
    
    func loadSlackTokens() -> (bot: String?, user: String?) {
        let bot = KeychainHelper.load(service: keychainService, account: "slack_bot_token")
        let user = KeychainHelper.load(service: keychainService, account: "slack_user_token")
        return (bot, user)
    }
    
    func clearSlackTokens() {
        KeychainHelper.delete(service: keychainService, account: "slack_bot_token")
        KeychainHelper.delete(service: keychainService, account: "slack_user_token")
    }
    
    // MARK: - GitHub Credentials
    
    func saveGitHubPAT(_ pat: String) {
        KeychainHelper.save(token: pat, service: keychainService, account: "github_pat")
    }
    
    func loadGitHubPAT() -> String? {
        KeychainHelper.load(service: keychainService, account: "github_pat")
    }
    
    func clearGitHubPAT() {
        KeychainHelper.delete(service: keychainService, account: "github_pat")
    }
    
    // MARK: - Google Credentials (delegated to LocalOAuthManager)
    
    func clearAllCredentials(for provider: Provider) {
        switch provider {
        case .slack:
            clearSlackTokens()
        case .github:
            clearGitHubPAT()
        case .googleWorkspace:
            LocalOAuthManager.shared.clearCredentials(for: provider)
        case .cursor:
            // Local AI tool has no credentials to clear - reads local files
            break
        }
    }
}

// MARK: - Local OAuth Manager

/// Handles OAuth flows locally without requiring a server bridge.
/// Uses loopback redirect (127.0.0.1) for secure local token exchange.
///
/// Flow:
/// 1. User provides their own Client ID + Secret (stored in Keychain)
/// 2. App opens browser to Google auth URL
/// 3. Google redirects to http://127.0.0.1:PORT/callback
/// 4. Local HTTP server catches the callback
/// 5. App exchanges code for tokens directly with Google
/// 6. Tokens stored in Keychain for Python worker
class LocalOAuthManager: ObservableObject {
    static let shared = LocalOAuthManager()
    
    // MARK: - Published State
    
    @Published var isAuthenticating = false
    @Published var authError: String?
    
    // MARK: - Constants
    
    private let keychainService = "minna_ai"
    private let loopbackPort: UInt16 = 8847  // Random high port for loopback
    
    // Google OAuth endpoints
    private let googleAuthURL = "https://accounts.google.com/o/oauth2/v2/auth"
    private let googleTokenURL = "https://oauth2.googleapis.com/token"
    
    // Scopes we need
    // Google OAuth scopes - read-only access to workspace data
    // Drive scope includes Docs, Sheets, Slides, and Meet transcripts
    private let googleScopes = [
        "https://www.googleapis.com/auth/calendar.readonly",
        "https://www.googleapis.com/auth/gmail.readonly",
        "https://www.googleapis.com/auth/drive.readonly",
        "https://www.googleapis.com/auth/userinfo.email",
        "https://www.googleapis.com/auth/userinfo.profile"
    ].joined(separator: " ")
    
    // MARK: - Keychain Keys
    
    private func clientIdKey(for provider: Provider) -> String {
        "\(provider.rawValue)_client_id"
    }
    
    private func clientSecretKey(for provider: Provider) -> String {
        "\(provider.rawValue)_client_secret"
    }
    
    // MARK: - Credential Management
    
    /// Check if user has configured credentials for a provider
    func hasCredentials(for provider: Provider) -> Bool {
        let clientId = KeychainHelper.load(service: keychainService, account: clientIdKey(for: provider))
        let clientSecret = KeychainHelper.load(service: keychainService, account: clientSecretKey(for: provider))
        return clientId != nil && clientSecret != nil
    }
    
    /// Save user-provided credentials
    func saveCredentials(clientId: String, clientSecret: String, for provider: Provider) {
        KeychainHelper.save(token: clientId, service: keychainService, account: clientIdKey(for: provider))
        KeychainHelper.save(token: clientSecret, service: keychainService, account: clientSecretKey(for: provider))
        print("✅ Saved \(provider.displayName) credentials to Keychain")
    }
    
    /// Load credentials from Keychain
    func loadCredentials(for provider: Provider) -> (clientId: String, clientSecret: String)? {
        guard let clientId = KeychainHelper.load(service: keychainService, account: clientIdKey(for: provider)),
              let clientSecret = KeychainHelper.load(service: keychainService, account: clientSecretKey(for: provider)) else {
            return nil
        }
        return (clientId, clientSecret)
    }
    
    /// Clear credentials
    func clearCredentials(for provider: Provider) {
        KeychainHelper.delete(service: keychainService, account: clientIdKey(for: provider))
        KeychainHelper.delete(service: keychainService, account: clientSecretKey(for: provider))
        KeychainHelper.delete(service: keychainService, account: provider.keychainAccount)
        KeychainHelper.delete(service: keychainService, account: "\(provider.rawValue)_refresh_token")
        print("🗑️ Cleared \(provider.displayName) credentials from Keychain")
    }
    
    // MARK: - OAuth Flow
    
    /// Start the OAuth flow for Google Workspace
    func startGoogleOAuth(completion: @escaping (Result<Void, Error>) -> Void) {
        guard let credentials = loadCredentials(for: .googleWorkspace) else {
            completion(.failure(OAuthError.noCredentials))
            return
        }
        
        DispatchQueue.main.async {
            self.isAuthenticating = true
            self.authError = nil
        }
        
        // Start local HTTP server to catch the callback
        startLoopbackServer { [weak self] result in
            guard let self = self else { return }
            
            switch result {
            case .success(let code):
                // Exchange code for tokens
                self.exchangeCodeForTokens(
                    code: code,
                    clientId: credentials.clientId,
                    clientSecret: credentials.clientSecret
                ) { tokenResult in
                    DispatchQueue.main.async {
                        self.isAuthenticating = false
                        switch tokenResult {
                        case .success:
                            completion(.success(()))
                        case .failure(let error):
                            self.authError = error.localizedDescription
                            completion(.failure(error))
                        }
                    }
                }
                
            case .failure(let error):
                DispatchQueue.main.async {
                    self.isAuthenticating = false
                    self.authError = error.localizedDescription
                    completion(.failure(error))
                }
            }
        }
        
        // Open browser to Google auth
        let redirectURI = "http://127.0.0.1:\(loopbackPort)/callback"
        let state = UUID().uuidString
        
        var components = URLComponents(string: googleAuthURL)!
        components.queryItems = [
            URLQueryItem(name: "client_id", value: credentials.clientId),
            URLQueryItem(name: "redirect_uri", value: redirectURI),
            URLQueryItem(name: "response_type", value: "code"),
            URLQueryItem(name: "scope", value: googleScopes),
            URLQueryItem(name: "access_type", value: "offline"),
            URLQueryItem(name: "prompt", value: "consent"),
            URLQueryItem(name: "state", value: state)
        ]
        
        if let url = components.url {
            NSWorkspace.shared.open(url)
        }
    }
    
    // MARK: - Loopback Server
    
    private var serverSocket: CFSocket?
    private var serverCompletion: ((Result<String, Error>) -> Void)?
    
    private func startLoopbackServer(completion: @escaping (Result<String, Error>) -> Void) {
        serverCompletion = completion
        
        // Create socket
        var context = CFSocketContext(version: 0, info: Unmanaged.passUnretained(self).toOpaque(), retain: nil, release: nil, copyDescription: nil)
        
        serverSocket = CFSocketCreate(
            kCFAllocatorDefault,
            PF_INET,
            SOCK_STREAM,
            IPPROTO_TCP,
            CFSocketCallBackType.acceptCallBack.rawValue,
            { (socket, callbackType, address, data, info) in
                guard let info = info else { return }
                let manager = Unmanaged<LocalOAuthManager>.fromOpaque(info).takeUnretainedValue()
                manager.handleIncomingConnection(socket: socket, data: data)
            },
            &context
        )
        
        guard let socket = serverSocket else {
            completion(.failure(OAuthError.serverFailed))
            return
        }
        
        // Bind to loopback address
        var addr = sockaddr_in()
        addr.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
        addr.sin_family = sa_family_t(AF_INET)
        addr.sin_port = loopbackPort.bigEndian
        addr.sin_addr.s_addr = inet_addr("127.0.0.1")
        
        let addressData = Data(bytes: &addr, count: MemoryLayout<sockaddr_in>.size) as CFData
        
        let result = CFSocketSetAddress(socket, addressData)
        if result != .success {
            completion(.failure(OAuthError.serverFailed))
            return
        }
        
        // Add to run loop
        let source = CFSocketCreateRunLoopSource(kCFAllocatorDefault, socket, 0)
        CFRunLoopAddSource(CFRunLoopGetCurrent(), source, .defaultMode)
        
        print("🌐 Loopback server started on port \(loopbackPort)")
        
        // Timeout after 5 minutes
        DispatchQueue.main.asyncAfter(deadline: .now() + 300) { [weak self] in
            if self?.isAuthenticating == true {
                self?.stopServer()
                self?.serverCompletion?(.failure(OAuthError.timeout))
            }
        }
    }
    
    private func handleIncomingConnection(socket: CFSocket?, data: UnsafeRawPointer?) {
        guard let data = data else { return }
        
        let handle = data.assumingMemoryBound(to: CFSocketNativeHandle.self).pointee
        let fileHandle = FileHandle(fileDescriptor: handle, closeOnDealloc: true)
        
        // Read the HTTP request
        let requestData = fileHandle.availableData
        guard let request = String(data: requestData, encoding: .utf8) else {
            return
        }
        
        // Parse the callback URL
        if let codeRange = request.range(of: "code=([^&\\s]+)", options: .regularExpression),
           let code = request[codeRange].split(separator: "=").last {
            
            // Send success response to browser
            let response = """
            HTTP/1.1 200 OK\r
            Content-Type: text/html; charset=utf-8\r
            Connection: close\r
            \r
            <html>
            <head><meta charset="utf-8"><title>Minna - Connected!</title></head>
            <body style="font-family: -apple-system, sans-serif; display: flex; justify-content: center; align-items: center; height: 100vh; margin: 0; background: #f5f5f5;">
                <div style="text-align: center;">
                    <h1 style="color: #333;">✅ Connected!</h1>
                    <p style="color: #666;">You can close this window and return to Minna.</p>
                </div>
            </body>
            </html>
            """
            
            fileHandle.write(response.data(using: .utf8)!)
            
            // Stop server and return code
            stopServer()
            serverCompletion?(.success(String(code)))
            
        } else if request.contains("error=") {
            // Handle OAuth error
            let response = """
            HTTP/1.1 200 OK\r
            Content-Type: text/html; charset=utf-8\r
            Connection: close\r
            \r
            <html>
            <head><meta charset="utf-8"><title>Minna - Error</title></head>
            <body style="font-family: -apple-system, sans-serif; display: flex; justify-content: center; align-items: center; height: 100vh; margin: 0; background: #fff5f5;">
                <div style="text-align: center;">
                    <h1 style="color: #c00;">❌ Authorization Failed</h1>
                    <p style="color: #666;">Please try again from the Minna app.</p>
                </div>
            </body>
            </html>
            """
            
            fileHandle.write(response.data(using: .utf8)!)
            
            stopServer()
            serverCompletion?(.failure(OAuthError.denied))
        }
    }
    
    private func stopServer() {
        if let socket = serverSocket {
            CFSocketInvalidate(socket)
            serverSocket = nil
        }
        print("🛑 Loopback server stopped")
    }
    
    // MARK: - Token Exchange
    
    private func exchangeCodeForTokens(
        code: String,
        clientId: String,
        clientSecret: String,
        completion: @escaping (Result<Void, Error>) -> Void
    ) {
        let redirectURI = "http://127.0.0.1:\(loopbackPort)/callback"
        
        var request = URLRequest(url: URL(string: googleTokenURL)!)
        request.httpMethod = "POST"
        request.setValue("application/x-www-form-urlencoded", forHTTPHeaderField: "Content-Type")
        
        let body = [
            "client_id": clientId,
            "client_secret": clientSecret,
            "code": code,
            "grant_type": "authorization_code",
            "redirect_uri": redirectURI
        ]
        
        request.httpBody = body
            .map { "\($0.key)=\($0.value.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? $0.value)" }
            .joined(separator: "&")
            .data(using: .utf8)
        
        URLSession.shared.dataTask(with: request) { [weak self] data, response, error in
            if let error = error {
                completion(.failure(error))
                return
            }
            
            guard let data = data,
                  let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let accessToken = json["access_token"] as? String else {
                completion(.failure(OAuthError.tokenExchangeFailed))
                return
            }
            
            let refreshToken = json["refresh_token"] as? String
            
            // Save tokens to Keychain
            self?.saveTokens(accessToken: accessToken, refreshToken: refreshToken, for: .googleWorkspace)
            
            completion(.success(()))
        }.resume()
    }
    
    private func saveTokens(accessToken: String, refreshToken: String?, for provider: Provider) {
        KeychainHelper.save(token: accessToken, service: keychainService, account: provider.keychainAccount)
        
        if let refreshToken = refreshToken {
            KeychainHelper.save(token: refreshToken, service: keychainService, account: "\(provider.rawValue)_refresh_token")
        }
        
        print("✅ Saved \(provider.displayName) tokens to Keychain")
    }
    
    // MARK: - Token Refresh
    
    /// Refresh an expired access token
    func refreshGoogleToken(completion: @escaping (Result<String, Error>) -> Void) {
        guard let credentials = loadCredentials(for: .googleWorkspace),
              let refreshToken = KeychainHelper.load(service: keychainService, account: "googleWorkspace_refresh_token") else {
            completion(.failure(OAuthError.noCredentials))
            return
        }
        
        var request = URLRequest(url: URL(string: googleTokenURL)!)
        request.httpMethod = "POST"
        request.setValue("application/x-www-form-urlencoded", forHTTPHeaderField: "Content-Type")
        
        let body = [
            "client_id": credentials.clientId,
            "client_secret": credentials.clientSecret,
            "refresh_token": refreshToken,
            "grant_type": "refresh_token"
        ]
        
        request.httpBody = body
            .map { "\($0.key)=\($0.value.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? $0.value)" }
            .joined(separator: "&")
            .data(using: .utf8)
        
        URLSession.shared.dataTask(with: request) { [weak self] data, response, error in
            if let error = error {
                completion(.failure(error))
                return
            }
            
            guard let data = data,
                  let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let accessToken = json["access_token"] as? String else {
                completion(.failure(OAuthError.tokenExchangeFailed))
                return
            }
            
            // Save new access token
            self?.saveTokens(accessToken: accessToken, refreshToken: nil, for: .googleWorkspace)
            
            // If Google rotated the refresh token, save that too
            if let newRefreshToken = json["refresh_token"] as? String {
                KeychainHelper.save(token: newRefreshToken, service: self?.keychainService ?? "minna_ai", account: "googleWorkspace_refresh_token")
            }
            
            completion(.success(accessToken))
        }.resume()
    }
}

// MARK: - Errors

enum OAuthError: LocalizedError {
    case noCredentials
    case serverFailed
    case timeout
    case denied
    case tokenExchangeFailed
    
    var errorDescription: String? {
        switch self {
        case .noCredentials: return "No OAuth credentials configured"
        case .serverFailed: return "Failed to start local auth server"
        case .timeout: return "Authorization timed out"
        case .denied: return "Authorization was denied"
        case .tokenExchangeFailed: return "Failed to exchange code for tokens"
        }
    }
}

