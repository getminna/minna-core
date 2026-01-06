import time
from typing import List, Dict, Optional, Callable
from slack_sdk import WebClient
from slack_sdk.errors import SlackApiError
from .base import BaseConnector, Document

class SlackConnector(BaseConnector):
    def __init__(self, slack_token: str, progress_callback: Optional[Callable] = None):
        self.client = WebClient(token=slack_token)
        self.user_cache: Dict[str, str] = {}
        self.progress_callback = progress_callback or (lambda *args, **kwargs: None)
        self._docs_processed = 0
        self._total_channels = 0
        
    def _fetch_all_users(self):
        """
        Fetches all users in the workspace to build a local cache of ID -> Real Name.
        This avoids N+1 API calls during ingestion.
        """
        self.progress_callback("fetching", "Loading user directory...")
        cursor = None
        while True:
            try:
                response = self.client.users_list(cursor=cursor, limit=200)
                if not response["ok"]:
                    break
                    
                members = response.get("members", [])
                for member in members:
                    uid = member.get("id")
                    # Prefer real name, fall back to display name, then name
                    profile = member.get("profile", {})
                    real_name = profile.get("real_name") or profile.get("display_name") or member.get("name")
                    
                    if uid and real_name:
                        self.user_cache[uid] = real_name
                        
                cursor = response.get("response_metadata", {}).get("next_cursor")
                if not cursor:
                    break
                    
                time.sleep(0.5) # Rate limit safety
                
            except SlackApiError as e:
                self.progress_callback("warning", f"Error fetching users: {e}")
                break
        
        self.progress_callback("fetching", f"Loaded {len(self.user_cache)} users")

    def _get_user_name(self, user_id: str) -> str:
        """Helper to resolve user IDs to names using the cache."""
        if not user_id:
            return "Unknown"
        return self.user_cache.get(user_id, user_id)

    def _fetch_channels(self) -> List[Dict]:
        """
        Fetches all conversation types (public, private, im, mpim) 
        where the bot/user is a member.
        """
        self.progress_callback("fetching", "Discovering channels...")
        channels = []
        cursor = None
        while True:
            try:
                response = self.client.conversations_list(
                    types="public_channel,private_channel,im,mpim",
                    cursor=cursor,
                    limit=200
                )
                if not response["ok"]:
                    break
                    
                for channel in response["channels"]:
                    # Is member check (IMs are implicitly 'member')
                    if channel.get("is_member") or channel.get("is_im"):
                        channels.append(channel)
                
                self.progress_callback("fetching", f"Found {len(channels)} channels so far...")
                        
                cursor = response.get("response_metadata", {}).get("next_cursor")
                if not cursor:
                    break
            except SlackApiError as e:
                if e.response.get("error") == "ratelimited":
                    retry_after = int(e.response.headers.get("Retry-After", 30))
                    self.progress_callback("waiting", f"Rate limited, waiting {retry_after}s...")
                    time.sleep(retry_after)
                    continue
                
                self.progress_callback("warning", f"Error fetching channels: {e}")
                break
            
            # Rate limit safety for listing
            time.sleep(1.2)

        self.progress_callback("fetching", f"Found {len(channels)} channels")
        return channels

    def _extract_text_from_message(self, message: Dict) -> str:
        """
        Extracts and cleans text from a Slack message object.
        prioritizes plain text fallback if blocks are complex.
        """
        import re
        
        # 1. Fallback / Plain Text Source
        text = message.get("text", "") or ""
        
        # 2. Cleanup: Remove user tags <@U12345>
        # Regex for <@U...>
        text = re.sub(r"<@U[A-Z0-9]+>", "", text)
        
        # 3. Cleanup: Remove other common clutter if needed
        # (e.g. broadcast tags like <!here> which are usually fine to keep or remove)
        
        return text.strip()

    def _process_channel(self, channel: Dict, since_timestamp: float) -> List[Document]:
        """
        Process a single channel to fetch messages and threads.
        Raises exception on failure.
        """
        channel_id = channel["id"]
        channel_name = channel.get("name_normalized") or "DM"
        channel_docs = []

        # Fetch History
        cursor = None
        has_more = True
        
        while has_more:
            # conversations.history
            response = self.client.conversations_history(
                channel=channel_id,
                oldest=str(since_timestamp),
                cursor=cursor,
                limit=100
            )
            
            if not response["ok"]:
                raise SlackApiError(f"Error fetching history: {response.get('error')}", response)
                
            messages = response.get("messages", [])
            
            for msg in messages:
                ts = msg.get("ts")
                if not ts: 
                    continue
                
                thread_ts = msg.get("thread_ts")
                reply_count = msg.get("reply_count", 0)
                
                full_content = []
                
                # If it's a thread parent with replies, fetch the whole thread
                if reply_count > 0 and thread_ts == ts:
                    try:
                        thread_res = self.client.conversations_replies(
                            channel=channel_id,
                            ts=thread_ts,
                            limit=1000
                        )
                        if thread_res["ok"]:
                            thread_msgs = thread_res.get("messages", [])
                            for t_msg in thread_msgs:
                                t_user = t_msg.get("user", "Unknown")
                                t_text = self._extract_text_from_message(t_msg)
                                full_content.append(f"{t_user}: {t_text}")
                    except SlackApiError:
                        # Fallback to just the main message
                        user = msg.get("user", "Unknown")
                        text = self._extract_text_from_message(msg)
                        full_content.append(f"{user}: {text}")
                else:
                    # Just a single message
                    user = msg.get("user", "Unknown")
                    text = self._extract_text_from_message(msg)
                    full_content.append(f"{user}: {text}")
                    
                # Create Document
                doc_text = "\n".join(full_content)
                
                doc = Document(
                    source="slack",
                    content=doc_text,
                    metadata={
                        "channel_id": channel_id,
                        "channel_name": channel_name,
                        "ts": ts,
                        "thread_ts": thread_ts,
                        "user": msg.get("user"),
                        "user_real_name": self._get_user_name(msg.get("user"))
                    }
                )
                channel_docs.append(doc)

            # Pagination
            cursor = response.get("response_metadata", {}).get("next_cursor")
            if not cursor:
                has_more = False
            else:
                 # Rate limit safety for history pagination
                 time.sleep(0.5)

        return channel_docs

    def _get_readable_error_reason(self, error: Exception) -> str:
        """
        Translates technical Slack API errors into human-readable advice.
        """
        if isinstance(error, SlackApiError):
            err_code = error.response.get("error")
            if err_code == "missing_scope":
                return "Bot is missing permissions. Check OAuth scopes."
            elif err_code == "channel_not_found":
                return "Channel deleted or bot removed."
            elif err_code == "account_inactive":
                return "User token belongs to a deactivated account."
            elif err_code == "ratelimited":
                return "API rate limit exceeded."
            elif err_code == "not_in_channel":
                return "Bot needs to be invited to this channel."
            elif err_code == "is_archived":
                return "Channel is archived."
        
        return str(error)

    def _fetch_specific_channels(self, channel_ids: List[str]) -> List[Dict]:
        """
        Fetches details for a specific list of channel IDs.
        """
        channels = []
        for cid in channel_ids:
            try:
                # conversations.info
                response = self.client.conversations_info(channel=cid)
                if response["ok"]:
                    ch = response["channel"]
                    # Normalize structure if needed, or just append
                    channels.append(ch)
                else:
                    print(f"⚠️ Could not find specific channel {cid}: {response.get('error')}")
            except Exception as e:
                print(f"⚠️ Error fetching specific channel {cid}: {e}")
                
        return channels

    def sync(self, since_timestamp: float, channel_ids: List[str] = None) -> List[Document]:
        documents = []
        self._docs_processed = 0
        
        if channel_ids:
            self.progress_callback("fetching", f"Targeted sync: {len(channel_ids)} channels")
            channels = self._fetch_specific_channels(channel_ids)
        else:
            channels = self._fetch_channels()
            
        # 1. Warm up User Cache
        self._fetch_all_users()
        
        self._total_channels = len(channels)
        failed_channels = []
        
        # Sweep 1: Fast Path
        for i, channel in enumerate(channels, 1):
            channel_name = channel.get("name_normalized") or "DM"
            
            # Filtering
            if channel.get("is_archived"):
                continue

            # Emit granular progress with channel name
            self.progress_callback(
                "syncing", 
                f"#{channel_name} ({i}/{self._total_channels})",
                documents_processed=self._docs_processed,
                total_documents=self._total_channels
            )
            
            try:
                channel_docs = self._process_channel(channel, since_timestamp)
                documents.extend(channel_docs)
                self._docs_processed = len(documents)
                
                # Update progress after each channel
                self.progress_callback(
                    "syncing",
                    f"#{channel_name} done ({len(channel_docs)} messages)",
                    documents_processed=self._docs_processed
                )
            except Exception as e:
                reason = self._get_readable_error_reason(e)
                self.progress_callback("warning", f"#{channel_name}: {reason}")
                failed_channels.append(channel)
            
            # Rate Limit Prevention between channels
            time.sleep(1.2)
            
        # Sweep 2: Cleanup (Dead Letter Queue)
        if failed_channels:
            self.progress_callback("syncing", f"Retrying {len(failed_channels)} failed channels...")
            
            for channel in failed_channels:
                channel_name = channel.get("name_normalized") or "DM"
                self.progress_callback("syncing", f"Retrying #{channel_name}...")
                
                try:
                    channel_docs = self._process_channel(channel, since_timestamp)
                    documents.extend(channel_docs)
                    self._docs_processed = len(documents)
                except Exception as e:
                    reason = self._get_readable_error_reason(e)
                    self.progress_callback("warning", f"Failed #{channel_name}: {reason}")
                    
                # Rate Limit Prevention
                time.sleep(2.0)
        
        self.progress_callback("syncing", f"Sync complete: {len(documents)} messages", documents_processed=len(documents))
        return documents
