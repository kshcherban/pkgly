#!/bin/bash
# ABOUTME: Runs the host-side MCP client against pkglyctl inside the integration service.
# ABOUTME: Preserves the Compose configuration selected by the integration orchestrator.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 "${SCRIPT_DIR}/test_mcp.py" "$@"
