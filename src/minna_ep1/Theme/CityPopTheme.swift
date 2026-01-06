import SwiftUI

/// Minna Design System
/// Linear-inspired clean aesthetic with City Pop accent colors
struct CityPopTheme {
    
    // MARK: - Core Colors (Clean & Minimal)
    
    /// Main background - very subtle warm gray
    static let background = Color(red: 0.98, green: 0.98, blue: 0.97)
    
    /// Card/surface background - white
    static let surface = Color.white
    
    /// Sidebar background - slightly darker than main
    static let sidebarBackground = Color(red: 0.96, green: 0.96, blue: 0.95)
    
    /// Primary text - near black
    static let textPrimary = Color(red: 0.1, green: 0.1, blue: 0.12)
    
    /// Secondary text - medium gray
    static let textSecondary = Color(red: 0.45, green: 0.45, blue: 0.48)
    
    /// Muted text - light gray
    static let textMuted = Color(red: 0.65, green: 0.65, blue: 0.68)
    
    // MARK: - City Pop Accent Colors (Saturation-Consistent)
    // Per MediBang City Pop guide: colors at same saturation/lightness for cohesion
    
    /// Primary accent - hot pink (baseline ~85% saturation)
    static let accent = Color(red: 0.98, green: 0.32, blue: 0.5)
    
    /// Coral accent - for gradients (pairs with pink)
    static let accentCoral = Color(red: 1.0, green: 0.55, blue: 0.4)
    
    /// Cyan accent - adjusted to match pink saturation
    static let accentCyan = Color(red: 0.0, green: 0.78, blue: 0.82)
    
    /// Success state - adjusted for saturation consistency
    static let success = Color(red: 0.2, green: 0.75, blue: 0.55)
    
    /// Syncing/warning state - warm gold
    static let syncing = Color(red: 0.96, green: 0.68, blue: 0.2)
    
    /// Error state
    static let error = Color(red: 0.92, green: 0.32, blue: 0.32)
    
    // MARK: - City Pop Gradients
    
    /// Sunset gradient for progress bars (pink to coral)
    static let progressGradient = LinearGradient(
        colors: [accent, accentCoral],
        startPoint: .leading,
        endPoint: .trailing
    )
    
    /// Subtle sky gradient for backgrounds (aqua + blue per City Pop guide)
    static let skyGradient = LinearGradient(
        colors: [
            Color(red: 0.94, green: 0.97, blue: 0.98),  // Pale sky
            Color(red: 0.88, green: 0.94, blue: 0.97)   // Slightly deeper
        ],
        startPoint: .top,
        endPoint: .bottom
    )
    
    // MARK: - Status Dot Gradients (City Pop personality)
    
    /// Connected/success - green to cyan
    static let statusConnectedGradient = LinearGradient(
        colors: [
            Color(red: 0.2, green: 0.75, blue: 0.55),   // Success green
            Color(red: 0.0, green: 0.78, blue: 0.82)    // Cyan
        ],
        startPoint: .topLeading,
        endPoint: .bottomTrailing
    )
    
    /// Syncing - orange to gold  
    static let statusSyncingGradient = LinearGradient(
        colors: [
            Color(red: 0.96, green: 0.68, blue: 0.2),   // Warm orange
            Color(red: 1.0, green: 0.82, blue: 0.3)     // Gold
        ],
        startPoint: .topLeading,
        endPoint: .bottomTrailing
    )
    
    /// Error - red to coral
    static let statusErrorGradient = LinearGradient(
        colors: [
            Color(red: 0.92, green: 0.32, blue: 0.32),  // Error red
            Color(red: 1.0, green: 0.55, blue: 0.4)     // Coral
        ],
        startPoint: .topLeading,
        endPoint: .bottomTrailing
    )
    
    // MARK: - Borders & Dividers
    
    /// Subtle border - very light
    static let border = Color(red: 0.92, green: 0.92, blue: 0.91)
    
    /// Divider - even lighter
    static let divider = Color(red: 0.94, green: 0.94, blue: 0.93)
    
    // MARK: - Provider Colors (Subtle)
    
    static func providerColor(for provider: String) -> Color {
        switch provider.lowercased() {
        case "slack":
            return Color(red: 0.88, green: 0.24, blue: 0.48) // Slack-ish pink
        case "google workspace", "google":
            return Color(red: 0.26, green: 0.52, blue: 0.96) // Google blue
        case "github":
            return Color(red: 0.15, green: 0.15, blue: 0.18) // GitHub dark
        default:
            return accent
        }
    }
    
    // MARK: - Typography
    
    struct Fonts {
        /// Headings - medium weight
        static func heading(size: CGFloat = 16) -> Font {
            return .system(size: size, weight: .semibold, design: .default)
        }
        
        /// Body text
        static func body(size: CGFloat = 14) -> Font {
            return .system(size: size, weight: .regular, design: .default)
        }
        
        /// Small labels
        static func caption(size: CGFloat = 12) -> Font {
            return .system(size: size, weight: .medium, design: .default)
        }
        
        /// Monospace for data
        static func mono(size: CGFloat = 12) -> Font {
            return .system(size: size, weight: .regular, design: .monospaced)
        }
    }
    
    // MARK: - Shadows
    
    /// Subtle card shadow
    static let cardShadow = Color.black.opacity(0.04)
    static let cardShadowRadius: CGFloat = 8
    
    // MARK: - Dark Mode Palette (Blue + Purple + Black per City Pop nighttime guide)
    // Foundation for future dark mode implementation
    
    struct DarkMode {
        static let background = Color(red: 0.08, green: 0.08, blue: 0.12)      // Deep night
        static let surface = Color(red: 0.12, green: 0.12, blue: 0.18)         // Card dark
        static let accent = Color(red: 0.7, green: 0.4, blue: 0.9)             // Purple glow
        static let accentCyan = Color(red: 0.3, green: 0.8, blue: 0.9)         // Neon cyan
        static let textPrimary = Color(red: 0.95, green: 0.95, blue: 0.97)     // Near white
        static let textSecondary = Color(red: 0.6, green: 0.6, blue: 0.65)     // Muted
        static let textMuted = Color(red: 0.4, green: 0.4, blue: 0.45)         // Very muted
        static let border = Color(red: 0.2, green: 0.2, blue: 0.25)            // Subtle border
    }
}

// MARK: - Simple Card Style

struct CardStyle: ViewModifier {
    var padding: CGFloat = 16
    
    func body(content: Content) -> some View {
        content
            .padding(padding)
            .background(CityPopTheme.surface)
            .cornerRadius(8)
            .overlay(
                RoundedRectangle(cornerRadius: 8)
                    .stroke(CityPopTheme.border, lineWidth: 1)
            )
    }
}

extension View {
    func cardStyle(padding: CGFloat = 16) -> some View {
        modifier(CardStyle(padding: padding))
    }
}
