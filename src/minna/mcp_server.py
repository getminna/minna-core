"""
Minna MCP Server - Universal Memory Interface

Exposes local vector database to Cursor, Claude Desktop, and other MCP-compatible clients.
Provides a single `get_context` tool that searches across all ingested sources
(Slack, Linear, GitHub, etc.) without requiring the LLM to know which source to query.
"""

from datetime import datetime
from mcp.server.fastmcp import FastMCP
from minna.core.vector_db import VectorManager

# Initialize the MCP Server
mcp = FastMCP("Minna Context Engine")

# Initialize the Vector DB on startup
# VectorManager loads the Nomic embedding model and connects to SQLite
db = VectorManager()


def format_timestamp(ts: str | float | None) -> str:
    """
    Converts a raw Unix timestamp to human-readable YYYY-MM-DD HH:MM format.
    
    Args:
        ts: Unix timestamp as string or float, or None.
        
    Returns:
        Formatted date string, or "Unknown" if conversion fails.
    """
    if ts is None:
        return "Unknown"
    
    try:
        # Handle string timestamps (common from Slack)
        if isinstance(ts, str):
            # Slack timestamps can be "1234567890.123456"
            ts = float(ts.split(".")[0])
        
        dt = datetime.fromtimestamp(ts)
        return dt.strftime("%Y-%m-%d %H:%M")
    except (ValueError, TypeError, OSError):
        return "Unknown"


@mcp.tool()
def get_context(query: str, limit: int = 5) -> str:
    """
    Retrieves relevant context from the user's local workspace (Slack, Linear, etc.).
    Use this to answer questions about past decisions, discussions, or tickets.
    
    Args:
        query: The semantic search query (e.g., "Why did we choose GraphQL?").
        limit: Number of results to return (default: 5).
        
    Returns:
        A Markdown string with the most relevant context found.
    """
    # 1. Perform Hybrid Search (Vector + Keyword Fallback)
    result_data = db.search(query=query, limit=limit)
    
    results = result_data.get("results", [])
    strategy = result_data.get("search_strategy", "unknown")
    
    # Map strategy to human-readable method name
    strategy_labels = {
        "strong_match": "Vector",
        "keyword": "Keyword",
        "weak_match": "Vector-Weak",
        "no_results": "None"
    }
    search_method = strategy_labels.get(strategy, strategy)
    
    # 2. Handle empty results
    if not results:
        return "No relevant local context found for this query."
    
    # 3. Format each result using the Agent-Friendly template
    markdown_parts = []
    
    for doc in results:
        content = doc.get("content", "No content available")
        metadata = doc.get("metadata", {})
        source = doc.get("source", "Unknown")
        distance = doc.get("distance", 0.0)
        
        # Calculate relevance score (inverse of distance, capped at 1.0)
        # Lower distance = higher relevance
        relevance = max(0.0, min(1.0, 1.0 - distance))
        
        # Extract display fields from metadata
        channel = metadata.get("channel_name") or metadata.get("title") or "General"
        timestamp = metadata.get("ts") or metadata.get("timestamp")
        formatted_date = format_timestamp(timestamp)
        
        # Build the formatted block
        block = f"""### [Source: {source}] {channel}
**Date:** {formatted_date}
**Relevance:** {relevance:.2f} ({search_method})

{content}
---"""
        markdown_parts.append(block)
    
    return "\n\n".join(markdown_parts)


if __name__ == "__main__":
    mcp.run()
