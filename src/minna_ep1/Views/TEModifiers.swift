import SwiftUI

// MARK: - Status Indicator

struct StatusIndicator: View {
    let status: SyncStatus
    
    var body: some View {
        Circle()
            .fill(statusColor)
            .frame(width: 8, height: 8)
    }
    
    private var statusColor: Color {
        switch status {
        case .syncing: return CityPopTheme.syncing
        case .active: return CityPopTheme.success
        case .error: return CityPopTheme.error
        case .idle: return CityPopTheme.textMuted
        }
    }
}

// MARK: - Provider Icon (1px Line Glyphs)

struct ProviderIcon: View {
    let provider: Provider
    let size: CGFloat
    
    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: size * 0.2)
                .fill(CityPopTheme.providerColor(for: provider.displayName).opacity(0.08))
                .frame(width: size, height: size)
            
            Image(systemName: brandIcon)
                .font(.system(size: size * 0.4, weight: .light)) // Thin 1px line weight
                .foregroundColor(CityPopTheme.providerColor(for: provider.displayName))
        }
    }
    
    private var brandIcon: String {
        switch provider {
        case .slack: return "bubble.left.and.bubble.right"  // Conversation/channels
        case .googleWorkspace: return "calendar"             // Calendar is core to GWS
        case .github: return "arrow.triangle.branch"         // Git branching
        }
    }
}

// MARK: - Sync Progress View

struct SyncProgressView: View {
    let progress: SyncProgress
    
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            // Progress bar with City Pop sunset gradient
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    RoundedRectangle(cornerRadius: 2)
                        .fill(CityPopTheme.divider)
                        .frame(height: 4)
                    
                    RoundedRectangle(cornerRadius: 2)
                        .fill(CityPopTheme.progressGradient)
                        .frame(width: max(geo.size.width * progress.percentage, 4), height: 4)
                }
            }
            .frame(height: 4)
            
            HStack {
                Text(progress.currentAction)
                    .font(.system(size: 12))
                    .foregroundColor(CityPopTheme.textSecondary)
                
                Spacer()
                
                Text("\(progress.documentsProcessed) docs")
                    .font(.system(size: 11, weight: .medium, design: .monospaced))
                    .foregroundColor(CityPopTheme.textMuted)
            }
        }
    }
}

struct SyncProgress {
    let documentsProcessed: Int
    let totalDocuments: Int?
    let currentAction: String
    
    var percentage: Double {
        if let total = totalDocuments, total > 0 {
            return min(Double(documentsProcessed) / Double(total), 1.0)
        }
        return 0.5
    }
}

// MARK: - Action Button (simplified)

struct ActionButton: View {
    let title: String
    let icon: String?
    let style: ButtonVariant
    let action: () -> Void
    
    enum ButtonVariant {
        case primary
        case secondary
        case destructive
    }
    
    var body: some View {
        Button(action: action) {
            HStack(spacing: 5) {
                if let icon = icon {
                    Image(systemName: icon)
                        .font(.system(size: 11, weight: .medium))
                }
                Text(title)
                    .font(.system(size: 12, weight: .medium))
            }
            .foregroundColor(foregroundColor)
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .background(
                RoundedRectangle(cornerRadius: 6)
                    .fill(backgroundColor)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 6)
                    .stroke(borderColor, lineWidth: 1)
            )
        }
        .buttonStyle(.plain)
    }
    
    private var foregroundColor: Color {
        switch style {
        case .primary: return .white
        case .secondary: return CityPopTheme.textSecondary
        case .destructive: return CityPopTheme.error
        }
    }
    
    private var backgroundColor: Color {
        switch style {
        case .primary: return CityPopTheme.accent
        case .secondary, .destructive: return Color.clear
        }
    }
    
    private var borderColor: Color {
        switch style {
        case .primary: return Color.clear
        case .secondary: return CityPopTheme.border
        case .destructive: return CityPopTheme.error.opacity(0.3)
        }
    }
}

// MARK: - Section Header

struct SectionHeader: View {
    let title: String
    let subtitle: String?
    
    init(_ title: String, subtitle: String? = nil) {
        self.title = title
        self.subtitle = subtitle
    }
    
    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
                .font(.system(size: 12, weight: .medium))
                .foregroundColor(CityPopTheme.textMuted)
                .textCase(.uppercase)
            
            if let subtitle = subtitle {
                Text(subtitle)
                    .font(.system(size: 13))
                    .foregroundColor(CityPopTheme.textSecondary)
            }
        }
    }
}

// MARK: - Nav Item (legacy - kept for compatibility)

struct NavItem: View {
    let title: String
    let icon: String
    let isSelected: Bool
    let action: () -> Void
    
    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                Image(systemName: icon)
                    .font(.system(size: 14, weight: .medium))
                    .frame(width: 20)
                    .foregroundColor(isSelected ? CityPopTheme.accent : CityPopTheme.textSecondary)
                
                Text(title)
                    .font(.system(size: 13, weight: isSelected ? .semibold : .regular))
                    .foregroundColor(isSelected ? CityPopTheme.textPrimary : CityPopTheme.textSecondary)
                
                Spacer()
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .background(isSelected ? CityPopTheme.surface : Color.clear)
            .cornerRadius(6)
        }
        .buttonStyle(.plain)
    }
}
