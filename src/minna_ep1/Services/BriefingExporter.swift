import SwiftUI
import AppKit

/// Handles exporting daily briefings to various formats
class BriefingExporter {
    static let shared = BriefingExporter()
    
    // MARK: - Export as HTML
    
    func exportAsHTML(_ briefing: DailyBriefing) -> URL? {
        let html = generateHTML(briefing)
        
        // Save to temp file
        let filename = "Minna_Briefing_\(briefing.dayNumber)_\(briefing.monthYear.replacingOccurrences(of: " ", with: "_")).html"
        let tempURL = FileManager.default.temporaryDirectory.appendingPathComponent(filename)
        
        do {
            try html.write(to: tempURL, atomically: true, encoding: .utf8)
            
            // Show save panel
            let savePanel = NSSavePanel()
            savePanel.allowedContentTypes = [.html]
            savePanel.nameFieldStringValue = filename
            savePanel.title = "Export Briefing as HTML"
            
            if savePanel.runModal() == .OK, let url = savePanel.url {
                try html.write(to: url, atomically: true, encoding: .utf8)
                return url
            }
        } catch {
            print("Failed to export HTML: \(error)")
        }
        
        return nil
    }
    
    // MARK: - Export as Text (Markdown)
    
    func exportAsText(_ briefing: DailyBriefing) {
        let markdown = generateMarkdown(briefing)
        
        // Copy to clipboard
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(markdown, forType: .string)
    }
    
    // MARK: - Export as PDF
    
    @MainActor
    func exportAsPDF(_ briefing: DailyBriefing, from view: some View) {
        let renderer = ImageRenderer(content: view.frame(width: 600))
        renderer.scale = 2.0
        
        guard let nsImage = renderer.nsImage else { return }
        
        let savePanel = NSSavePanel()
        savePanel.allowedContentTypes = [.pdf]
        savePanel.nameFieldStringValue = "Minna_Briefing_\(briefing.dayNumber).pdf"
        savePanel.title = "Export Briefing as PDF"
        
        if savePanel.runModal() == .OK, let url = savePanel.url {
            // Create PDF from image
            let imageRect = CGRect(origin: .zero, size: nsImage.size)
            
            let pdfData = NSMutableData()
            guard let consumer = CGDataConsumer(data: pdfData as CFMutableData),
                  let context = CGContext(consumer: consumer, mediaBox: nil, nil) else { return }
            
            let mediaBox = imageRect
            context.beginPDFPage([kCGPDFContextMediaBox as String: NSValue(rect: mediaBox)] as CFDictionary)
            
            if let cgImage = nsImage.cgImage(forProposedRect: nil, context: nil, hints: nil) {
                context.draw(cgImage, in: imageRect)
            }
            
            context.endPDFPage()
            context.closePDF()
            
            try? pdfData.write(to: url, options: .atomic)
        }
    }
    
    // MARK: - Export as Image
    
    @MainActor
    func copyAsImage(_ briefing: DailyBriefing, from view: some View) {
        let renderer = ImageRenderer(content: view.frame(width: 600))
        renderer.scale = 2.0
        
        guard let nsImage = renderer.nsImage else { return }
        
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.writeObjects([nsImage])
    }
    
    // MARK: - HTML Generation
    
    private func generateHTML(_ briefing: DailyBriefing) -> String {
        """
        <!DOCTYPE html>
        <html lang="en">
        <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <title>Daily Briefing - \(briefing.fullDateString)</title>
            <style>
                :root {
                    --bg: #FDFAF5;
                    --surface: #FFFFFF;
                    --text-primary: #1A1A1E;
                    --text-secondary: #737375;
                    --text-muted: #A6A6AA;
                    --accent: #FF6B6B;
                    --accent-cyan: #00D4AA;
                    --success: #33C78E;
                    --warning: #FAC033;
                    --error: #F25959;
                    --border: #E0E0DF;
                }
                
                @media (prefers-color-scheme: dark) {
                    :root {
                        --bg: #1A1A1E;
                        --surface: #2A2A2E;
                        --text-primary: #FAFAFA;
                        --text-secondary: #A6A6AA;
                        --text-muted: #737375;
                        --border: #3A3A3E;
                    }
                }
                
                * { margin: 0; padding: 0; box-sizing: border-box; }
                
                body {
                    font-family: -apple-system, BlinkMacSystemFont, 'SF Pro Text', sans-serif;
                    background: var(--bg);
                    color: var(--text-primary);
                    line-height: 1.5;
                    padding: 40px;
                    max-width: 700px;
                    margin: 0 auto;
                }
                
                .briefing {
                    background: var(--surface);
                    border-radius: 12px;
                    overflow: hidden;
                    box-shadow: 0 2px 8px rgba(0,0,0,0.04);
                }
                
                .date-block {
                    padding: 24px;
                    border-bottom: 1px solid var(--border);
                }
                
                .day-of-week {
                    font-size: 13px;
                    font-weight: 500;
                    color: var(--text-muted);
                    letter-spacing: 0.5px;
                }
                
                .day-number {
                    font-size: 56px;
                    font-weight: 300;
                    line-height: 1.1;
                }
                
                .accent-bar {
                    width: 40px;
                    height: 2px;
                    background: var(--accent);
                    margin: 8px 0;
                }
                
                .month-year {
                    font-size: 11px;
                    font-family: 'SF Mono', monospace;
                    font-weight: 500;
                    color: var(--text-muted);
                    letter-spacing: 0.5px;
                }
                
                .section {
                    padding: 16px 24px;
                    border-bottom: 1px solid var(--border);
                }
                
                .section:last-child {
                    border-bottom: none;
                }
                
                .section-header {
                    display: flex;
                    align-items: center;
                    gap: 8px;
                    margin-bottom: 16px;
                }
                
                .section-title {
                    font-size: 11px;
                    font-family: 'SF Mono', monospace;
                    font-weight: 600;
                    color: var(--text-muted);
                    letter-spacing: 0.5px;
                }
                
                .section-line {
                    flex: 1;
                    height: 1px;
                    background: var(--border);
                }
                
                .section-count {
                    font-size: 11px;
                    font-family: 'SF Mono', monospace;
                    color: var(--text-muted);
                }
                
                .meeting {
                    display: flex;
                    gap: 12px;
                    padding: 12px 0;
                    border-bottom: 1px solid var(--border);
                }
                
                .meeting:last-child {
                    border-bottom: none;
                }
                
                .meeting-time {
                    width: 50px;
                    text-align: right;
                }
                
                .time {
                    font-size: 14px;
                    font-family: 'SF Mono', monospace;
                    font-weight: 500;
                }
                
                .duration {
                    font-size: 10px;
                    font-family: 'SF Mono', monospace;
                    color: var(--text-muted);
                }
                
                .meeting-divider {
                    width: 1px;
                    background: var(--border);
                }
                
                .meeting-details {
                    flex: 1;
                }
                
                .meeting-title {
                    font-size: 14px;
                    font-weight: 500;
                }
                
                .meeting-attendees {
                    font-size: 11px;
                    color: var(--text-secondary);
                    margin-top: 4px;
                }
                
                .prep-notes {
                    background: rgba(0, 212, 170, 0.08);
                    border: 1px solid rgba(0, 212, 170, 0.2);
                    border-radius: 6px;
                    padding: 12px;
                    margin-top: 12px;
                    font-size: 12px;
                    color: var(--text-secondary);
                    line-height: 1.6;
                }
                
                .tracked-item {
                    padding: 10px 0;
                    border-bottom: 1px solid var(--border);
                }
                
                .tracked-item:last-child {
                    border-bottom: none;
                }
                
                .item-title {
                    font-size: 13px;
                    font-weight: 500;
                }
                
                .item-meta {
                    font-size: 10px;
                    color: var(--text-muted);
                    margin-top: 4px;
                }
                
                .risk-indicator {
                    font-size: 10px;
                    color: var(--text-muted);
                    margin-top: 4px;
                }
                
                .insight-card {
                    background: rgba(255, 107, 107, 0.06);
                    border: 1px solid rgba(255, 107, 107, 0.15);
                    border-radius: 6px;
                    padding: 12px;
                    margin-bottom: 12px;
                }
                
                .insight-card:last-child {
                    margin-bottom: 0;
                }
                
                .insight-text {
                    font-size: 13px;
                    line-height: 1.5;
                }
                
                .insight-meeting {
                    font-size: 11px;
                    color: var(--accent);
                    margin-top: 8px;
                }
                
                .footer {
                    padding: 16px 24px;
                    text-align: center;
                    font-size: 10px;
                    color: var(--text-muted);
                }
                
                details {
                    cursor: pointer;
                }
                
                summary {
                    list-style: none;
                }
                
                summary::-webkit-details-marker {
                    display: none;
                }
            </style>
        </head>
        <body>
            <div class="briefing">
                <div class="date-block">
                    <div class="day-of-week">\(briefing.dayOfWeek)</div>
                    <div class="day-number">\(briefing.dayNumber)</div>
                    <div class="accent-bar"></div>
                    <div class="month-year">\(briefing.monthYear)</div>
                </div>
                
                \(generateMeetingsHTML(briefing.meetings))
                \(generateAwaitingHTML(briefing.awaitingFromOthers, title: "AWAITING FROM OTHERS"))
                \(generateOwedHTML(briefing.othersAwaitingFromMe, title: "OTHERS AWAITING FROM ME"))
                \(generateInsightsHTML(briefing.proactiveInsights))
                
                <div class="footer">
                    Generated by Minna · \(briefing.generatedTimestamp)
                </div>
            </div>
        </body>
        </html>
        """
    }
    
    private func generateMeetingsHTML(_ meetings: [MeetingWithPrep]) -> String {
        guard !meetings.isEmpty else { return "" }
        
        let meetingsHTML = meetings.map { meeting in
            """
            <div class="meeting">
                <div class="meeting-time">
                    <div class="time">\(meeting.timeString)</div>
                    <div class="duration">\(meeting.durationString)</div>
                </div>
                <div class="meeting-divider"></div>
                <div class="meeting-details">
                    <details open>
                        <summary>
                            <div class="meeting-title">\(meeting.title)</div>
                            <div class="meeting-attendees">\(meeting.attendeesString)</div>
                        </summary>
                        <div class="prep-notes">📋 PREP: \(meeting.prepNotes)</div>
                    </details>
                </div>
            </div>
            """
        }.joined(separator: "\n")
        
        return """
        <div class="section">
            <div class="section-header">
                <span class="section-title">TODAY'S MEETINGS</span>
                <div class="section-line"></div>
                <span class="section-count">\(meetings.count)</span>
            </div>
            \(meetingsHTML)
        </div>
        """
    }
    
    private func generateAwaitingHTML(_ items: [TrackedItem], title: String) -> String {
        guard !items.isEmpty else { return "" }
        
        let itemsHTML = items.map { item in
            let riskEmoji = item.riskLevel.emoji
            let activity = item.lastActivity ?? ""
            
            return """
            <div class="tracked-item">
                <div class="item-title">\(item.title)</div>
                <div class="item-meta">\(item.owner) · \(item.context)\(item.dueDateString.map { " · \($0)" } ?? "")</div>
                \(activity.isEmpty ? "" : "<div class=\"risk-indicator\">\(riskEmoji) \(item.riskLevel.rawValue) — \(activity)</div>")
            </div>
            """
        }.joined(separator: "\n")
        
        return """
        <div class="section">
            <div class="section-header">
                <span class="section-title">\(title)</span>
                <div class="section-line"></div>
                <span class="section-count">\(items.count)</span>
            </div>
            \(itemsHTML)
        </div>
        """
    }
    
    private func generateOwedHTML(_ items: [TrackedItem], title: String) -> String {
        guard !items.isEmpty else { return "" }
        
        let itemsHTML = items.map { item in
            let prefix = item.isOverdue ? "⚠️ " : ""
            
            return """
            <div class="tracked-item">
                <div class="item-title">\(prefix)\(item.title)</div>
                <div class="item-meta">\(item.owner) · \(item.context)\(item.dueDateString.map { " · \($0)" } ?? "")</div>
            </div>
            """
        }.joined(separator: "\n")
        
        return """
        <div class="section">
            <div class="section-header">
                <span class="section-title">\(title)</span>
                <div class="section-line"></div>
                <span class="section-count">\(items.count)</span>
            </div>
            \(itemsHTML)
        </div>
        """
    }
    
    private func generateInsightsHTML(_ insights: [Insight]) -> String {
        guard !insights.isEmpty else { return "" }
        
        let insightsHTML = insights.map { insight in
            """
            <div class="insight-card">
                <div class="insight-text">\(insight.summary)</div>
                \(insight.relevantMeeting.map { "<div class=\"insight-meeting\">→ \($0)</div>" } ?? "")
            </div>
            """
        }.joined(separator: "\n")
        
        return """
        <div class="section">
            <div class="section-header">
                <span class="section-title">✨ INSIGHTS</span>
                <div class="section-line"></div>
            </div>
            \(insightsHTML)
        </div>
        """
    }
    
    // MARK: - Markdown Generation
    
    private func generateMarkdown(_ briefing: DailyBriefing) -> String {
        var md = """
        # Daily Briefing
        ## \(briefing.fullDateString)
        
        ---
        
        """
        
        // Meetings
        if !briefing.meetings.isEmpty {
            md += "### 📅 Today's Meetings\n\n"
            for meeting in briefing.meetings {
                md += "**\(meeting.timeString)** — \(meeting.title) (\(meeting.durationString))\n"
                md += "*\(meeting.attendeesString)*\n\n"
                md += "> 📋 **PREP:** \(meeting.prepNotes)\n\n"
            }
        }
        
        // Awaiting from others
        if !briefing.awaitingFromOthers.isEmpty {
            md += "---\n\n### ⏳ Awaiting From Others\n\n"
            for item in briefing.awaitingFromOthers {
                md += "- **\(item.title)**\n"
                md += "  - \(item.owner) · \(item.context)"
                if let due = item.dueDateString {
                    md += " · \(due)"
                }
                md += "\n"
                if let activity = item.lastActivity {
                    md += "  - \(item.riskLevel.emoji) \(item.riskLevel.rawValue) — \(activity)\n"
                }
                md += "\n"
            }
        }
        
        // Others awaiting from me
        if !briefing.othersAwaitingFromMe.isEmpty {
            md += "---\n\n### 📤 Others Awaiting From Me\n\n"
            for item in briefing.othersAwaitingFromMe {
                let prefix = item.isOverdue ? "⚠️ " : ""
                md += "- \(prefix)**\(item.title)**\n"
                md += "  - \(item.owner) · \(item.context)"
                if let due = item.dueDateString {
                    md += " · \(due)"
                }
                md += "\n\n"
            }
        }
        
        // Insights
        if !briefing.proactiveInsights.isEmpty {
            md += "---\n\n### ✨ Insights\n\n"
            for insight in briefing.proactiveInsights {
                md += "> \(insight.summary)\n"
                if let meeting = insight.relevantMeeting {
                    md += "> → *\(meeting)*\n"
                }
                md += "\n"
            }
        }
        
        md += "---\n\n*Generated by Minna · \(briefing.generatedTimestamp)*\n"
        
        return md
    }
}

