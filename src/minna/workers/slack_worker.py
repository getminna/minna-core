"""
Slack Worker - Orchestrates sync between Slack and the Local Vector DB.

This module bridges the macOS Keychain (where the Swift app stores OAuth tokens)
with the Slack Connector (business logic) and VectorManager (storage).

Sovereign Mode Flow:
    1. User creates their own Slack App via manifest
    2. User provides Bot Token (xoxb-) and/or User Token (xoxp-)
    3. Tokens stored in Keychain by Swift app
    4. This worker reads tokens and syncs messages
"""

import keyring
from typing import Callable, Optional, Dict, Any
from minna.core.vector_db import VectorManager
from minna.ingestion.slack import SlackConnector


class SlackWorker:
    """
    Orchestrates the sync between Slack and the Local Vector DB.
    Authenticates via macOS Keychain using Sovereign Mode tokens.
    
    Supports dual token pattern:
    - User Token (xoxp-): Primary, for user-scoped access
    - Bot Token (xoxb-): Optional, for bot-specific features
    """
    
    # Keychain coordinates - must match Swift CredentialManager
    KEYCHAIN_SERVICE = "minna_ai"
    KEYCHAIN_USER_TOKEN = "slack_user_token"  # Primary token (xoxp-)
    KEYCHAIN_BOT_TOKEN = "slack_bot_token"    # Optional bot token (xoxb-)
    
    # Legacy key for backwards compatibility
    KEYCHAIN_LEGACY = "slack_token"
    
    def __init__(self, progress_callback: Optional[Callable] = None):
        """
        Initialize SlackWorker.
        
        Args:
            progress_callback: Optional callback for progress updates.
                               Signature: (status: str, message: str, documents_processed: int, total_documents: int)
        """
        self.progress_callback = progress_callback or (lambda *args, **kwargs: None)
        
        # Retrieve tokens securely from Keychain
        self.progress_callback("authenticating", "Reading Slack tokens from Keychain...")
        
        # Try new Sovereign Mode tokens first
        user_token = keyring.get_password(self.KEYCHAIN_SERVICE, self.KEYCHAIN_USER_TOKEN)
        bot_token = keyring.get_password(self.KEYCHAIN_SERVICE, self.KEYCHAIN_BOT_TOKEN)
        
        # Fall back to legacy token (from old OAuth flow)
        legacy_token = keyring.get_password(self.KEYCHAIN_SERVICE, self.KEYCHAIN_LEGACY)
        
        # Prefer user token, fall back to legacy
        token = user_token or legacy_token
        
        if not token:
            raise ValueError(
                "No Slack token found in Keychain. "
                "Please configure Slack in the Minna app."
            )
        
        # Validate it looks like a Slack token
        if not token.startswith("xox"):
            raise ValueError(
                f"Token doesn't look like a Slack token (got: {token[:20]}...). "
                "Please reconfigure Slack in the Minna app."
            )
        
        token_type = "user" if token.startswith("xoxp-") else "bot" if token.startswith("xoxb-") else "unknown"
        self.progress_callback("authenticated", f"Slack {token_type} token validated")
        
        self.connector = SlackConnector(slack_token=token, progress_callback=self.progress_callback)
        self.db = VectorManager()  # Uses standard path from config
    
    def sync(self, since_timestamp: float = 0) -> Dict[str, Any]:
        """
        Syncs Slack messages to the local vector database.
        
        Args:
            since_timestamp: Unix timestamp to fetch messages after.
                             Defaults to 0 (fetch all history).
                             
        Returns:
            Dict with sync results including document count.
        """
        self.progress_callback("syncing", "Starting Slack sync...")
        
        try:
            # Connector handles rate limits and thread grouping
            documents = self.connector.sync(since_timestamp=since_timestamp)
            
            if documents:
                self.progress_callback("indexing", f"Indexing {len(documents)} conversations...", 
                                      documents_processed=len(documents))
                self.db.add_documents(documents)
                return {"documents": len(documents), "success": True}
            else:
                return {"documents": 0, "success": True}
                
        except Exception as e:
            raise


# CLI entrypoint for manual testing
if __name__ == "__main__":
    import sys
    
    # Optional: pass timestamp as CLI arg
    since = float(sys.argv[1]) if len(sys.argv) > 1 else 0
    
    worker = SlackWorker()
    worker.sync(since_timestamp=since)


