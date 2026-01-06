"""
GitHub Worker - Orchestrates sync between GitHub and the Local Vector DB.

Sovereign Mode Flow:
    1. User creates a Fine-grained Personal Access Token
    2. Token stored in Keychain by Swift app
    3. This worker reads token and syncs issues/PRs/comments

Focus: Comments and discussions (per connector priority)
    - Issue comments
    - PR review comments
    - Discussion threads
"""

import keyring
from typing import Callable, Optional, Dict, Any
from minna.core.vector_db import VectorManager
from minna.ingestion.github import GitHubConnector


class GitHubWorker:
    """
    Orchestrates the sync between GitHub and the Local Vector DB.
    Authenticates via Personal Access Token stored in macOS Keychain.
    """
    
    # Keychain coordinates - must match Swift CredentialManager
    KEYCHAIN_SERVICE = "minna_ai"
    KEYCHAIN_PAT = "github_pat"
    
    def __init__(self, progress_callback: Optional[Callable] = None):
        """
        Initialize GitHubWorker.
        
        Args:
            progress_callback: Optional callback for progress updates.
                               Signature: (status: str, message: str, documents_processed: int, total_documents: int)
        """
        self.progress_callback = progress_callback or (lambda *args, **kwargs: None)
        
        # Retrieve PAT securely from Keychain
        self.progress_callback("authenticating", "Reading GitHub PAT from Keychain...")
        
        pat = keyring.get_password(self.KEYCHAIN_SERVICE, self.KEYCHAIN_PAT)
        
        if not pat:
            raise ValueError(
                "No GitHub PAT found in Keychain. "
                "Please configure GitHub in the Minna app."
            )
        
        # Validate it looks like a GitHub PAT
        if not (pat.startswith("github_pat_") or pat.startswith("ghp_")):
            raise ValueError(
                f"Token doesn't look like a GitHub PAT (got: {pat[:15]}...). "
                "Please reconfigure GitHub in the Minna app."
            )
        
        self.progress_callback("authenticated", "GitHub PAT validated")
        
        self.connector = GitHubConnector(pat=pat, progress_callback=self.progress_callback)
        self.db = VectorManager()
    
    def sync(self, days_back: int = 30) -> Dict[str, Any]:
        """
        Syncs GitHub data to the local vector database.
        
        Args:
            days_back: How many days of history to sync.
                       
        Returns:
            Dict with sync results including document count.
        """
        self.progress_callback("syncing", "Starting GitHub sync...")
        
        try:
            documents = self.connector.sync(days_back=days_back)
            
            if documents:
                self.progress_callback(
                    "indexing",
                    f"Indexing {len(documents)} items...",
                    documents_processed=len(documents)
                )
                self.db.add_documents(documents)
                
                # Count by type
                issue_count = sum(1 for d in documents if "issue" in d.source)
                pr_count = sum(1 for d in documents if "pull_request" in d.source)
                comment_count = sum(1 for d in documents if "comment" in d.source)
                
                return {
                    "documents": len(documents),
                    "issues": issue_count,
                    "pull_requests": pr_count,
                    "comments": comment_count,
                    "success": True
                }
            else:
                return {"documents": 0, "success": True}
                
        except Exception as e:
            raise


# CLI entrypoint for manual testing
if __name__ == "__main__":
    import sys
    
    days = int(sys.argv[1]) if len(sys.argv) > 1 else 30
    
    def print_progress(status, message, **kwargs):
        docs = kwargs.get("documents_processed", "")
        print(f"[{status}] {message} {f'({docs} docs)' if docs else ''}")
    
    worker = GitHubWorker(progress_callback=print_progress)
    result = worker.sync(days_back=days)
    print(f"\nResult: {result}")

