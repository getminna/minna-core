import sys
import argparse
from minna.core.vector_db import VectorManager

def main():
    parser = argparse.ArgumentParser(description="Search Minna Vector Database")
    parser.add_argument("query", help="The search query text")
    parser.add_argument("--limit", type=int, default=5, help="Number of results to return (default: 5)")
    args = parser.parse_args()

    try:
        vm = VectorManager()
        # Search now returns a dict with 'results' and 'search_strategy'
        search_response = vm.search(args.query, limit=args.limit)
        results = search_response.get("results", [])
        strategy = search_response.get("search_strategy", "unknown")

        if not results:
            print("No results found.")
            return

        # Print Headers based on strategy
        if strategy == "keyword":
            print("\n🔍 No strong conceptual matches, but found these mentions.\n")
        elif strategy == "weak_match":
            print("\n⚠️ Low Confidence / Related Concepts.\n")
        elif strategy == "strong_match":
            print("\n✅ Top semantic matches:\n")
        
        for res in results:
            # Check if distance is available, otherwise just print standard info
            score_display = ""
            if "distance" in res:
                score_display = f"[Score: {res['distance']:.4f}] " 

            source = res.get("source", "Unknown Source")
            content = res.get("content", "").strip()
            metadata = res.get("metadata", {})
            
            # Metadata Info
            meta_info = []
            if "channel_name" in metadata:
                meta_info.append(f"#{metadata['channel_name']}")
            
            # Prefer Real Name -> ID
            user_display = metadata.get("user_real_name") or metadata.get("user")
            if user_display:
                meta_info.append(f"@{user_display}")

            if "ts" in metadata:
                meta_info.append(f"TS:{metadata['ts']}")
                
            meta_str = f" ({' | '.join(meta_info)})" if meta_info else ""
            
            print(f"{score_display}Source: {source}{meta_str}")
            print(f"{content}")
            print("-" * 40)

    except Exception as e:
        print(f"Error encountered: {e}")
        sys.exit(1)

if __name__ == "__main__":
    main()
