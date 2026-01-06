"""
Minna CLI - Standardized entry point for Swift process invocation.

Usage:
    python -m minna.cli sync --provider slack
    python -m minna.cli sync --provider linear
    python -m minna.cli sync --provider github

This avoids constructing Python scripts as strings in Swift,
providing type safety and easier debugging.
"""

import argparse
import sys
import json


def emit_progress(status: str, message: str, documents_processed: int = None, total_documents: int = None):
    """
    Emit structured progress for Swift to parse.
    
    Format: MINNA_PROGRESS:{"status": "...", "message": "...", "documents_processed": N, "total_documents": M}
    
    Swift parses lines starting with MINNA_PROGRESS: to update
    the specific provider's state in real-time.
    """
    payload = {
        "status": status,
        "message": message
    }
    if documents_processed is not None:
        payload["documents_processed"] = documents_processed
    if total_documents is not None:
        payload["total_documents"] = total_documents
        
    print(f"MINNA_PROGRESS:{json.dumps(payload)}", flush=True)


def sync_provider(provider: str):
    """
    Run sync for the specified provider.
    
    Args:
        provider: One of 'slack', 'linear', 'github', 'google'
    """
    emit_progress("starting", f"Initializing {provider.title()} sync...")
    
    try:
        if provider == "slack":
            from minna.workers.slack_worker import SlackWorker
            worker = SlackWorker(progress_callback=emit_progress)
            
            result = worker.sync()
            emit_progress("complete", "Slack sync complete", documents_processed=result.get("documents", 0))
            
        elif provider == "google":
            from minna.workers.google_worker import GoogleWorker
            worker = GoogleWorker(progress_callback=emit_progress)
            
            result = worker.sync()
            emit_progress("complete", "Google Workspace sync complete", documents_processed=result.get("documents", 0))
            
        elif provider == "github":
            from minna.workers.github_worker import GitHubWorker
            worker = GitHubWorker(progress_callback=emit_progress)
            
            result = worker.sync()
            emit_progress("complete", "GitHub sync complete", documents_processed=result.get("documents", 0))
            
        else:
            emit_progress("error", f"Unknown provider: {provider}")
            sys.exit(1)
            
    except ImportError as e:
        # Worker doesn't exist yet
        emit_progress("error", f"Worker not implemented: {e}")
        sys.exit(1)
        
    except Exception as e:
        # Catch-all to prevent zombie processes
        emit_progress("error", str(e))
        sys.exit(1)


def main():
    parser = argparse.ArgumentParser(
        description="Minna CLI - Local-first context engine"
    )
    
    subparsers = parser.add_subparsers(dest="command", help="Available commands")
    
    # Sync command
    sync_parser = subparsers.add_parser("sync", help="Sync a provider to local DB")
    sync_parser.add_argument(
        "--provider",
        required=True,
        choices=["slack", "google", "github"],
        help="Provider to sync"
    )
    
    args = parser.parse_args()
    
    if args.command == "sync":
        sync_provider(args.provider)
    else:
        parser.print_help()
        sys.exit(1)


if __name__ == "__main__":
    main()


