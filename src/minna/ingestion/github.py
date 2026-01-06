"""
GitHub Connector - Issues, PRs, and Comments sync for Minna context engine.

Fetches:
- Issue comments
- PR review comments  
- Discussion threads

Uses GitHub REST API with Personal Access Token.

Focus on comments (per connector priority) to capture:
- Decision context
- Code review discussions
- Team communications
"""

import time
from datetime import datetime, timedelta
from typing import List, Dict, Optional, Callable
import requests
from .base import BaseConnector, Document


class GitHubConnector(BaseConnector):
    """
    Syncs GitHub issues, PRs, and comments to the Minna vector database.
    Uses REST API with Fine-grained Personal Access Token.
    """
    
    BASE_URL = "https://api.github.com"
    
    def __init__(self, pat: str, progress_callback: Optional[Callable] = None):
        """
        Initialize GitHubConnector.
        
        Args:
            pat: Personal Access Token (github_pat_ or ghp_)
            progress_callback: Callback for progress updates
        """
        self.pat = pat
        self.progress_callback = progress_callback or (lambda *args, **kwargs: None)
        self._docs_processed = 0
        self._user_cache: Dict[str, str] = {}
        
    def _headers(self) -> Dict[str, str]:
        """Standard auth headers for GitHub API requests."""
        return {
            "Authorization": f"Bearer {self.pat}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
        }
    
    def _api_get(self, endpoint: str, params: Dict = None) -> Optional[Dict | List]:
        """Make an authenticated GET request to GitHub API."""
        url = f"{self.BASE_URL}{endpoint}" if endpoint.startswith("/") else endpoint
        
        try:
            response = requests.get(url, headers=self._headers(), params=params, timeout=30)
            
            # Rate limiting
            remaining = int(response.headers.get("X-RateLimit-Remaining", 100))
            if remaining < 10:
                reset_time = int(response.headers.get("X-RateLimit-Reset", 0))
                wait_seconds = max(reset_time - int(time.time()), 0) + 1
                self.progress_callback("waiting", f"Rate limit low, waiting {wait_seconds}s...")
                time.sleep(min(wait_seconds, 60))
            
            if response.status_code == 401:
                self.progress_callback("error", "Invalid or expired PAT")
                return None
            elif response.status_code == 403:
                self.progress_callback("error", "Insufficient permissions or rate limited")
                return None
            elif response.status_code == 404:
                return None  # Resource not found, skip silently
            elif response.status_code != 200:
                self.progress_callback("warning", f"API error: {response.status_code}")
                return None
                
            return response.json()
        except requests.RequestException as e:
            self.progress_callback("error", f"Network error: {e}")
            return None
    
    def _paginate(self, endpoint: str, params: Dict = None, max_items: int = 100) -> List[Dict]:
        """Paginate through GitHub API results."""
        items = []
        params = params or {}
        params["per_page"] = min(100, max_items)
        page = 1
        
        while len(items) < max_items:
            params["page"] = page
            data = self._api_get(endpoint, params)
            
            if not data or len(data) == 0:
                break
                
            items.extend(data)
            
            if len(data) < params["per_page"]:
                break
                
            page += 1
            time.sleep(0.1)  # Rate limit courtesy
            
        return items[:max_items]
    
    # =========================================================================
    # REPOSITORY DISCOVERY
    # =========================================================================
    
    def _fetch_repos(self, max_repos: int = 20) -> List[Dict]:
        """Fetch repositories the user has access to."""
        self.progress_callback("fetching", "Discovering repositories...")
        
        # Get repos the user owns or has push access to
        repos = self._paginate("/user/repos", {
            "sort": "pushed",
            "direction": "desc",
            "affiliation": "owner,collaborator,organization_member"
        }, max_repos)
        
        self.progress_callback("fetching", f"Found {len(repos)} repositories")
        return repos
    
    # =========================================================================
    # ISSUE COMMENTS
    # =========================================================================
    
    def _fetch_issue_comments(self, owner: str, repo: str, since: datetime, max_comments: int = 100) -> List[Document]:
        """Fetch issue comments from a repository."""
        documents = []
        
        comments = self._paginate(
            f"/repos/{owner}/{repo}/issues/comments",
            {"since": since.isoformat() + "Z", "sort": "updated", "direction": "desc"},
            max_comments
        )
        
        for comment in comments:
            doc = self._comment_to_document(comment, owner, repo, "issue")
            if doc:
                documents.append(doc)
                
        return documents
    
    def _comment_to_document(self, comment: Dict, owner: str, repo: str, comment_type: str) -> Optional[Document]:
        """Convert a GitHub comment to a Minna Document."""
        comment_id = comment.get("id")
        body = comment.get("body", "")
        
        if not body or len(body.strip()) < 10:
            return None  # Skip empty or trivial comments
            
        user = comment.get("user", {})
        author = user.get("login", "unknown")
        
        created_at = comment.get("created_at", "")
        updated_at = comment.get("updated_at", "")
        
        # Get issue/PR context
        issue_url = comment.get("issue_url", "")
        html_url = comment.get("html_url", "")
        
        # Build searchable content
        content = f"GitHub {comment_type} comment by @{author}:\n\n{body}"
        
        return Document(
            source=f"github_{comment_type}_comment",
            content=content,
            metadata={
                "comment_id": comment_id,
                "repository": f"{owner}/{repo}",
                "author": author,
                "created_at": created_at,
                "updated_at": updated_at,
                "issue_url": issue_url,
                "html_url": html_url,
                "comment_type": comment_type,
            }
        )
    
    # =========================================================================
    # PR REVIEW COMMENTS
    # =========================================================================
    
    def _fetch_pr_comments(self, owner: str, repo: str, since: datetime, max_comments: int = 100) -> List[Document]:
        """Fetch PR review comments from a repository."""
        documents = []
        
        comments = self._paginate(
            f"/repos/{owner}/{repo}/pulls/comments",
            {"since": since.isoformat() + "Z", "sort": "updated", "direction": "desc"},
            max_comments
        )
        
        for comment in comments:
            doc = self._pr_comment_to_document(comment, owner, repo)
            if doc:
                documents.append(doc)
                
        return documents
    
    def _pr_comment_to_document(self, comment: Dict, owner: str, repo: str) -> Optional[Document]:
        """Convert a PR review comment to a Minna Document."""
        comment_id = comment.get("id")
        body = comment.get("body", "")
        
        if not body or len(body.strip()) < 10:
            return None
            
        user = comment.get("user", {})
        author = user.get("login", "unknown")
        
        # Code review context
        path = comment.get("path", "")
        diff_hunk = comment.get("diff_hunk", "")
        
        created_at = comment.get("created_at", "")
        html_url = comment.get("html_url", "")
        
        # Build searchable content with code context
        content_parts = [
            f"GitHub PR review comment by @{author}",
        ]
        
        if path:
            content_parts.append(f"File: {path}")
            
        if diff_hunk:
            content_parts.append(f"Code context:\n```\n{diff_hunk[:500]}\n```")
            
        content_parts.append(f"\nComment:\n{body}")
        
        content = "\n".join(content_parts)
        
        return Document(
            source="github_pr_review_comment",
            content=content,
            metadata={
                "comment_id": comment_id,
                "repository": f"{owner}/{repo}",
                "author": author,
                "file_path": path,
                "created_at": created_at,
                "html_url": html_url,
            }
        )
    
    # =========================================================================
    # ISSUES (WITH BODY)
    # =========================================================================
    
    def _fetch_issues(self, owner: str, repo: str, since: datetime, max_issues: int = 50) -> List[Document]:
        """Fetch issues from a repository."""
        documents = []
        
        issues = self._paginate(
            f"/repos/{owner}/{repo}/issues",
            {"since": since.isoformat() + "Z", "state": "all", "sort": "updated"},
            max_issues
        )
        
        for issue in issues:
            # Skip PRs (they come through the issues endpoint too)
            if "pull_request" in issue:
                continue
                
            doc = self._issue_to_document(issue, owner, repo)
            if doc:
                documents.append(doc)
                
        return documents
    
    def _issue_to_document(self, issue: Dict, owner: str, repo: str) -> Optional[Document]:
        """Convert a GitHub issue to a Minna Document."""
        issue_number = issue.get("number")
        title = issue.get("title", "")
        body = issue.get("body", "") or ""
        
        user = issue.get("user", {})
        author = user.get("login", "unknown")
        
        state = issue.get("state", "open")
        labels = [l.get("name", "") for l in issue.get("labels", [])]
        
        created_at = issue.get("created_at", "")
        html_url = issue.get("html_url", "")
        
        # Build searchable content
        content_parts = [
            f"GitHub Issue #{issue_number}: {title}",
            f"Status: {state}",
            f"Author: @{author}",
        ]
        
        if labels:
            content_parts.append(f"Labels: {', '.join(labels)}")
            
        if body:
            content_parts.append(f"\n{body[:2000]}")
        
        content = "\n".join(content_parts)
        
        return Document(
            source="github_issue",
            content=content,
            metadata={
                "issue_number": issue_number,
                "repository": f"{owner}/{repo}",
                "title": title,
                "author": author,
                "state": state,
                "labels": labels,
                "created_at": created_at,
                "html_url": html_url,
            }
        )
    
    # =========================================================================
    # MAIN SYNC
    # =========================================================================
    
    def sync(self, days_back: int = 30, max_repos: int = 10) -> List[Document]:
        """
        Full sync: Issues, PRs, and Comments across repositories.
        
        Args:
            days_back: How many days of history to sync
            max_repos: Maximum number of repositories to sync
            
        Returns:
            List of Documents from GitHub
        """
        all_documents = []
        since = datetime.utcnow() - timedelta(days=days_back)
        
        # Get repositories
        repos = self._fetch_repos(max_repos)
        total_repos = len(repos)
        
        for i, repo in enumerate(repos, 1):
            owner = repo.get("owner", {}).get("login", "")
            repo_name = repo.get("name", "")
            full_name = f"{owner}/{repo_name}"
            
            self.progress_callback(
                "syncing",
                f"{full_name} ({i}/{total_repos})",
                documents_processed=len(all_documents)
            )
            
            try:
                # Fetch issue comments (highest priority per spec)
                issue_comments = self._fetch_issue_comments(owner, repo_name, since, max_comments=50)
                all_documents.extend(issue_comments)
                
                # Fetch PR review comments
                pr_comments = self._fetch_pr_comments(owner, repo_name, since, max_comments=50)
                all_documents.extend(pr_comments)
                
                # Fetch issues
                issues = self._fetch_issues(owner, repo_name, since, max_issues=30)
                all_documents.extend(issues)
                
            except Exception as e:
                self.progress_callback("warning", f"Error syncing {full_name}: {e}")
                continue
            
            time.sleep(0.5)  # Rate limit between repos
        
        self.progress_callback(
            "syncing",
            f"GitHub sync complete: {len(all_documents)} items",
            documents_processed=len(all_documents)
        )
        
        return all_documents

