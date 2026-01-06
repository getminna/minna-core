import Foundation
import Security

/// Helper for secure token storage in macOS Keychain
struct KeychainHelper {
    
    /// Saves a token to the Keychain
    /// - Parameters:
    ///   - token: The token string to save
    ///   - service: The service identifier (e.g., "minna_ai")
    ///   - account: The account identifier (e.g., "slack_token")
    /// - Returns: True if save was successful
    @discardableResult
    static func save(token: String, service: String, account: String) -> Bool {
        guard let data = token.data(using: .utf8) else {
            print("❌ Keychain: Failed to encode token as UTF-8")
            return false
        }
        
        // First, try to delete any existing item
        let deleteQuery: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account
        ]
        SecItemDelete(deleteQuery as CFDictionary)
        
        // Now add the new item
        let addQuery: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleWhenUnlocked
        ]
        
        let status = SecItemAdd(addQuery as CFDictionary, nil)
        
        if status == errSecSuccess {
            print("✅ Keychain: Token saved for \(account)")
            return true
        } else {
            print("❌ Keychain: Failed to save token. Status: \(status)")
            return false
        }
    }
    
    /// Retrieves a token from the Keychain
    /// - Parameters:
    ///   - service: The service identifier
    ///   - account: The account identifier
    /// - Returns: The token string if found, nil otherwise
    static func load(service: String, account: String) -> String? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne
        ]
        
        var result: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        
        guard status == errSecSuccess,
              let data = result as? Data,
              let token = String(data: data, encoding: .utf8) else {
            return nil
        }
        
        return token
    }
    
    /// Deletes a token from the Keychain
    /// - Parameters:
    ///   - service: The service identifier
    ///   - account: The account identifier
    /// - Returns: True if deletion was successful
    @discardableResult
    static func delete(service: String, account: String) -> Bool {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account
        ]
        
        let status = SecItemDelete(query as CFDictionary)
        return status == errSecSuccess || status == errSecItemNotFound
    }
    
    /// Checks if a token exists in the Keychain
    /// - Parameters:
    ///   - service: The service identifier
    ///   - account: The account identifier
    /// - Returns: True if a token exists
    static func exists(service: String, account: String) -> Bool {
        return load(service: service, account: account) != nil
    }
}



