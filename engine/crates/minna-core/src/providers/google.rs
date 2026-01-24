//! Google Workspace provider implementation.
//!
//! Syncs Drive files, Calendar events, and Gmail messages,
//! extracting relationship edges for Gravity Well.

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use tracing::info;

use crate::Document;
use crate::progress::emit_progress;
use minna_auth_bridge::TokenStore;

use super::{
    call_google_api, ExtractedEdge, NodeRef, NodeType, Relation,
    SyncContext, SyncProvider, SyncSummary,
};

/// Google Workspace provider for syncing Drive, Calendar, and Gmail.
pub struct GoogleProvider;

#[async_trait]
impl SyncProvider for GoogleProvider {
    fn name(&self) -> &'static str {
        "google"
    }

    fn display_name(&self) -> &'static str {
        "Google Workspace"
    }

    async fn sync(
        &self,
        ctx: &SyncContext<'_>,
        since_days: Option<i64>,
        mode: Option<&str>,
    ) -> Result<SyncSummary> {
        info!("Starting Google Workspace sync (since_days: {:?}, mode: {:?})", since_days, mode);

        // Sync Drive
        emit_progress("google", "syncing", "Scanning your Drive...", None);
        let (drive_docs, drive_edges, drive_items) = self.sync_drive(ctx, since_days, mode).await?;

        // Sync Calendar
        emit_progress("google", "syncing", "Looking at your calendar...", Some(drive_docs));
        let (cal_docs, cal_edges, cal_items) = self.sync_calendar(ctx, since_days, mode).await?;

        // Sync Gmail
        emit_progress("google", "syncing", "Getting your email...", Some(drive_docs + cal_docs));
        let (gmail_docs, gmail_edges, gmail_items) = self.sync_gmail(ctx, since_days, mode).await?;

        let total_docs = drive_docs + cal_docs + gmail_docs;
        let total_edges = drive_edges + cal_edges + gmail_edges;
        let total_items = drive_items + cal_items + gmail_items;

        info!(
            "Google sync complete: {} docs, {} edges ({} drive, {} calendar, {} gmail)",
            total_docs, total_edges, drive_docs, cal_docs, gmail_docs
        );

        Ok(SyncSummary {
            provider: "google".to_string(),
            items_scanned: total_items,
            documents_processed: total_docs,
            updated_at: Utc::now().to_rfc3339(),
        })
    }
}

impl GoogleProvider {
    /// Sync Google Drive files.
    async fn sync_drive(
        &self,
        ctx: &SyncContext<'_>,
        since_days: Option<i64>,
        mode: Option<&str>,
    ) -> Result<(usize, usize, usize)> {
        let is_full_sync = mode == Some("full");
        info!("Starting Google Drive sync");

        let token_store = TokenStore::load(ctx.auth_path)?;
        let initial_token = token_store
            .get(minna_auth_bridge::Provider::Google)
            .ok_or_else(|| anyhow::anyhow!("missing google token"))?;
        let mut current_token = initial_token.access_token.clone();

        let since = self.calculate_since(ctx, "google_drive", since_days, is_full_sync).await?;

        let file_limit = if is_full_sync {
            std::env::var("MINNA_DRIVE_FILE_LIMIT_FULL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1000)
        } else {
            std::env::var("MINNA_DRIVE_FILE_LIMIT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(50)
        };

        // Get user email (with token refresh support)
        let user_info_result = call_google_api("google", ctx.http_client, &current_token, |token| {
            ctx.http_client
                .get("https://www.googleapis.com/oauth2/v2/userinfo")
                .bearer_auth(token)
        }).await?;
        current_token = user_info_result.token;
        let user_info: GoogleUserInfo = user_info_result.response.json().await?;
        let user_email = user_info.email.clone().unwrap_or_else(|| "me".to_string());

        let mut docs_indexed = 0usize;
        let mut edges_extracted = 0usize;
        let mut page_token: Option<String> = None;

        loop {
            let query = format!(
                "(modifiedTime > '{}') and trashed = false",
                since
            );

            let mut query_params: Vec<(&str, String)> = vec![
                ("q", query),
                ("fields", "files(id,name,mimeType,modifiedTime,webViewLink,owners,sharingUser),nextPageToken".to_string()),
                ("pageSize", "100".to_string()),
            ];

            if let Some(ref pt) = page_token {
                query_params.push(("pageToken", pt.clone()));
            }

            let api_result = call_google_api("google_drive", ctx.http_client, &current_token, |token| {
                ctx.http_client
                    .get("https://www.googleapis.com/drive/v3/files")
                    .query(&query_params)
                    .bearer_auth(token)
            })
            .await?;
            current_token = api_result.token;
            let response = api_result.response;

            let list: DriveListResponse = response.json().await?;

            if let Some(files) = list.files {
                for file in files {
                    if docs_indexed >= file_limit {
                        break;
                    }

                    let updated_at = file.modified_time
                        .as_ref()
                        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(Utc::now);

                    let doc = Document {
                        id: None,
                        uri: file.web_view_link.clone().unwrap_or_else(|| format!("drive://{}", file.id)),
                        source: "google_drive".to_string(),
                        title: Some(file.name.clone()),
                        body: format!(
                            "# {}\n\n- Type: {}\n- Modified: {}\n- URL: {}",
                            file.name,
                            file.mime_type.as_deref().unwrap_or("unknown"),
                            updated_at.to_rfc3339(),
                            file.web_view_link.as_deref().unwrap_or("N/A")
                        ),
                        updated_at,
                    };

                    ctx.index_document(doc).await?;
                    docs_indexed += 1;

                    // Extract edges
                    let edges = self.extract_drive_edges(&file, &user_email, updated_at);
                    if !edges.is_empty() {
                        ctx.index_edges(&edges).await?;
                        edges_extracted += edges.len();
                    }
                }
            }

            page_token = list.next_page_token;
            if page_token.is_none() || docs_indexed >= file_limit {
                break;
            }
        }

        ctx.set_sync_cursor("google_drive", &Utc::now().to_rfc3339()).await?;
        info!("Drive sync: {} docs, {} edges", docs_indexed, edges_extracted);

        Ok((docs_indexed, edges_extracted, docs_indexed))
    }

    /// Sync Google Calendar events.
    async fn sync_calendar(
        &self,
        ctx: &SyncContext<'_>,
        since_days: Option<i64>,
        mode: Option<&str>,
    ) -> Result<(usize, usize, usize)> {
        let is_full_sync = mode == Some("full");
        info!("Starting Google Calendar sync");

        let token_store = TokenStore::load(ctx.auth_path)?;
        let initial_token = token_store
            .get(minna_auth_bridge::Provider::Google)
            .ok_or_else(|| anyhow::anyhow!("missing google token"))?;
        let mut current_token = initial_token.access_token.clone();

        let since = self.calculate_since(ctx, "google_calendar", since_days, is_full_sync).await?;

        let event_limit = if is_full_sync {
            std::env::var("MINNA_CALENDAR_EVENT_LIMIT_FULL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(500)
        } else {
            std::env::var("MINNA_CALENDAR_EVENT_LIMIT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(100)
        };

        let mut docs_indexed = 0usize;
        let mut edges_extracted = 0usize;
        let mut page_token: Option<String> = None;

        loop {
            let mut query_params: Vec<(&str, String)> = vec![
                ("timeMin", since.clone()),
                ("maxResults", "100".to_string()),
                ("singleEvents", "true".to_string()),
                ("orderBy", "updated".to_string()),
            ];

            if let Some(ref pt) = page_token {
                query_params.push(("pageToken", pt.clone()));
            }

            let api_result = call_google_api("google_calendar", ctx.http_client, &current_token, |token| {
                ctx.http_client
                    .get("https://www.googleapis.com/calendar/v3/calendars/primary/events")
                    .query(&query_params)
                    .bearer_auth(token)
            })
            .await?;
            current_token = api_result.token;
            let response = api_result.response;

            let events: CalendarEventsResponse = response.json().await?;

            if let Some(items) = events.items {
                for event in items {
                    if docs_indexed >= event_limit {
                        break;
                    }

                    let updated_at = event.updated
                        .as_ref()
                        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(Utc::now);

                    let summary = event.summary.as_deref().unwrap_or("(No title)");
                    let attendees_str = event.attendees
                        .as_ref()
                        .map(|a| a.iter()
                            .filter_map(|att| att.email.as_ref())
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", "))
                        .unwrap_or_default();

                    let doc = Document {
                        id: None,
                        uri: event.html_link.clone().unwrap_or_else(|| format!("calendar://{}", event.id)),
                        source: "google_calendar".to_string(),
                        title: Some(summary.to_string()),
                        body: format!(
                            "# {}\n\n- Start: {}\n- End: {}\n- Attendees: {}\n- URL: {}\n\n{}",
                            summary,
                            event.start.as_ref().and_then(|s| s.date_time.as_ref().or(s.date.as_ref())).unwrap_or(&"TBD".to_string()),
                            event.end.as_ref().and_then(|e| e.date_time.as_ref().or(e.date.as_ref())).unwrap_or(&"TBD".to_string()),
                            attendees_str,
                            event.html_link.as_deref().unwrap_or("N/A"),
                            event.description.as_deref().unwrap_or("")
                        ),
                        updated_at,
                    };

                    ctx.index_document(doc).await?;
                    docs_indexed += 1;

                    // Extract edges
                    let edges = self.extract_calendar_edges(&event, updated_at);
                    if !edges.is_empty() {
                        ctx.index_edges(&edges).await?;
                        edges_extracted += edges.len();
                    }
                }
            }

            page_token = events.next_page_token;
            if page_token.is_none() || docs_indexed >= event_limit {
                break;
            }
        }

        ctx.set_sync_cursor("google_calendar", &Utc::now().to_rfc3339()).await?;
        info!("Calendar sync: {} docs, {} edges", docs_indexed, edges_extracted);

        Ok((docs_indexed, edges_extracted, docs_indexed))
    }

    /// Sync Gmail messages.
    async fn sync_gmail(
        &self,
        ctx: &SyncContext<'_>,
        since_days: Option<i64>,
        mode: Option<&str>,
    ) -> Result<(usize, usize, usize)> {
        let is_full_sync = mode == Some("full");
        info!("Starting Gmail sync");

        let token_store = TokenStore::load(ctx.auth_path)?;
        let initial_token = token_store
            .get(minna_auth_bridge::Provider::Google)
            .ok_or_else(|| anyhow::anyhow!("missing google token"))?;
        let mut current_token = initial_token.access_token.clone();

        let days = if is_full_sync {
            since_days.unwrap_or(90)
        } else {
            since_days.unwrap_or(30)
        };

        let message_limit = if is_full_sync {
            std::env::var("MINNA_GMAIL_MESSAGE_LIMIT_FULL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(500)
        } else {
            std::env::var("MINNA_GMAIL_MESSAGE_LIMIT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(100)
        };

        // Get message list
        let after_date = (Utc::now() - chrono::Duration::days(days)).format("%Y/%m/%d");
        let query = format!("after:{}", after_date);

        let query_params: Vec<(&str, String)> = vec![
            ("q", query),
            ("maxResults", message_limit.to_string()),
        ];

        let api_result = call_google_api("gmail", ctx.http_client, &current_token, |token| {
            ctx.http_client
                .get("https://gmail.googleapis.com/gmail/v1/users/me/messages")
                .query(&query_params)
                .bearer_auth(token)
        })
        .await?;
        current_token = api_result.token;

        let list: GmailListResponse = api_result.response.json().await?;

        let mut docs_indexed = 0usize;
        let mut edges_extracted = 0usize;

        if let Some(messages) = list.messages {
            for msg_ref in messages.into_iter().take(message_limit) {
                // Fetch full message
                let msg_url = format!(
                    "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?format=metadata&metadataHeaders=From&metadataHeaders=To&metadataHeaders=Cc&metadataHeaders=Subject&metadataHeaders=Date",
                    msg_ref.id
                );

                let msg_result = call_google_api("gmail", ctx.http_client, &current_token, |token| {
                    ctx.http_client
                        .get(&msg_url)
                        .bearer_auth(token)
                })
                .await?;
                current_token = msg_result.token;
                let msg_response = msg_result.response;

                let message: GmailMessage = msg_response.json().await?;

                let headers = message.payload.as_ref()
                    .and_then(|p| p.headers.as_ref())
                    .cloned()
                    .unwrap_or_default();

                let subject = headers.iter()
                    .find(|h| h.name.eq_ignore_ascii_case("subject"))
                    .and_then(|h| h.value.clone())
                    .unwrap_or_else(|| "(No subject)".to_string());

                let from = headers.iter()
                    .find(|h| h.name.eq_ignore_ascii_case("from"))
                    .and_then(|h| h.value.clone())
                    .unwrap_or_default();

                let to = headers.iter()
                    .find(|h| h.name.eq_ignore_ascii_case("to"))
                    .and_then(|h| h.value.clone())
                    .unwrap_or_default();

                let date_str = headers.iter()
                    .find(|h| h.name.eq_ignore_ascii_case("date"))
                    .and_then(|h| h.value.clone())
                    .unwrap_or_default();

                let updated_at = message.internal_date
                    .as_ref()
                    .and_then(|ts| ts.parse::<i64>().ok())
                    .map(|ts| DateTime::from_timestamp_millis(ts).unwrap_or_else(Utc::now))
                    .unwrap_or_else(Utc::now);

                let doc = Document {
                    id: None,
                    uri: format!("https://mail.google.com/mail/u/0/#inbox/{}", message.id),
                    source: "gmail".to_string(),
                    title: Some(subject.clone()),
                    body: format!(
                        "# {}\n\n- From: {}\n- To: {}\n- Date: {}",
                        subject, from, to, date_str
                    ),
                    updated_at,
                };

                ctx.index_document(doc).await?;
                docs_indexed += 1;

                // Extract edges
                let edges = self.extract_gmail_edges(&message.id, &from, &to, &headers, updated_at);
                if !edges.is_empty() {
                    ctx.index_edges(&edges).await?;
                    edges_extracted += edges.len();
                }
            }
        }

        ctx.set_sync_cursor("gmail", &Utc::now().to_rfc3339()).await?;
        info!("Gmail sync: {} docs, {} edges", docs_indexed, edges_extracted);

        Ok((docs_indexed, edges_extracted, docs_indexed))
    }

    async fn calculate_since(
        &self,
        ctx: &SyncContext<'_>,
        cursor_key: &str,
        since_days: Option<i64>,
        is_full_sync: bool,
    ) -> Result<String> {
        if is_full_sync {
            let days = since_days.unwrap_or(90);
            Ok((Utc::now() - chrono::Duration::days(days)).to_rfc3339())
        } else if let Some(days) = since_days {
            Ok((Utc::now() - chrono::Duration::days(days)).to_rfc3339())
        } else {
            let cursor = ctx.get_sync_cursor(cursor_key).await?.unwrap_or_default();
            if cursor.is_empty() {
                Ok((Utc::now() - chrono::Duration::days(30)).to_rfc3339())
            } else {
                Ok(cursor)
            }
        }
    }

    fn extract_drive_edges(
        &self,
        file: &DriveFile,
        _user_email: &str,
        observed_at: DateTime<Utc>,
    ) -> Vec<ExtractedEdge> {
        let mut edges = Vec::new();

        let doc_node = NodeRef::with_name(
            NodeType::Document,
            "google_drive",
            &file.id,
            &file.name,
        );

        // Owner → Document (AuthorOf)
        if let Some(ref owners) = file.owners {
            for owner in owners {
                if let Some(ref email) = owner.email_address {
                    let user_node = NodeRef::with_name(
                        NodeType::User,
                        "google",
                        email,
                        owner.display_name.as_deref().unwrap_or(email),
                    );
                    edges.push(ExtractedEdge::new(
                        user_node,
                        doc_node.clone(),
                        Relation::AuthorOf,
                        observed_at,
                    ));
                }
            }
        }

        edges
    }

    fn extract_calendar_edges(
        &self,
        event: &CalendarEvent,
        observed_at: DateTime<Utc>,
    ) -> Vec<ExtractedEdge> {
        let mut edges = Vec::new();

        let event_node = NodeRef::with_name(
            NodeType::Document, // Using Document for events
            "google_calendar",
            &event.id,
            event.summary.as_deref().unwrap_or("Event"),
        );

        // Organizer → Event (AuthorOf)
        if let Some(ref organizer) = event.organizer {
            if let Some(ref email) = organizer.email {
                let user_node = NodeRef::with_name(
                    NodeType::User,
                    "google",
                    email,
                    organizer.display_name.as_deref().unwrap_or(email),
                );
                edges.push(ExtractedEdge::new(
                    user_node,
                    event_node.clone(),
                    Relation::AuthorOf,
                    observed_at,
                ));
            }
        }

        // Attendees → Event (MentionedIn - they're mentioned/invited)
        if let Some(ref attendees) = event.attendees {
            for attendee in attendees {
                if let Some(ref email) = attendee.email {
                    // Skip the organizer if they're also an attendee
                    if event.organizer.as_ref().and_then(|o| o.email.as_ref()) == Some(email) {
                        continue;
                    }
                    let user_node = NodeRef::with_name(
                        NodeType::User,
                        "google",
                        email,
                        attendee.display_name.as_deref().unwrap_or(email),
                    );
                    edges.push(ExtractedEdge::new(
                        user_node,
                        event_node.clone(),
                        Relation::MentionedIn,
                        observed_at,
                    ));
                }
            }
        }

        edges
    }

    fn extract_gmail_edges(
        &self,
        message_id: &str,
        from: &str,
        to: &str,
        headers: &[GmailHeader],
        observed_at: DateTime<Utc>,
    ) -> Vec<ExtractedEdge> {
        let mut edges = Vec::new();

        let message_node = NodeRef::new(
            NodeType::Message,
            "gmail",
            message_id,
        );

        // Extract email from "Name <email>" format
        let extract_email = |s: &str| -> Option<String> {
            if let Some(start) = s.find('<') {
                if let Some(end) = s.find('>') {
                    return Some(s[start + 1..end].to_string());
                }
            }
            if s.contains('@') {
                Some(s.trim().to_string())
            } else {
                None
            }
        };

        // From → Message (AuthorOf)
        if let Some(from_email) = extract_email(from) {
            let user_node = NodeRef::with_name(
                NodeType::User,
                "google",
                &from_email,
                &from_email,
            );
            edges.push(ExtractedEdge::new(
                user_node,
                message_node.clone(),
                Relation::AuthorOf,
                observed_at,
            ));
        }

        // To recipients → Message (MentionedIn)
        for recipient in to.split(',') {
            if let Some(email) = extract_email(recipient.trim()) {
                let user_node = NodeRef::with_name(
                    NodeType::User,
                    "google",
                    &email,
                    &email,
                );
                edges.push(ExtractedEdge::new(
                    user_node,
                    message_node.clone(),
                    Relation::MentionedIn,
                    observed_at,
                ));
            }
        }

        // CC recipients → Message (MentionedIn)
        let cc = headers.iter()
            .find(|h| h.name.eq_ignore_ascii_case("cc"))
            .and_then(|h| h.value.clone())
            .unwrap_or_default();

        for recipient in cc.split(',') {
            if let Some(email) = extract_email(recipient.trim()) {
                let user_node = NodeRef::with_name(
                    NodeType::User,
                    "google",
                    &email,
                    &email,
                );
                edges.push(ExtractedEdge::new(
                    user_node,
                    message_node.clone(),
                    Relation::MentionedIn,
                    observed_at,
                ));
            }
        }

        edges
    }
}

// --- API Response Types ---

#[derive(Debug, Clone, Deserialize)]
struct GoogleUserInfo {
    email: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DriveListResponse {
    files: Option<Vec<DriveFile>>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DriveFile {
    id: String,
    name: String,
    #[serde(rename = "mimeType")]
    mime_type: Option<String>,
    #[serde(rename = "modifiedTime")]
    modified_time: Option<String>,
    #[serde(rename = "webViewLink")]
    web_view_link: Option<String>,
    owners: Option<Vec<DriveUser>>,
}

#[derive(Debug, Clone, Deserialize)]
struct DriveUser {
    #[serde(rename = "emailAddress")]
    email_address: Option<String>,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct CalendarEventsResponse {
    items: Option<Vec<CalendarEvent>>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct CalendarEvent {
    id: String,
    summary: Option<String>,
    description: Option<String>,
    #[serde(rename = "htmlLink")]
    html_link: Option<String>,
    updated: Option<String>,
    start: Option<CalendarTime>,
    end: Option<CalendarTime>,
    organizer: Option<CalendarPerson>,
    attendees: Option<Vec<CalendarPerson>>,
}

#[derive(Debug, Clone, Deserialize)]
struct CalendarTime {
    #[serde(rename = "dateTime")]
    date_time: Option<String>,
    date: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct CalendarPerson {
    email: Option<String>,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct GmailListResponse {
    messages: Option<Vec<GmailMessageRef>>,
}

#[derive(Debug, Clone, Deserialize)]
struct GmailMessageRef {
    id: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GmailMessage {
    id: String,
    #[serde(rename = "internalDate")]
    internal_date: Option<String>,
    payload: Option<GmailPayload>,
}

#[derive(Debug, Clone, Deserialize)]
struct GmailPayload {
    headers: Option<Vec<GmailHeader>>,
}

#[derive(Debug, Clone, Deserialize)]
struct GmailHeader {
    name: String,
    value: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_email() {
        let extract_email = |s: &str| -> Option<String> {
            if let Some(start) = s.find('<') {
                if let Some(end) = s.find('>') {
                    return Some(s[start + 1..end].to_string());
                }
            }
            if s.contains('@') {
                Some(s.trim().to_string())
            } else {
                None
            }
        };

        assert_eq!(extract_email("Alice <alice@example.com>"), Some("alice@example.com".to_string()));
        assert_eq!(extract_email("bob@example.com"), Some("bob@example.com".to_string()));
        assert_eq!(extract_email("No Email"), None);
    }
}
