import os
import sys
import json
from pathlib import Path
from dotenv import load_dotenv
import argparse
import time

from minna.core.vector_db import VectorManager
from minna.ingestion.slack import SlackConnector
from minna.utils.paths import get_data_path, is_frozen

def load_config():
    """Loads environment variables from .env file."""
    # 1. Determine where to look for .env
    if is_frozen():
        # In bundled mode, look in the App Support folder
        env_path = get_data_path(".env")
    else:
        # In dev mode, look in current directory (or src/minna)
        env_path = Path(os.getcwd()) / ".env"

    print(f"🔍 Looking for .env at: {env_path}")

    # 2. Force load from the explicit path
    if env_path.exists():
        print("✅ Found .env file on disk.")
        load_dotenv(dotenv_path=env_path, override=True)
    else:
        # It's okay if it's missing in some contexts (like checking setup for broken state),
        # but we log it.
        print(f"⚠️ .env file not found at {env_path}")
        print(f"📂 Files in directory: {os.listdir(env_path.parent) if env_path.parent.exists() else 'Directory not found'}")

    # 3. Sanitize token
    token = os.getenv("SLACK_USER_TOKEN", "")
    if token:
        token = token.strip().replace("–", "-").replace("—", "-")
        os.environ["SLACK_USER_TOKEN"] = token
        print(f"✅ Loaded SLACK_USER_TOKEN (Length: {len(token)})")
    else:
        print("ℹ️ SLACK_USER_TOKEN not currently set in environment.")

def check_setup():
    """Checks the setup status and returns a JSON object."""
    status = {
        "slack_auth": False,
        "db_initialized": False,
        "env_path": str(get_data_path(".env")) if is_frozen() else str(Path(os.getcwd()) / ".env"),
        "db_path": str(get_data_path("minna.db"))
    }
    
    # Check Slack Token
    token = os.getenv("SLACK_USER_TOKEN")
    if token and len(token) > 10: # Basic validity check
        status["slack_auth"] = True
        
    # Check Database
    db_path = get_data_path("minna.db")
    if db_path.exists():
        status["db_initialized"] = True
        
    print(json.dumps(status))

def main():
    # 1. Parse CLI Arguments
    parser = argparse.ArgumentParser(description="Minna Engine Runner")
    parser.add_argument("--days", type=int, default=7, help="Number of days to sync (default: 7)")
    parser.add_argument("--backfill", action="store_true", default=False, help="Enable backfill mode")
    parser.add_argument("--channels", nargs="+", help="List of specific channel IDs to sync")
    parser.add_argument("--check-setup", action="store_true", help="Check setup status and exit with JSON")
    args = parser.parse_args()

    # 2. Load Env
    load_config()

    # 3. Handle Setup Check
    if args.check_setup:
        check_setup()
        return

    # 4. Validate Token for Normal Config
    slack_token = os.getenv("SLACK_USER_TOKEN")
    if not slack_token:
        print("❌ ERROR: SLACK_USER_TOKEN variable is empty.")
        print("Check: Did you save the .env file? Is the variable name spelled correctly?")
        sys.exit(1)

    print("Initializing Minna Engine...")
    
    try:
        # 5. Initialize Components
        vector_db = VectorManager()
        slack_connector = SlackConnector(slack_token=slack_token)
        
        # 6. Running Sync
        days_back = args.days
        since_timestamp = time.time() - (days_back * 24 * 3600)
        
        mode_str = "BACKFILL" if args.backfill else "NORMAL"
        print(f"Starting {mode_str} sync for the last {days_back} days...")
        documents = slack_connector.sync(since_timestamp=since_timestamp, channel_ids=args.channels)
        
        if not documents:
            print("No new documents found.")
            return

        print(f"Found {len(documents)} documents. Indexing...")
        
        # 7. Insert into DB
        vector_db.add_documents(documents)
        
        print("Sync and Indexing Complete.")
        
    except Exception as e:
        print(f"An error occurred: {e}")
        sys.exit(1)

if __name__ == "__main__":
    main()
