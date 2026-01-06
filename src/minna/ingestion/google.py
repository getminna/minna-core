"""
Google Workspace Connector - Calendar and Gmail sync for Minna context engine.

Fetches:
- Calendar events (meetings, attendees, descriptions)
- Gmail messages (threads, subjects, bodies) with noise filtering

Uses Google REST APIs directly (not the heavy google-api-python-client).

Sovereign Mode Token Refresh:
- User provides their own OAuth Client ID + Secret
- On 401 response, refreshes access token locally using stored credentials
- Retries the failed request once with the new token
- Fails gracefully if refresh also fails
"""

import time
import re
import base64
from datetime import datetime, timedelta
from typing import List, Dict, Optional, Callable, Tuple
import requests
from .base import BaseConnector, Document


class GoogleWorkspaceConnector(BaseConnector):
    """
    Syncs Google Calendar and Gmail to the Minna vector database.
    Uses lightweight REST API calls instead of the full Google SDK.
    
    Sovereign Mode: User provides their own OAuth credentials.
    Handles token refresh automatically using local credentials.
    """
    
    BASE_URL_CALENDAR = "https://www.googleapis.com/calendar/v3"
    BASE_URL_GMAIL = "https://gmail.googleapis.com/gmail/v1"
    GOOGLE_TOKEN_URL = "https://oauth2.googleapis.com/token"  # Direct to Google, no bridge
    
    # =========================================================================
    # GMAIL NOISE FILTERING
    # =========================================================================
    
    # No-reply patterns - these senders are almost always automated
    NOREPLY_PATTERNS = [
        r"no[-_]?reply",
        r"noreply",
        r"do[-_]?not[-_]?reply",
        r"notifications?@",
        r"alerts?@",
        r"mailer[-_]?daemon",
        r"postmaster@",
    ]
    
    # Automated service domains - high noise, low signal
    AUTOMATED_DOMAINS = [
        "github.com",
        "gitlab.com",
        "bitbucket.org",
        "jira.atlassian.com",
        "atlassian.net",
        "linear.app",
        "notion.so",
        "slack.com",
        "asana.com",
        "monday.com",
        "trello.com",
        "aws.amazon.com",
        "amazonses.com",
        "cloud.google.com",
        "azure.microsoft.com",
        "pagerduty.com",
        "opsgenie.com",
        "datadog.com",
        "sentry.io",
        "stripe.com",
        "sendgrid.net",
        "mailchimp.com",
        "hubspot.com",
        "salesforce.com",
        "zendesk.com",
        "intercom.io",
        "calendly.com",
    ]
    
    # Subject patterns that indicate automated/low-value emails
    AUTOMATED_SUBJECT_PATTERNS = [
        r"^\[.*?\]",  # [JIRA-123], [GitHub], etc.
        r"^Re: \[.*?\]",
        r"^Build (failed|succeeded|passed)",
        r"^Pipeline",
        r"^Deploy",
        r"^Alert:",
        r"^Notification:",
        r"^Automated:",
        r"^Your .* receipt",
        r"^Invoice",
        r"^Password reset",
        r"^Verify your",
        r"^Confirm your",
        r"^Welcome to",
        r"^Thanks for signing up",
    ]
    
    def __init__(
        self, 
        access_token: str, 
        refresh_token: str = None,
        client_id: str = None,
        client_secret: str = None,
        progress_callback: Optional[Callable] = None,
        on_token_refresh: Optional[Callable[[str, str], None]] = None
    ):
        """
        Initialize GoogleWorkspaceConnector.
        
        Sovereign Mode: User provides their own OAuth credentials stored in Keychain.
        
        Args:
            access_token: OAuth access token
            refresh_token: OAuth refresh token for automatic renewal
            client_id: User's OAuth Client ID (for local token refresh)
            client_secret: User's OAuth Client Secret (for local token refresh)
            progress_callback: Callback for progress updates
            on_token_refresh: Callback when tokens are refreshed (new_access, new_refresh)
                              Used to persist new tokens to Keychain
        """
        self.access_token = access_token
        self.refresh_token = refresh_token
        self.client_id = client_id
        self.client_secret = client_secret
        self.progress_callback = progress_callback or (lambda *args, **kwargs: None)
        self.on_token_refresh = on_token_refresh
        self._docs_processed = 0
        self._token_refreshed = False  # Track if we've already tried refreshing
        
    def _headers(self) -> Dict[str, str]:
        """Standard auth headers for Google API requests."""
        return {
            "Authorization": f"Bearer {self.access_token}",
            "Content-Type": "application/json",
        }
    
    def _refresh_access_token(self) -> bool:
        """
        Attempt to refresh the access token using the refresh token.
        
        Sovereign Mode: Uses locally stored client credentials to refresh
        directly with Google, no bridge required.
        
        Returns:
            True if refresh succeeded, False otherwise
        """
        if not self.refresh_token:
            self.progress_callback("error", "No refresh token available")
            return False
            
        if not self.client_id or not self.client_secret:
            self.progress_callback("error", "No client credentials for token refresh")
            return False
            
        if self._token_refreshed:
            # Already tried refreshing this session, don't loop
            self.progress_callback("error", "Token refresh already attempted")
            return False
            
        self.progress_callback("authenticating", "Refreshing access token locally...")
        
        try:
            # Refresh directly with Google OAuth endpoint
            response = requests.post(
                self.GOOGLE_TOKEN_URL,
                data={
                    "client_id": self.client_id,
                    "client_secret": self.client_secret,
                    "refresh_token": self.refresh_token,
                    "grant_type": "refresh_token"
                },
                headers={"Content-Type": "application/x-www-form-urlencoded"},
                timeout=30
            )
            
            if response.status_code != 200:
                error_detail = response.json().get("error_description", response.text[:100])
                self.progress_callback("error", f"Token refresh failed: {error_detail}")
                return False
                
            data = response.json()
            new_access_token = data.get("access_token")
            new_refresh_token = data.get("refresh_token")  # Google may rotate refresh tokens
            
            if not new_access_token:
                self.progress_callback("error", "No access token in refresh response")
                return False
                
            # Update tokens
            self.access_token = new_access_token
            if new_refresh_token:
                self.refresh_token = new_refresh_token
                
            self._token_refreshed = True
            
            # Notify caller to persist new tokens
            if self.on_token_refresh:
                self.on_token_refresh(new_access_token, new_refresh_token or self.refresh_token)
                
            self.progress_callback("authenticated", "Token refreshed successfully (local)")
            return True
            
        except requests.RequestException as e:
            self.progress_callback("error", f"Token refresh network error: {e}")
            return False
    
    def _api_get(self, url: str, params: Dict = None, _retry: bool = True) -> Optional[Dict]:
        """
        Make an authenticated GET request to Google APIs.
        
        Handles 401 errors by attempting token refresh and retrying once.
        
        Args:
            url: API endpoint URL
            params: Query parameters
            _retry: Internal flag to prevent infinite retry loops
            
        Returns:
            JSON response dict or None on failure
        """
        try:
            response = requests.get(url, headers=self._headers(), params=params, timeout=30)
            
            if response.status_code == 401:
                # Token expired - attempt refresh and retry
                if _retry and self._refresh_access_token():
                    self.progress_callback("syncing", "Retrying with new token...")
                    return self._api_get(url, params, _retry=False)
                else:
                    self.progress_callback("error", "Token expired - please re-authenticate")
                    return None
                    
            elif response.status_code == 403:
                self.progress_callback("error", "Insufficient permissions")
                return None
            elif response.status_code != 200:
                self.progress_callback("warning", f"API error: {response.status_code}")
                return None
                
            return response.json()
        except requests.RequestException as e:
            self.progress_callback("error", f"Network error: {e}")
            return None
    
    # =========================================================================
    # GMAIL NOISE DETECTION
    # =========================================================================
    
    def _is_automated_sender(self, from_addr: str) -> bool:
        """Check if sender is an automated/no-reply address."""
        from_lower = from_addr.lower()
        
        # Check no-reply patterns
        for pattern in self.NOREPLY_PATTERNS:
            if re.search(pattern, from_lower):
                return True
                
        # Check automated domains
        for domain in self.AUTOMATED_DOMAINS:
            if domain in from_lower:
                return True
                
        return False
    
    def _is_automated_subject(self, subject: str) -> bool:
        """Check if subject indicates an automated email."""
        for pattern in self.AUTOMATED_SUBJECT_PATTERNS:
            if re.search(pattern, subject, re.IGNORECASE):
                return True
        return False
    
    def _calculate_email_signal_score(self, from_addr: str, subject: str, labels: List[str]) -> float:
        """
        Calculate a signal score for an email (0.0 = noise, 1.0 = high signal).
        
        Used to decide whether to include email and how to weight content.
        """
        score = 0.5  # Base score
        
        # Penalize automated senders
        if self._is_automated_sender(from_addr):
            score -= 0.3
            
        # Penalize automated subjects
        if self._is_automated_subject(subject):
            score -= 0.2
            
        # Boost important/starred emails
        if "IMPORTANT" in labels or "STARRED" in labels:
            score += 0.3
            
        # Boost INBOX emails (user hasn't archived)
        if "INBOX" in labels:
            score += 0.1
            
        # Penalize promotional/social
        if "CATEGORY_PROMOTIONS" in labels or "CATEGORY_SOCIAL" in labels:
            score -= 0.2
            
        return max(0.0, min(1.0, score))
    
    # =========================================================================
    # CALENDAR SYNC
    # =========================================================================
    
    def _fetch_calendars(self) -> List[Dict]:
        """Fetch all calendars the user has access to."""
        self.progress_callback("fetching", "Discovering calendars...")
        
        calendars = []
        page_token = None
        
        while True:
            params = {"maxResults": 100}
            if page_token:
                params["pageToken"] = page_token
                
            data = self._api_get(f"{self.BASE_URL_CALENDAR}/users/me/calendarList", params)
            if not data:
                break
                
            for cal in data.get("items", []):
                # Only sync calendars the user owns or has write access to
                # This filters out read-only subscribed calendars
                if cal.get("accessRole") in ["owner", "writer"]:
                    calendars.append(cal)
                    
            page_token = data.get("nextPageToken")
            if not page_token:
                break
                
            time.sleep(0.2)  # Rate limit
            
        self.progress_callback("fetching", f"Found {len(calendars)} calendars")
        return calendars
    
    def _fetch_events(self, calendar_id: str, calendar_name: str, days_back: int = 30, days_forward: int = 14) -> List[Document]:
        """Fetch events from a specific calendar."""
        documents = []
        
        # Time range: past N days to future M days
        time_min = (datetime.utcnow() - timedelta(days=days_back)).isoformat() + "Z"
        time_max = (datetime.utcnow() + timedelta(days=days_forward)).isoformat() + "Z"
        
        page_token = None
        event_count = 0
        
        while True:
            params = {
                "maxResults": 250,
                "singleEvents": "true",  # Expand recurring events
                "orderBy": "startTime",
                "timeMin": time_min,
                "timeMax": time_max,
            }
            if page_token:
                params["pageToken"] = page_token
                
            data = self._api_get(
                f"{self.BASE_URL_CALENDAR}/calendars/{requests.utils.quote(calendar_id, safe='')}/events",
                params
            )
            if not data:
                break
                
            for event in data.get("items", []):
                doc = self._event_to_document(event, calendar_name)
                if doc:
                    documents.append(doc)
                    event_count += 1
                    
            page_token = data.get("nextPageToken")
            if not page_token:
                break
                
            time.sleep(0.2)  # Rate limit
            
        return documents
    
    def _event_to_document(self, event: Dict, calendar_name: str) -> Optional[Document]:
        """Convert a Google Calendar event to a Minna Document."""
        event_id = event.get("id")
        summary = event.get("summary", "Untitled Event")
        description = event.get("description", "")
        location = event.get("location", "")
        
        # Handle all-day vs timed events
        start = event.get("start", {})
        end = event.get("end", {})
        start_time = start.get("dateTime") or start.get("date")
        end_time = end.get("dateTime") or end.get("date")
        
        # Attendees
        attendees = event.get("attendees", [])
        attendee_names = [a.get("displayName") or a.get("email", "") for a in attendees]
        organizer = event.get("organizer", {})
        organizer_name = organizer.get("displayName") or organizer.get("email", "")
        
        # Build searchable content
        content_parts = [
            f"Meeting: {summary}",
            f"When: {start_time} to {end_time}",
        ]
        
        if organizer_name:
            content_parts.append(f"Organizer: {organizer_name}")
        if attendee_names:
            content_parts.append(f"Attendees: {', '.join(attendee_names)}")
        if location:
            content_parts.append(f"Location: {location}")
        if description:
            content_parts.append(f"Description: {description}")
            
        # Meeting links (Zoom, Meet, etc.)
        conference = event.get("conferenceData", {})
        entry_points = conference.get("entryPoints", [])
        for ep in entry_points:
            if ep.get("entryPointType") == "video":
                content_parts.append(f"Video Call: {ep.get('uri', '')}")
                
        content = "\n".join(content_parts)
        
        # Skip empty or declined events
        if not summary or summary.strip() == "":
            return None
            
        return Document(
            source="google_calendar",
            content=content,
            metadata={
                "event_id": event_id,
                "calendar_name": calendar_name,
                "summary": summary,
                "start_time": start_time,
                "end_time": end_time,
                "organizer": organizer_name,
                "attendees": attendee_names,
                "location": location,
                "has_video_call": len(entry_points) > 0,
            }
        )
    
    def sync_calendar(self, days_back: int = 30, days_forward: int = 14) -> List[Document]:
        """Sync all calendar events to documents."""
        all_documents = []
        
        calendars = self._fetch_calendars()
        total_calendars = len(calendars)
        
        for i, calendar in enumerate(calendars, 1):
            cal_id = calendar.get("id")
            cal_name = calendar.get("summary", "Unknown Calendar")
            
            self.progress_callback(
                "syncing",
                f"Calendar: {cal_name} ({i}/{total_calendars})",
                documents_processed=len(all_documents)
            )
            
            events = self._fetch_events(cal_id, cal_name, days_back, days_forward)
            all_documents.extend(events)
            
            time.sleep(0.3)  # Rate limit between calendars
            
        self.progress_callback("syncing", f"Calendar sync done: {len(all_documents)} events")
        return all_documents
    
    # =========================================================================
    # GMAIL SYNC
    # =========================================================================
    
    def _fetch_message_list(self, query: str = "", max_results: int = 100) -> List[Dict]:
        """Fetch list of message IDs matching query."""
        self.progress_callback("fetching", "Discovering emails...")
        
        messages = []
        page_token = None
        
        while len(messages) < max_results:
            params = {
                "maxResults": min(100, max_results - len(messages)),
                "q": query,
            }
            if page_token:
                params["pageToken"] = page_token
                
            data = self._api_get(f"{self.BASE_URL_GMAIL}/users/me/messages", params)
            if not data:
                break
                
            messages.extend(data.get("messages", []))
            
            page_token = data.get("nextPageToken")
            if not page_token:
                break
                
            time.sleep(0.2)
            
        self.progress_callback("fetching", f"Found {len(messages)} emails")
        return messages
    
    def _fetch_message_detail(self, message_id: str) -> Optional[Dict]:
        """Fetch full message content."""
        return self._api_get(
            f"{self.BASE_URL_GMAIL}/users/me/messages/{message_id}",
            params={"format": "full"}
        )
    
    def _extract_email_body(self, payload: Dict) -> str:
        """Extract plain text body from Gmail message payload."""
        body = ""
        
        # Direct body (simple messages)
        if payload.get("body", {}).get("data"):
            body = base64.urlsafe_b64decode(payload["body"]["data"]).decode("utf-8", errors="ignore")
            return body[:2000]  # Truncate for embedding
            
        # Multipart messages
        parts = payload.get("parts", [])
        for part in parts:
            mime_type = part.get("mimeType", "")
            
            if mime_type == "text/plain":
                data = part.get("body", {}).get("data")
                if data:
                    body = base64.urlsafe_b64decode(data).decode("utf-8", errors="ignore")
                    return body[:2000]
                    
            # Recursively check nested parts
            if part.get("parts"):
                nested = self._extract_email_body(part)
                if nested:
                    return nested
                    
        return body[:2000] if body else ""
    
    def _get_header(self, headers: List[Dict], name: str) -> str:
        """Get a specific header value from message headers."""
        for h in headers:
            if h.get("name", "").lower() == name.lower():
                return h.get("value", "")
        return ""
    
    def _message_to_document(self, message: Dict) -> Optional[Document]:
        """
        Convert a Gmail message to a Minna Document.
        
        Applies noise filtering:
        - Skips low-signal automated emails
        - For borderline automated emails, weights subject over body
        """
        msg_id = message.get("id")
        thread_id = message.get("threadId")
        payload = message.get("payload", {})
        headers = payload.get("headers", [])
        
        # Extract key headers
        subject = self._get_header(headers, "Subject") or "(No Subject)"
        from_addr = self._get_header(headers, "From")
        to_addr = self._get_header(headers, "To")
        date_str = self._get_header(headers, "Date")
        
        # Labels (for filtering)
        labels = message.get("labelIds", [])
        
        # Skip drafts and spam
        if "DRAFT" in labels or "SPAM" in labels:
            return None
        
        # Calculate signal score
        signal_score = self._calculate_email_signal_score(from_addr, subject, labels)
        
        # Skip very low signal emails (pure noise)
        if signal_score < 0.2:
            return None
            
        # Extract body
        body = self._extract_email_body(payload)
        
        # For medium-signal (likely automated) emails, heavily weight subject
        # Body of automated emails is often HTML noise
        is_automated = self._is_automated_sender(from_addr) or self._is_automated_subject(subject)
        
        # Build searchable content
        if is_automated and signal_score < 0.5:
            # For automated emails: subject-heavy, minimal body
            content_parts = [
                f"Notification: {subject}",  # Label as notification
                f"From: {from_addr}",
                f"Date: {date_str}",
            ]
            # Only include first 200 chars of body for automated emails
            if body:
                content_parts.append(f"\nSummary: {body[:200]}...")
        else:
            # For human emails: full content
            content_parts = [
                f"Email: {subject}",
                f"From: {from_addr}",
                f"To: {to_addr}",
                f"Date: {date_str}",
            ]
            if body:
                content_parts.append(f"\n{body}")
            
        content = "\n".join(content_parts)
        
        return Document(
            source="gmail",
            content=content,
            metadata={
                "message_id": msg_id,
                "thread_id": thread_id,
                "subject": subject,
                "from": from_addr,
                "to": to_addr,
                "date": date_str,
                "labels": labels,
                "is_unread": "UNREAD" in labels,
                "is_important": "IMPORTANT" in labels,
                "is_automated": is_automated,
                "signal_score": signal_score,
            }
        )
    
    def sync_gmail(self, days_back: int = 14, max_messages: int = 200) -> List[Document]:
        """Sync recent Gmail messages to documents."""
        # Query for recent messages
        query = f"newer_than:{days_back}d"
        
        message_list = self._fetch_message_list(query, max_messages)
        total_messages = len(message_list)
        
        documents = []
        
        for i, msg_ref in enumerate(message_list, 1):
            msg_id = msg_ref.get("id")
            
            if i % 10 == 0 or i == 1:
                self.progress_callback(
                    "syncing",
                    f"Email {i}/{total_messages}",
                    documents_processed=len(documents)
                )
            
            message = self._fetch_message_detail(msg_id)
            if not message:
                continue
                
            doc = self._message_to_document(message)
            if doc:
                documents.append(doc)
                
            time.sleep(0.1)  # Rate limit
            
        self.progress_callback("syncing", f"Gmail sync done: {len(documents)} emails")
        return documents
    
    # =========================================================================
    # MAIN SYNC
    # =========================================================================
    
    def sync(self, days_back: int = 14) -> List[Document]:
        """
        Full sync: Calendar + Gmail.
        
        Args:
            days_back: How many days of history to sync
            
        Returns:
            List of Documents from both Calendar and Gmail
        """
        all_documents = []
        
        # Calendar sync
        self.progress_callback("syncing", "Starting Calendar sync...")
        calendar_docs = self.sync_calendar(days_back=days_back, days_forward=14)
        all_documents.extend(calendar_docs)
        
        # Gmail sync
        self.progress_callback("syncing", "Starting Gmail sync...")
        gmail_docs = self.sync_gmail(days_back=days_back, max_messages=200)
        all_documents.extend(gmail_docs)
        
        self.progress_callback(
            "syncing",
            f"Google Workspace sync complete: {len(all_documents)} items",
            documents_processed=len(all_documents)
        )
        
        return all_documents

