import Foundation

// MARK: - Daily Briefing

struct DailyBriefing {
    let date: Date
    let meetings: [MeetingWithPrep]
    let awaitingFromOthers: [TrackedItem]
    let othersAwaitingFromMe: [TrackedItem]
    let proactiveInsights: [Insight]
    
    // MARK: - Date Formatting
    
    var dayOfWeek: String {
        let formatter = DateFormatter()
        formatter.dateFormat = "EEE"
        return formatter.string(from: date).uppercased()
    }
    
    var dayNumber: String {
        let formatter = DateFormatter()
        formatter.dateFormat = "dd"
        return formatter.string(from: date)
    }
    
    var monthYear: String {
        let formatter = DateFormatter()
        formatter.dateFormat = "MMMM yyyy"
        return formatter.string(from: date).uppercased()
    }
    
    var fullDateString: String {
        let formatter = DateFormatter()
        formatter.dateFormat = "EEEE, MMMM d, yyyy"
        return formatter.string(from: date)
    }
    
    var dateString: String {
        fullDateString
    }
    
    var generatedTimestamp: String {
        let formatter = DateFormatter()
        formatter.dateFormat = "MMM d, yyyy 'at' h:mm a"
        return formatter.string(from: Date())
    }
}

// MARK: - Meeting with Prep

struct MeetingWithPrep: Identifiable {
    let id = UUID()
    let title: String
    let time: Date
    let duration: Int // minutes
    let attendees: [String]
    let prepNotes: String // Gemma-generated from vault context
    
    var timeString: String {
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm"
        return formatter.string(from: time)
    }
    
    var durationString: String {
        if duration >= 60 {
            let hours = duration / 60
            let mins = duration % 60
            if mins == 0 {
                return "\(hours)h"
            }
            return "\(hours)h \(mins)m"
        }
        return "\(duration)m"
    }
    
    var attendeesString: String {
        if attendees.count <= 2 {
            return attendees.joined(separator: ", ")
        }
        return "\(attendees.count) people"
    }
}

// MARK: - Tracked Item

struct TrackedItem: Identifiable {
    let id = UUID()
    let title: String
    let owner: String
    let source: Provider
    let context: String // channel, PR number, etc.
    let dueDate: Date?
    let riskLevel: RiskLevel
    let lastActivity: String? // "mentioned 'almost done' yesterday"
    
    var dueDateString: String? {
        guard let due = dueDate else { return nil }
        
        let calendar = Calendar.current
        let today = calendar.startOfDay(for: Date())
        let dueDay = calendar.startOfDay(for: due)
        
        let diff = calendar.dateComponents([.day], from: today, to: dueDay).day ?? 0
        
        switch diff {
        case ..<0:
            return "Overdue"
        case 0:
            return "Due today"
        case 1:
            return "Due tomorrow"
        case 2...7:
            let formatter = DateFormatter()
            formatter.dateFormat = "EEE"
            return "Due \(formatter.string(from: due))"
        default:
            let formatter = DateFormatter()
            formatter.dateFormat = "MMM d"
            return "Due \(formatter.string(from: due))"
        }
    }
    
    var isOverdue: Bool {
        guard let due = dueDate else { return false }
        return due < Date()
    }
}

// MARK: - Risk Level

enum RiskLevel: String {
    case low = "Low"
    case medium = "Medium"
    case high = "High"
    
    var emoji: String {
        switch self {
        case .low: return "🟢"
        case .medium: return "🟡"
        case .high: return "🔴"
        }
    }
}

// MARK: - Insight

struct Insight: Identifiable {
    let id = UUID()
    let summary: String
    let relevantMeeting: String? // "Consider addressing in Q1 Planning"
    let sources: [String] // channels/threads that triggered this
}

// MARK: - Sample Data

extension DailyBriefing {
    static var sample: DailyBriefing {
        let calendar = Calendar.current
        let today = Date()
        
        // Create meeting times for today
        let meeting1Time = calendar.date(bySettingHour: 10, minute: 0, second: 0, of: today)!
        let meeting2Time = calendar.date(bySettingHour: 14, minute: 0, second: 0, of: today)!
        let meeting3Time = calendar.date(bySettingHour: 16, minute: 0, second: 0, of: today)!
        
        return DailyBriefing(
            date: today,
            meetings: [
                MeetingWithPrep(
                    title: "Q1 Planning Review",
                    time: meeting1Time,
                    duration: 60,
                    attendees: ["Sarah Chen", "Mike Rodriguez", "Jenny Liu"],
                    prepNotes: "Sarah likely focused on API timeline (3 recent mentions in #engineering). Mike concerned about budget allocation per yesterday's thread. Jenny may raise design resource constraints."
                ),
                MeetingWithPrep(
                    title: "1:1 with David",
                    time: meeting2Time,
                    duration: 30,
                    attendees: ["David Park"],
                    prepNotes: "Last 1:1 discussed promotion timeline. He's been very active in PR reviews this week — may want recognition. Also mentioned interest in the new auth project."
                ),
                MeetingWithPrep(
                    title: "Design Sync",
                    time: meeting3Time,
                    duration: 45,
                    attendees: ["Design Team"],
                    prepNotes: "Settings page mockups are the main deliverable. Team velocity has been good — shipped 3 features last sprint. May want to discuss upcoming rebrand timeline."
                )
            ],
            awaitingFromOthers: [
                TrackedItem(
                    title: "API spec review feedback",
                    owner: "@sarah",
                    source: .slack,
                    context: "#engineering",
                    dueDate: today,
                    riskLevel: .medium,
                    lastActivity: "No activity in 2 days"
                ),
                TrackedItem(
                    title: "Updated mockups for settings page",
                    owner: "@jenny",
                    source: .slack,
                    context: "DM",
                    dueDate: calendar.date(byAdding: .day, value: 1, to: today),
                    riskLevel: .low,
                    lastActivity: "She mentioned \"almost done\" yesterday"
                )
            ],
            othersAwaitingFromMe: [
                TrackedItem(
                    title: "Review PR #1024 — Auth flow fix",
                    owner: "@david",
                    source: .github,
                    context: "PR #1024",
                    dueDate: today,
                    riskLevel: .high,
                    lastActivity: "Requested 2 days ago"
                ),
                TrackedItem(
                    title: "Feedback on Q1 roadmap doc",
                    owner: "#product",
                    source: .slack,
                    context: "#product",
                    dueDate: calendar.date(byAdding: .day, value: 3, to: today),
                    riskLevel: .low,
                    lastActivity: nil
                )
            ],
            proactiveInsights: [
                Insight(
                    summary: "The Product launch timeline is at risk — 4 mentions of \"blocked\" in #engineering this week, up from 0 last week.",
                    relevantMeeting: "Consider addressing in Q1 Planning Review",
                    sources: ["#engineering", "#product"]
                ),
                Insight(
                    summary: "David has mass 12 PRs reviewed this month — highest on the team. Recognition opportunity.",
                    relevantMeeting: "Good topic for your 1:1",
                    sources: ["GitHub activity"]
                )
            ]
        )
    }
}


