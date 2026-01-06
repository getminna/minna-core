"""
Google Workspace Worker - Orchestrates sync between Google and the Local Vector DB.

This module bridges the macOS Keychain (where the Swift app stores OAuth tokens)
with the Google Workspace Connector (business logic) and VectorManager (storage).

Sovereign Mode Flow:
    1. User provides their own OAuth Client ID + Secret (stored in Keychain)
    2. User completes OAuth in browser, tokens stored in Keychain
    3. This worker reads all credentials from Keychain
    4. On token expiry, refreshes locally using stored client credentials
    5. User never needs to re-authenticate unless refresh token expires

No Vercel bridge required - all OAuth happens locally.
"""

import keyring
from typing import Callable, Optional, Dict, Any
from minna.core.vector_db import VectorManager
from minna.ingestion.google import GoogleWorkspaceConnector


class GoogleWorker:
    """
    Orchestrates the sync between Google Workspace and the Local Vector DB.
    
    Sovereign Mode: All credentials stored locally in Keychain.
    Token refresh happens locally using user's own OAuth credentials.
    """
    
    # Keychain coordinates - must match Swift CredentialManager
    KEYCHAIN_SERVICE = "minna_ai"
    KEYCHAIN_ACCOUNT_ACCESS = "googleWorkspace_token"
    KEYCHAIN_ACCOUNT_REFRESH = "googleWorkspace_refresh_token"
    KEYCHAIN_CLIENT_ID = "googleWorkspace_client_id"
    KEYCHAIN_CLIENT_SECRET = "googleWorkspace_client_secret"
    
    def __init__(self, progress_callback: Optional[Callable] = None):
        """
        Initialize GoogleWorker.
        
        Args:
            progress_callback: Optional callback for progress updates.
        """
        self.progress_callback = progress_callback or (lambda *args, **kwargs: None)
        
        # Retrieve all credentials from Keychain
        self.progress_callback("authenticating", "Reading Google credentials from Keychain...")
        
        access_token = keyring.get_password(self.KEYCHAIN_SERVICE, self.KEYCHAIN_ACCOUNT_ACCESS)
        refresh_token = keyring.get_password(self.KEYCHAIN_SERVICE, self.KEYCHAIN_ACCOUNT_REFRESH)
        client_id = keyring.get_password(self.KEYCHAIN_SERVICE, self.KEYCHAIN_CLIENT_ID)
        client_secret = keyring.get_password(self.KEYCHAIN_SERVICE, self.KEYCHAIN_CLIENT_SECRET)
        
        if not access_token:
            raise ValueError(
                "No Google token found in Keychain. "
                "Please authenticate with Google via the Minna app."
            )
            
        if not client_id or not client_secret:
            self.progress_callback("warning", "No client credentials - token refresh will fail")
        
        self.progress_callback("authenticated", "Google credentials validated (Sovereign Mode)")
        
        # Create connector with all credentials for local token refresh
        self.connector = GoogleWorkspaceConnector(
            access_token=access_token,
            refresh_token=refresh_token,
            client_id=client_id,
            client_secret=client_secret,
            progress_callback=self.progress_callback,
            on_token_refresh=self._persist_refreshed_tokens
        )
        self.db = VectorManager()
    
    def _persist_refreshed_tokens(self, new_access_token: str, new_refresh_token: str):
        """
        Callback invoked when tokens are refreshed.
        Persists the new tokens to Keychain so they survive app restarts.
        """
        self.progress_callback("authenticating", "Saving refreshed tokens to Keychain...")
        
        try:
            # Save new access token
            keyring.set_password(
                self.KEYCHAIN_SERVICE, 
                self.KEYCHAIN_ACCOUNT_ACCESS, 
                new_access_token
            )
            
            # Save new refresh token (Google may rotate these)
            if new_refresh_token:
                keyring.set_password(
                    self.KEYCHAIN_SERVICE,
                    self.KEYCHAIN_ACCOUNT_REFRESH,
                    new_refresh_token
                )
                
            self.progress_callback("authenticated", "Refreshed tokens saved to Keychain")
            
        except Exception as e:
            # Log but don't fail sync - tokens are still valid in memory
            self.progress_callback("warning", f"Could not persist tokens: {e}")
    
    def sync(self, days_back: int = 14) -> Dict[str, Any]:
        """
        Syncs Google Calendar and Gmail to the local vector database.
        
        Args:
            days_back: How many days of history to sync.
                       Defaults to 14 (2 weeks).
                       
        Returns:
            Dict with sync results including document count.
        """
        self.progress_callback("syncing", "Starting Google Workspace sync...")
        
        try:
            # Connector handles calendar + gmail sync
            documents = self.connector.sync(days_back=days_back)
            
            if documents:
                self.progress_callback(
                    "indexing", 
                    f"Indexing {len(documents)} items...",
                    documents_processed=len(documents)
                )
                self.db.add_documents(documents)
                
                # Count by source for summary
                calendar_count = sum(1 for d in documents if d.source == "google_calendar")
                gmail_count = sum(1 for d in documents if d.source == "gmail")
                
                return {
                    "documents": len(documents),
                    "calendar_events": calendar_count,
                    "emails": gmail_count,
                    "success": True
                }
            else:
                return {"documents": 0, "success": True}
                
        except Exception as e:
            raise


# CLI entrypoint for manual testing
if __name__ == "__main__":
    import sys
    
    # Optional: pass days_back as CLI arg
    days = int(sys.argv[1]) if len(sys.argv) > 1 else 14
    
    def print_progress(status, message, **kwargs):
        docs = kwargs.get("documents_processed", "")
        print(f"[{status}] {message} {f'({docs} docs)' if docs else ''}")
    
    worker = GoogleWorker(progress_callback=print_progress)
    result = worker.sync(days_back=days)
    print(f"\nResult: {result}")

