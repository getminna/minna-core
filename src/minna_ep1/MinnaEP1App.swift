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
        // #region InjectionNext setup
        #if DEBUG
        // Load InjectionNext bundle for hot reload
        let injectionPaths = [
            "/Applications/InjectionIII.app/Contents/Resources/macOSInjection.bundle",
            "/Applications/InjectionNext.app/Contents/Resources/macOSInjection.bundle"
        ]
        
        for bundlePath in injectionPaths {
            if let bundle = Bundle(path: bundlePath) {
                bundle.load()
                print("✓ InjectionNext bundle loaded from: \(bundlePath)")
                break
            }
        }
        #endif
        // #endregion
        
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
        .enableInjection()
    }

    #if DEBUG
    @ObserveInjection var forceRedraw
    #endif
}

#if canImport(HotSwiftUI)
@_exported import HotSwiftUI
#elseif canImport(Inject)
@_exported import Inject
#else
// This code can be found in the Swift package:
// https://github.com/johnno1962/HotSwiftUI

#if DEBUG
import Combine

private var loadInjectionOnce: () = {
        guard objc_getClass("InjectionClient") == nil else {
            return
        }
        #if os(macOS) || targetEnvironment(macCatalyst)
        let bundleName = "macOSInjection.bundle"
        #elseif os(tvOS)
        let bundleName = "tvOSInjection.bundle"
        #elseif os(visionOS)
        let bundleName = "xrOSInjection.bundle"
        #elseif targetEnvironment(simulator)
        let bundleName = "iOSInjection.bundle"
        #else
        let bundleName = "maciOSInjection.bundle"
        #endif
        let bundlePath = "/Applications/InjectionIII.app/Contents/Resources/"+bundleName
        guard let bundle = Bundle(path: bundlePath), bundle.load() else {
            return print("""
                ⚠️ Could not load injection bundle from \(bundlePath). \
                Have you downloaded the InjectionIII.app from either \
                https://github.com/johnno1962/InjectionIII/releases \
                or the Mac App Store?
                """)
        }
}()

public let injectionObserver = InjectionObserver()

public class InjectionObserver: ObservableObject {
    @Published var injectionNumber = 0
    var cancellable: AnyCancellable? = nil
    let publisher = PassthroughSubject<Void, Never>()
    init() {
        _ = loadInjectionOnce // .enableInjection() optional Xcode 16+
        cancellable = NotificationCenter.default.publisher(for:
            Notification.Name("INJECTION_BUNDLE_NOTIFICATION"))
            .sink { [weak self] change in
            self?.injectionNumber += 1
            self?.publisher.send()
        }
    }
}

extension SwiftUI.View {
    public func eraseToAnyView() -> some SwiftUI.View {
        _ = loadInjectionOnce
        return AnyView(self)
    }
    public func enableInjection() -> some SwiftUI.View {
        return eraseToAnyView()
    }
    public func loadInjection() -> some SwiftUI.View {
        return eraseToAnyView()
    }
    public func onInjection(bumpState: @escaping () -> ()) -> some SwiftUI.View {
        return self
            .onReceive(injectionObserver.publisher, perform: bumpState)
            .eraseToAnyView()
    }
}

@available(iOS 13.0, *)
@propertyWrapper
public struct ObserveInjection: DynamicProperty {
    @ObservedObject private var iO = injectionObserver
    public init() {}
    public private(set) var wrappedValue: Int {
        get {0} set {}
    }
}
#else
extension SwiftUI.View {
    @inline(__always)
    public func eraseToAnyView() -> some SwiftUI.View { return self }
    @inline(__always)
    public func enableInjection() -> some SwiftUI.View { return self }
    @inline(__always)
    public func loadInjection() -> some SwiftUI.View { return self }
    @inline(__always)
    public func onInjection(bumpState: @escaping () -> ()) -> some SwiftUI.View {
        return self
    }
}

@available(iOS 13.0, *)
@propertyWrapper
public struct ObserveInjection {
    public init() {}
    public private(set) var wrappedValue: Int {
        get {0} set {}
    }
}
#endif
#endif
