#!/bin/bash
# Start Minna MCP server
# Usage: ./start_minna.sh

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

export PYTHONPATH="${SCRIPT_DIR}/src:${PYTHONPATH}"

# Try poetry first, fall back to python
if command -v poetry &> /dev/null; then
    exec poetry run python -m minna.mcp_server "$@"
else
    exec python -m minna.mcp_server "$@"
fi
