import SwiftUI

/// Linear-inspired Daily Briefing View
/// Automated executive intelligence pulled from the local vault
struct BriefingView: View {
    @State private var briefing = DailyBriefing.sample
    @State private var expandedMeetings: Set<UUID> = []
    @State private var showShareMenu = false
    @State private var showCopyConfirmation = false
    
    private let exporter = BriefingExporter.shared
    
    var body: some View {
        VStack(spacing: 0) {
            // Header with share button
            header
            
            // Scrollable content
            ScrollView {
                VStack(spacing: 16) {
                    // Date block
                    dateBlock
                    
                    // Meetings section
                    meetingsSection
                    
                    // Awaiting from others
                    awaitingFromOthersSection
                    
                    // Others awaiting from me
                    othersAwaitingSection
                    
                    // Proactive insights
                    insightsSection
                    
                    // Footer
                    footer
                }
                .padding(.horizontal, 24)
                .padding(.bottom, 24)
            }
        }
        .background(CityPopTheme.background)
    }
    
    // MARK: - Header
    
    private var header: some View {
        HStack {
            VStack(alignment: .leading, spacing: 4) {
                Text("Daily Briefing")
                    .font(.system(size: 18, weight: .semibold))
                    .foregroundColor(CityPopTheme.textPrimary)
                
                Text(briefing.dateString)
                    .font(.system(size: 13))
                    .foregroundColor(CityPopTheme.textSecondary)
            }
            
            Spacer()
            
            // Copy confirmation
            if showCopyConfirmation {
                HStack(spacing: 4) {
                    Image(systemName: "checkmark.circle.fill")
                        .font(.system(size: 12))
                    Text("Copied!")
                        .font(.system(size: 12, weight: .medium))
                }
                .foregroundColor(CityPopTheme.success)
                .transition(.opacity)
            }
            
            // Share button
            Button(action: { showShareMenu.toggle() }) {
                HStack(spacing: 4) {
                    Image(systemName: "square.and.arrow.up")
                        .font(.system(size: 12))
                    Text("Share")
                        .font(.system(size: 12, weight: .medium))
                }
                .foregroundColor(CityPopTheme.textSecondary)
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
                .background(CityPopTheme.surface)
                .cornerRadius(6)
                .overlay(
                    RoundedRectangle(cornerRadius: 6)
                        .stroke(CityPopTheme.border, lineWidth: 1)
                )
            }
            .buttonStyle(.plain)
            .popover(isPresented: $showShareMenu) {
                ShareMenu(briefing: briefing) {
                    withAnimation {
                        showCopyConfirmation = true
                    }
                    DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
                        withAnimation {
                            showCopyConfirmation = false
                        }
                    }
                }
            }
        }
        .padding(.horizontal, 24)
        .padding(.top, 24)
        .padding(.bottom, 20)
    }
    
    // MARK: - Date Block
    
    private var dateBlock: some View {
        HStack(spacing: 20) {
            // Quick stats
            StatCard(value: "\(briefing.meetings.count)", label: "meetings", color: CityPopTheme.accentCyan)
            StatCard(value: "\(briefing.awaitingFromOthers.count)", label: "awaiting", color: CityPopTheme.syncing)
            StatCard(value: "\(briefing.othersAwaitingFromMe.count)", label: "to do", color: CityPopTheme.accent)
        }
    }
    
    // MARK: - Meetings Section
    
    private var meetingsSection: some View {
        BriefingSection(title: "Meetings", count: briefing.meetings.count) {
            VStack(spacing: 0) {
                ForEach(briefing.meetings) { meeting in
                    MeetingRow(
                        meeting: meeting,
                        isExpanded: expandedMeetings.contains(meeting.id),
                        onToggle: {
                            withAnimation(.easeOut(duration: 0.15)) {
                                if expandedMeetings.contains(meeting.id) {
                                    expandedMeetings.remove(meeting.id)
                                } else {
                                    expandedMeetings.insert(meeting.id)
                                }
                            }
                        }
                    )
                    
                    if meeting.id != briefing.meetings.last?.id {
                        Rectangle().fill(CityPopTheme.divider).frame(height: 1).padding(.leading, 62)
                    }
                }
            }
        }
    }
    
    // MARK: - Awaiting From Others
    
    private var awaitingFromOthersSection: some View {
        BriefingSection(title: "Awaiting from others", count: briefing.awaitingFromOthers.count) {
            VStack(spacing: 0) {
                ForEach(briefing.awaitingFromOthers) { item in
                    TrackedItemRow(item: item, style: .awaiting)
                    
                    if item.id != briefing.awaitingFromOthers.last?.id {
                        Rectangle().fill(CityPopTheme.divider).frame(height: 1).padding(.leading, 16)
                    }
                }
            }
        }
    }
    
    // MARK: - Others Awaiting From Me
    
    private var othersAwaitingSection: some View {
        BriefingSection(title: "Others awaiting from me", count: briefing.othersAwaitingFromMe.count) {
            VStack(spacing: 0) {
                ForEach(briefing.othersAwaitingFromMe) { item in
                    TrackedItemRow(item: item, style: .owed)
                    
                    if item.id != briefing.othersAwaitingFromMe.last?.id {
                        Rectangle().fill(CityPopTheme.divider).frame(height: 1).padding(.leading, 16)
                    }
                }
            }
        }
    }
    
    // MARK: - Proactive Insights
    
    private var insightsSection: some View {
        BriefingSection(title: "Insights", count: nil) {
            VStack(spacing: 8) {
                ForEach(briefing.proactiveInsights) { insight in
                    InsightCard(insight: insight)
                }
            }
        }
    }
    
    // MARK: - Footer
    
    private var footer: some View {
        HStack(spacing: 6) {
            Image(systemName: "sparkles")
                .font(.system(size: 10))
            Text("Generated by Minna · \(briefing.generatedTimestamp)")
                .font(CityPopTheme.Fonts.caption(size: 10))
        }
        .foregroundColor(CityPopTheme.textMuted)
        .padding(20)
    }
}

// MARK: - Briefing Section (Linear-style)

struct BriefingSection<Content: View>: View {
    let title: String
    let count: Int?
    let content: () -> Content
    
    init(title: String, count: Int?, @ViewBuilder content: @escaping () -> Content) {
        self.title = title
        self.count = count
        self.content = content
    }
    
    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Section header
            HStack {
                Text(title)
                    .font(.system(size: 12, weight: .medium))
                    .foregroundColor(CityPopTheme.textMuted)
                
                Spacer()
                
                if let count = count {
                    Text("\(count)")
                        .font(.system(size: 12, weight: .medium))
                        .foregroundColor(CityPopTheme.textMuted)
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
            
            // Content
            content()
                .padding(.horizontal, 16)
                .padding(.bottom, 12)
        }
        .background(CityPopTheme.surface)
        .cornerRadius(8)
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .stroke(CityPopTheme.border, lineWidth: 1)
        )
    }
}

// MARK: - Stat Card

struct StatCard: View {
    let value: String
    let label: String
    let color: Color
    
    var body: some View {
        VStack(spacing: 4) {
            Text(value)
                .font(.system(size: 24, weight: .semibold))
                .foregroundColor(color)
            
            Text(label)
                .font(.system(size: 11, weight: .medium))
                .foregroundColor(CityPopTheme.textMuted)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 16)
        .background(CityPopTheme.surface)
        .cornerRadius(8)
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .stroke(CityPopTheme.border, lineWidth: 1)
        )
    }
}

// MARK: - Meeting Row

struct MeetingRow: View {
    let meeting: MeetingWithPrep
    let isExpanded: Bool
    let onToggle: () -> Void
    
    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Main row
            HStack(alignment: .top, spacing: 12) {
                // Time column
                Text(meeting.timeString)
                    .font(.system(size: 13, weight: .medium, design: .monospaced))
                    .foregroundColor(CityPopTheme.textSecondary)
                    .frame(width: 50, alignment: .trailing)
                
                // Meeting details
                VStack(alignment: .leading, spacing: 4) {
                    Text(meeting.title)
                        .font(.system(size: 14, weight: .medium))
                        .foregroundColor(CityPopTheme.textPrimary)
                    
                    Text("\(meeting.attendeesString) · \(meeting.durationString)")
                        .font(.system(size: 12))
                        .foregroundColor(CityPopTheme.textSecondary)
                }
                
                Spacer()
                
                // Expand button
                Button(action: onToggle) {
                    Image(systemName: isExpanded ? "chevron.up" : "chevron.down")
                        .font(.system(size: 10, weight: .medium))
                        .foregroundColor(CityPopTheme.textMuted)
                }
                .buttonStyle(.plain)
            }
            .padding(.vertical, 10)
            
            // Prep notes (expandable)
            if isExpanded {
                Text(meeting.prepNotes)
                    .font(.system(size: 12))
                    .foregroundColor(CityPopTheme.textSecondary)
                    .padding(12)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(CityPopTheme.background)
                    .cornerRadius(6)
                    .padding(.leading, 62)
                    .padding(.bottom, 8)
            }
        }
    }
}

// MARK: - Prep Notes Card (legacy)

struct PrepNotesCard: View {
    let notes: String
    
    var body: some View {
        Text(notes)
            .font(.system(size: 12))
            .foregroundColor(CityPopTheme.textSecondary)
            .padding(12)
            .background(CityPopTheme.background)
            .cornerRadius(6)
    }
}

// MARK: - Tracked Item Row

struct TrackedItemRow: View {
    let item: TrackedItem
    let style: ItemStyle
    
    enum ItemStyle {
        case awaiting
        case owed
    }
    
    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            // Status indicator
            Circle()
                .fill(item.isOverdue ? CityPopTheme.error : statusColor)
                .frame(width: 6, height: 6)
                .padding(.top, 6)
            
            VStack(alignment: .leading, spacing: 4) {
                // Title
                Text(item.title)
                    .font(.system(size: 13, weight: .medium))
                    .foregroundColor(CityPopTheme.textPrimary)
                
                // Meta row
                HStack(spacing: 4) {
                    Text(item.owner)
                    Text("·")
                    Text(item.context)
                    if let due = item.dueDateString {
                        Text("·")
                        Text(due)
                            .foregroundColor(item.isOverdue ? CityPopTheme.error : CityPopTheme.textMuted)
                    }
                }
                .font(.system(size: 11))
                .foregroundColor(CityPopTheme.textMuted)
            }
            
            Spacer()
        }
        .padding(.vertical, 8)
    }
    
    private var statusColor: Color {
        switch item.riskLevel {
        case .low: return CityPopTheme.success
        case .medium: return CityPopTheme.syncing
        case .high: return CityPopTheme.error
        }
    }
}

// MARK: - Insight Card

struct InsightCard: View {
    let insight: Insight
    
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(insight.summary)
                .font(.system(size: 13))
                .foregroundColor(CityPopTheme.textPrimary)
            
            if let relevantMeeting = insight.relevantMeeting {
                Text("→ \(relevantMeeting)")
                    .font(.system(size: 11))
                    .foregroundColor(CityPopTheme.accent)
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(CityPopTheme.background)
        .cornerRadius(6)
    }
}

// MARK: - Share Menu

struct ShareMenu: View {
    @Environment(\.dismiss) var dismiss
    let briefing: DailyBriefing
    let onCopy: () -> Void
    
    private let exporter = BriefingExporter.shared
    
    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("Share Briefing")
                .font(CityPopTheme.Fonts.caption(size: 11))
                .foregroundColor(CityPopTheme.textMuted)
                .padding(.horizontal, 12)
                .padding(.top, 12)
                .padding(.bottom, 4)
            
            Divider()
            
            ShareMenuItem(icon: "doc.richtext", title: "Export as HTML") {
                _ = exporter.exportAsHTML(briefing)
                dismiss()
            }
            
            ShareMenuItem(icon: "doc.fill", title: "Export as PDF") {
                let exportView = BriefingExportView(briefing: briefing)
                exporter.exportAsPDF(briefing, from: exportView)
                dismiss()
            }
            
            ShareMenuItem(icon: "photo", title: "Copy as Image") {
                let exportView = BriefingExportView(briefing: briefing)
                exporter.copyAsImage(briefing, from: exportView)
                onCopy()
                dismiss()
            }
            
            ShareMenuItem(icon: "doc.on.clipboard", title: "Copy as Text") {
                exporter.exportAsText(briefing)
                onCopy()
                dismiss()
            }
        }
        .padding(.bottom, 8)
        .frame(width: 180)
    }
}

struct ShareMenuItem: View {
    let icon: String
    let title: String
    let action: () -> Void
    
    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                Image(systemName: icon)
                    .font(.system(size: 13))
                    .frame(width: 20)
                
                Text(title)
                    .font(CityPopTheme.Fonts.body(size: 13))
                
                Spacer()
            }
            .foregroundColor(CityPopTheme.textPrimary)
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .background(Color.clear)
        .cornerRadius(4)
    }
}

// MARK: - Export View (for Image/PDF rendering)

struct BriefingExportView: View {
    let briefing: DailyBriefing
    
    var body: some View {
        VStack(spacing: 0) {
            // Date block
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 0) {
                    Text(briefing.dayOfWeek)
                        .font(.system(size: 13, weight: .medium))
                        .foregroundColor(CityPopTheme.textMuted)
                    
                    Text(briefing.dayNumber)
                        .font(.system(size: 48, weight: .light))
                        .foregroundColor(CityPopTheme.textPrimary)
                    
                    Rectangle()
                        .fill(CityPopTheme.accent)
                        .frame(width: 40, height: 2)
                    
                    Text(briefing.monthYear)
                        .font(.system(size: 11, weight: .medium, design: .monospaced))
                        .foregroundColor(CityPopTheme.textMuted)
                        .padding(.top, 6)
                }
                Spacer()
            }
            .padding(20)
            .background(CityPopTheme.surface)
            
            // Meetings
            if !briefing.meetings.isEmpty {
                exportSection(title: "TODAY'S MEETINGS", count: briefing.meetings.count) {
                    ForEach(briefing.meetings) { meeting in
                        ExportMeetingRow(meeting: meeting)
                    }
                }
            }
            
            // Awaiting from others
            if !briefing.awaitingFromOthers.isEmpty {
                exportSection(title: "AWAITING FROM OTHERS", count: briefing.awaitingFromOthers.count) {
                    ForEach(briefing.awaitingFromOthers) { item in
                        ExportItemRow(item: item, showRisk: true)
                    }
                }
            }
            
            // Others awaiting
            if !briefing.othersAwaitingFromMe.isEmpty {
                exportSection(title: "OTHERS AWAITING FROM ME", count: briefing.othersAwaitingFromMe.count) {
                    ForEach(briefing.othersAwaitingFromMe) { item in
                        ExportItemRow(item: item, showRisk: false)
                    }
                }
            }
            
            // Insights
            if !briefing.proactiveInsights.isEmpty {
                exportSection(title: "INSIGHTS", count: nil) {
                    ForEach(briefing.proactiveInsights) { insight in
                        ExportInsightCard(insight: insight)
                    }
                }
            }
            
            // Footer
            HStack {
                Text("Generated by Minna · \(briefing.generatedTimestamp)")
                    .font(.system(size: 10))
                    .foregroundColor(CityPopTheme.textMuted)
            }
            .frame(maxWidth: .infinity)
            .padding(16)
        }
        .background(CityPopTheme.background)
    }
    
    private func exportSection<Content: View>(title: String, count: Int?, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                Text(title)
                    .font(.system(size: 10, weight: .semibold, design: .monospaced))
                    .foregroundColor(CityPopTheme.textMuted)
                
                Rectangle()
                    .fill(CityPopTheme.border)
                    .frame(height: 1)
                
                if let count = count {
                    Text("\(count)")
                        .font(.system(size: 10, weight: .medium, design: .monospaced))
                        .foregroundColor(CityPopTheme.textMuted)
                }
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 10)
            
            VStack(spacing: 0) {
                content()
            }
            .padding(.horizontal, 20)
            .padding(.bottom, 12)
        }
        .background(CityPopTheme.surface)
        .padding(.top, 1)
    }
}

struct ExportMeetingRow: View {
    let meeting: MeetingWithPrep
    
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .top, spacing: 10) {
                Text(meeting.timeString)
                    .font(.system(size: 12, weight: .medium, design: .monospaced))
                    .foregroundColor(CityPopTheme.textPrimary)
                    .frame(width: 40, alignment: .trailing)
                
                VStack(alignment: .leading, spacing: 2) {
                    Text(meeting.title)
                        .font(.system(size: 13, weight: .medium))
                        .foregroundColor(CityPopTheme.textPrimary)
                    
                    Text("\(meeting.attendeesString) · \(meeting.durationString)")
                        .font(.system(size: 10))
                        .foregroundColor(CityPopTheme.textSecondary)
                }
            }
            
            Text("📋 PREP: \(meeting.prepNotes)")
                .font(.system(size: 10))
                .foregroundColor(CityPopTheme.textSecondary)
                .padding(8)
                .background(CityPopTheme.accentCyan.opacity(0.08))
                .cornerRadius(4)
                .padding(.leading, 50)
        }
        .padding(.vertical, 8)
    }
}

struct ExportItemRow: View {
    let item: TrackedItem
    let showRisk: Bool
    
    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 4) {
                if item.isOverdue {
                    Text("⚠️")
                        .font(.system(size: 10))
                }
                Text(item.title)
                    .font(.system(size: 12, weight: .medium))
                    .foregroundColor(CityPopTheme.textPrimary)
            }
            
            HStack(spacing: 4) {
                Text(item.owner)
                Text("·")
                Text(item.context)
                if let due = item.dueDateString {
                    Text("·")
                    Text(due)
                }
            }
            .font(.system(size: 10))
            .foregroundColor(CityPopTheme.textMuted)
            
            if showRisk, let activity = item.lastActivity {
                Text("\(item.riskLevel.emoji) \(item.riskLevel.rawValue) — \(activity)")
                    .font(.system(size: 10))
                    .foregroundColor(CityPopTheme.textMuted)
            }
        }
        .padding(.vertical, 6)
    }
}

struct ExportInsightCard: View {
    let insight: Insight
    
    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(insight.summary)
                .font(.system(size: 11))
                .foregroundColor(CityPopTheme.textPrimary)
            
            if let meeting = insight.relevantMeeting {
                Text("→ \(meeting)")
                    .font(.system(size: 10))
                    .foregroundColor(CityPopTheme.accent)
            }
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(CityPopTheme.accent.opacity(0.06))
        .cornerRadius(4)
        .padding(.bottom, 8)
    }
}

// MARK: - Preview

struct BriefingView_Previews: PreviewProvider {
    static var previews: some View {
        BriefingView()
            .frame(width: 600, height: 800)
    }
}

