import SwiftUI
import AppKit

/// Minna - Local Context Engine
/// A clean, professional macOS app for managing workspace data sync
@main
struct MinnaEP1App: App {
    @NSApplicationDelegateAdaptor(EP1AppDelegate.self) var appDelegate
    
    var body: some Scene {
        WindowGroup {
            ControlCenterView()
                .frame(minWidth: 720, minHeight: 480)
                .onOpenURL { url in
                    MinnaEngineManager.shared.handleDeepLink(url: url)
                }
        }
        .defaultSize(width: 800, height: 520)
        .windowResizability(.contentMinSize)
        
        Settings {
            SettingsView()
        }
    }
}

// MARK: - App Delegate

class EP1AppDelegate: NSObject, NSApplicationDelegate {
    
    func applicationDidFinishLaunching(_ notification: Notification) {
        // Register for custom URL scheme events (minna://)
        NSAppleEventManager.shared().setEventHandler(
            self,
            andSelector: #selector(handleGetURL(_:withReplyEvent:)),
            forEventClass: AEEventClass(kInternetEventClass),
            andEventID: AEEventID(kAEGetURL)
        )
        
        print("✓ Minna started")
    }
    
    func applicationWillTerminate(_ notification: Notification) {
        MinnaEngineManager.shared.cancelAllSyncs()
    }
    
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        return true
    }
    
    @objc func handleGetURL(_ event: NSAppleEventDescriptor, withReplyEvent reply: NSAppleEventDescriptor) {
        guard let urlString = event.paramDescriptor(forKeyword: AEKeyword(keyDirectObject))?.stringValue,
              let url = URL(string: urlString) else {
            return
        }
        
        MinnaEngineManager.shared.handleDeepLink(url: url)
        NSApplication.shared.activate(ignoringOtherApps: true)
    }
}

// MARK: - Settings View

struct SettingsView: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("Minna Settings")
                .font(CityPopTheme.Fonts.heading(size: 18))
            
            Divider()
            
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    Text("Version")
                        .foregroundColor(CityPopTheme.textSecondary)
                    Spacer()
                    Text("0.1.0")
                        .font(CityPopTheme.Fonts.mono(size: 13))
                        .foregroundColor(CityPopTheme.textMuted)
                }
            }
            
            Spacer()
        }
        .padding(24)
        .frame(width: 360, height: 200)
    }
}
