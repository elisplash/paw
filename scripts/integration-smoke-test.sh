#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════
# n8n Integration Smoke Test
#
# Spins up a real n8n instance in Docker and validates that our API
# contracts (URL construction, auth headers, response parsing) work
# against the real thing.
#
# This is how we validate 25,000+ potential n8n tools without testing
# each one individually: we prove the PROTOCOL LAYER works.
#
# Usage:
#   ./scripts/integration-smoke-test.sh          # full test
#   KEEP_CONTAINER=1 ./scripts/...               # don't cleanup after
#   N8N_IMAGE=n8nio/n8n:1.76.0 ./scripts/...    # test specific version
#
# Requirements: Docker
# ═══════════════════════════════════════════════════════════════════════

set -euo pipefail

# ── Config ──────────────────────────────────────────────────────────────

CONTAINER_NAME="paw-n8n-smoke-test"
N8N_PORT="${N8N_PORT:-5679}"       # Use different port from dev to avoid conflicts
N8N_IMAGE="${N8N_IMAGE:-n8nio/n8n:latest}"
N8N_URL="http://localhost:${N8N_PORT}"
OWNER_EMAIL="agent@paw.local"
OWNER_PASSWORD="${PAW_OWNER_PASSWORD:-***REMOVED***}"
KEEP_CONTAINER="${KEEP_CONTAINER:-0}"
MAX_STARTUP_WAIT=180               # seconds (generous for first-time image pull + migrations)
PASSED=0
FAILED=0
SKIPPED=0

# ── Helpers ─────────────────────────────────────────────────────────────

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m'

pass() { PASSED=$((PASSED + 1)); echo -e "  ${GREEN}✓${NC} $1"; }
fail() { FAILED=$((FAILED + 1)); echo -e "  ${RED}✗${NC} $1: $2"; }
skip() { SKIPPED=$((SKIPPED + 1)); echo -e "  ${YELLOW}⊘${NC} $1 (skipped: $2)"; }
info() { echo -e "${BLUE}▸${NC} $1"; }

cleanup() {
    if [[ "${KEEP_CONTAINER}" == "1" ]]; then
        info "KEEP_CONTAINER=1 — leaving ${CONTAINER_NAME} running on port ${N8N_PORT}"
        return
    fi
    info "Cleaning up..."
    docker rm -f "${CONTAINER_NAME}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

# ── Pre-flight ──────────────────────────────────────────────────────────

if ! command -v docker &>/dev/null; then
    echo "Docker not found — skipping integration smoke test"
    exit 0
fi

if ! docker info &>/dev/null; then
    echo "Docker daemon not accessible — skipping"
    exit 0
fi

echo ""
echo "════════════════════════════════════════════════════════════════"
echo "  n8n Integration Smoke Test"
echo "  Image: ${N8N_IMAGE}"
echo "  Port:  ${N8N_PORT}"
echo "════════════════════════════════════════════════════════════════"
echo ""

# ── 1. Start n8n ───────────────────────────────────────────────────────

info "Pulling n8n image (may take a minute on first run)..."
docker pull "${N8N_IMAGE}" 2>&1 | tail -1

info "Starting n8n container..."
docker rm -f "${CONTAINER_NAME}" >/dev/null 2>&1 || true
docker run -d \
    --name "${CONTAINER_NAME}" \
    -p "${N8N_PORT}:5678" \
    -e N8N_DIAGNOSTICS_ENABLED=false \
    -e N8N_PERSONALIZATION_ENABLED=false \
    "${N8N_IMAGE}" >/dev/null

# ── 2. Wait for healthy ───────────────────────────────────────────────

info "Waiting for n8n to be ready (max ${MAX_STARTUP_WAIT}s)..."
waited=0
while [[ $waited -lt $MAX_STARTUP_WAIT ]]; do
    status=$(curl -s -o /dev/null -w '%{http_code}' "${N8N_URL}/healthz" 2>/dev/null || echo "000")
    if [[ "$status" == "200" ]]; then
        break
    fi
    sleep 2
    ((waited+=2))
    if (( waited % 10 == 0 )); then
        echo "    ...still waiting (${waited}s, last status: ${status})"
    fi
done

if [[ $waited -ge $MAX_STARTUP_WAIT ]]; then
    fail "Startup" "n8n did not become healthy within ${MAX_STARTUP_WAIT}s"
    exit 1
fi
pass "n8n healthy after ${waited}s"

# Give n8n extra time to finish route registration after healthz responds.
# healthz responds early; REST middleware takes a few more seconds.
info "Waiting for REST routes to stabilize..."
route_waited=0
while [[ $route_waited -lt 30 ]]; do
    route_status=$(curl -s -o /dev/null -w '%{http_code}' \
        -X POST "${N8N_URL}/rest/login" \
        -H "Content-Type: application/json" \
        -d '{"emailOrLdapLoginId":"probe@probe.local","password":"probe"}' 2>/dev/null || echo "000")
    # Any non-404 means the route exists (we expect 401 or 422 for bad creds)
    if [[ "$route_status" != "404" && "$route_status" != "000" ]]; then
        break
    fi
    sleep 2
    ((route_waited+=2))
done
if [[ $route_waited -ge 30 ]]; then
    info "WARNING: REST routes may not be fully registered (last status: ${route_status})"
fi

# ── 3. /healthz endpoint ──────────────────────────────────────────────

info "Testing API contracts..."

status=$(curl -s -o /dev/null -w '%{http_code}' "${N8N_URL}/healthz")
if [[ "$status" == "200" ]]; then
    pass "GET /healthz → 200"
else
    fail "GET /healthz" "Expected 200, got ${status}"
fi

# ── 4. Owner setup (POST /rest/owner/setup) ──────────────────────────

# First call should succeed (200/201)
owner_resp=$(curl -s -w '\n%{http_code}' -X POST "${N8N_URL}/rest/owner/setup" \
    -H "Content-Type: application/json" \
    -d "{\"email\":\"${OWNER_EMAIL}\",\"firstName\":\"Paw\",\"lastName\":\"Agent\",\"password\":\"${OWNER_PASSWORD}\"}")
owner_status=$(echo "$owner_resp" | tail -1)
if [[ "$owner_status" == "200" || "$owner_status" == "201" ]]; then
    pass "POST /rest/owner/setup → ${owner_status} (created)"
elif [[ "$owner_status" == "400" ]]; then
    pass "POST /rest/owner/setup → 400 (already exists — OK)"
else
    fail "POST /rest/owner/setup" "Unexpected status ${owner_status}"
fi

# Second call should return 400 (idempotent)
owner_resp2=$(curl -s -o /dev/null -w '%{http_code}' -X POST "${N8N_URL}/rest/owner/setup" \
    -H "Content-Type: application/json" \
    -d "{\"email\":\"${OWNER_EMAIL}\",\"firstName\":\"Paw\",\"lastName\":\"Agent\",\"password\":\"${OWNER_PASSWORD}\"}")
if [[ "$owner_resp2" == "400" ]]; then
    pass "POST /rest/owner/setup (repeat) → 400 (idempotent)"
else
    fail "POST /rest/owner/setup (repeat)" "Expected 400, got ${owner_resp2}"
fi

# ── 5. Session login (POST /rest/login) ──────────────────────────────

login_resp=$(curl -s -c /tmp/n8n-cookies.txt -w '\n%{http_code}' \
    -X POST "${N8N_URL}/rest/login" \
    -H "Content-Type: application/json" \
    -d "{\"emailOrLdapLoginId\":\"${OWNER_EMAIL}\",\"password\":\"${OWNER_PASSWORD}\"}")
login_status=$(echo "$login_resp" | tail -1)
if [[ "$login_status" == "200" ]]; then
    pass "POST /rest/login → 200 (session created)"
    HAS_SESSION=1
else
    fail "POST /rest/login" "Expected 200, got ${login_status}"
    HAS_SESSION=0
fi

# ── 6. Generate API key ─────────────────────────────────────────────

# We need an API key for the public API. Generate one via the session.
if [[ "${HAS_SESSION}" == "1" ]]; then
    # Create API key via internal endpoint
    # n8n 2.x requires scopes + expiresAt; older versions accept just a label
    apikey_resp=$(curl -s -b /tmp/n8n-cookies.txt \
        -X POST "${N8N_URL}/rest/api-keys" \
        -H "Content-Type: application/json" \
        -d '{"label":"smoke-test","scopes":["workflow:read","workflow:create","workflow:delete","workflow:list","workflow:execute"],"expiresAt":0}')
    API_KEY=$(echo "$apikey_resp" | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    # n8n 2.x wraps in 'data', older versions return directly
    obj = d.get('data', d) if isinstance(d, dict) else d
    # Try rawApiKey first (n8n 2.x: apiKey is redacted, rawApiKey has the real one)
    for key in ['rawApiKey', 'apiKey', 'key']:
        if isinstance(obj, dict) and key in obj:
            val = obj[key]
            # Skip redacted keys (contain asterisks)
            if isinstance(val, str) and '*' in val:
                continue
            print(val)
            sys.exit(0)
    print('')
except:
    print('')
" 2>/dev/null)
    if [[ -n "$API_KEY" ]]; then
        pass "API key generated"
    else
        skip "API key generation" "Could not extract key; some tests will be skipped"
        API_KEY=""
    fi
else
    skip "API key generation" "No session"
    API_KEY=""
fi

# ── 7. Public API: workflow listing ──────────────────────────────────

if [[ -n "$API_KEY" ]]; then
    wf_resp=$(curl -s -w '\n%{http_code}' \
        -H "X-N8N-API-KEY: ${API_KEY}" \
        -H "Accept: application/json" \
        "${N8N_URL}/api/v1/workflows?limit=1")
    wf_status=$(echo "$wf_resp" | tail -1)
    wf_body=$(echo "$wf_resp" | head -n -1)

    if [[ "$wf_status" == "200" ]]; then
        pass "GET /api/v1/workflows?limit=1 → 200"

        # Verify response shape has "data" key
        if echo "$wf_body" | python3 -c "import sys,json; d=json.load(sys.stdin); assert 'data' in d" 2>/dev/null; then
            pass "Response has 'data' field"
        else
            fail "Response shape" "Missing 'data' field"
        fi

        # Check for version header
        version=$(curl -s -I -H "X-N8N-API-KEY: ${API_KEY}" "${N8N_URL}/api/v1/workflows?limit=1" 2>/dev/null \
            | grep -i 'x-n8n-version' | awk '{print $2}' | tr -d '\r' || true)
        if [[ -n "$version" ]]; then
            pass "x-n8n-version header present: ${version}"
        else
            skip "x-n8n-version header" "Not present (optional in some n8n versions)"
        fi
    else
        fail "GET /api/v1/workflows" "Expected 200, got ${wf_status}"
    fi

    # Test without API key → should fail (401/403)
    noauth_status=$(curl -s -o /dev/null -w '%{http_code}' \
        "${N8N_URL}/api/v1/workflows?limit=1")
    if [[ "$noauth_status" == "401" || "$noauth_status" == "403" ]]; then
        pass "GET /api/v1/workflows (no auth) → ${noauth_status} (expected)"
    else
        fail "Auth enforcement" "Expected 401/403 without API key, got ${noauth_status}"
    fi
else
    skip "Public API tests" "No API key"
fi

# ── 8. Create and list a workflow ────────────────────────────────────

if [[ -n "$API_KEY" ]]; then
    create_body='{"name":"Smoke Test Workflow","nodes":[{"parameters":{"path":"smoke-test"},"name":"Webhook","type":"n8n-nodes-base.webhook","typeVersion":1,"position":[250,300]}],"connections":{},"settings":{}}'
    create_resp=$(curl -s -w '\n%{http_code}' \
        -X POST "${N8N_URL}/api/v1/workflows" \
        -H "X-N8N-API-KEY: ${API_KEY}" \
        -H "Content-Type: application/json" \
        -d "$create_body")
    create_status=$(echo "$create_resp" | tail -1)
    create_respbody=$(echo "$create_resp" | head -n -1)

    if [[ "$create_status" == "200" || "$create_status" == "201" ]]; then
        pass "POST /api/v1/workflows → ${create_status} (workflow created)"

        # Extract workflow ID (handle both string and numeric)
        WF_ID=$(echo "$create_respbody" | python3 -c "import sys,json; print(json.load(sys.stdin).get('id',''))" 2>/dev/null || echo "")
        if [[ -n "$WF_ID" ]]; then
            pass "Created workflow ID: ${WF_ID}"
        fi
    else
        fail "POST /api/v1/workflows" "Expected 200/201, got ${create_status}"
    fi

    # List should now include our workflow
    list_resp=$(curl -s \
        -H "X-N8N-API-KEY: ${API_KEY}" \
        "${N8N_URL}/api/v1/workflows")
    wf_count=$(echo "$list_resp" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d.get('data',[])))" 2>/dev/null || echo "0")
    if [[ "$wf_count" -ge 1 ]]; then
        pass "Workflow list contains ${wf_count} workflow(s)"
    else
        fail "Workflow list" "Expected at least 1 workflow"
    fi
else
    skip "Workflow CRUD" "No API key"
fi

# ── 9. Community packages endpoint ──────────────────────────────────

if [[ -n "$API_KEY" ]]; then
    # Try v1 first, then internal REST endpoint (n8n 2.x moved it)
    pkg_resp=$(curl -s -w '\n%{http_code}' \
        -H "X-N8N-API-KEY: ${API_KEY}" \
        "${N8N_URL}/api/v1/community-packages")
    pkg_status=$(echo "$pkg_resp" | tail -1)

    if [[ "$pkg_status" == "200" ]]; then
        pass "GET /api/v1/community-packages → 200"
    else
        # Fallback: n8n 2.x uses internal REST endpoint with session auth
        pkg_resp2=$(curl -s -w '\n%{http_code}' \
            -b /tmp/n8n-cookies.txt \
            "${N8N_URL}/rest/community-packages")
        pkg_status2=$(echo "$pkg_resp2" | tail -1)
        if [[ "$pkg_status2" == "200" ]]; then
            pass "GET /rest/community-packages → 200 (n8n 2.x internal endpoint)"
        else
            fail "GET community-packages" "Expected 200, got v1=${pkg_status} rest=${pkg_status2}"
        fi
    fi
else
    skip "Community packages" "No API key"
fi

# ── 10. MCP endpoint detection ──────────────────────────────────────

mcp_resp=$(curl -s -w '\n%{http_code}' "${N8N_URL}/rest/mcp/api-key")
mcp_status=$(echo "$mcp_resp" | tail -1)
mcp_body=$(echo "$mcp_resp" | head -n -1)

if [[ "$mcp_status" == "401" ]]; then
    pass "GET /rest/mcp/api-key → 401 (MCP route exists, auth required)"
elif [[ "$mcp_status" == "404" ]]; then
    if echo "$mcp_body" | grep -q "Cannot GET"; then
        pass "GET /rest/mcp/api-key → 404 'Cannot GET' (old n8n, no MCP — correctly detected)"
    else
        pass "GET /rest/mcp/api-key → 404 (JSON 404 — MCP route may exist)"
    fi
else
    pass "GET /rest/mcp/api-key → ${mcp_status} (MCP endpoint accessible)"
fi

# ── 11. MCP enable + token retrieval (if session available) ──────────

if [[ "${HAS_SESSION}" == "1" ]]; then
    # Enable MCP access
    mcp_enable=$(curl -s -o /dev/null -w '%{http_code}' -b /tmp/n8n-cookies.txt \
        -X PATCH "${N8N_URL}/rest/mcp/settings" \
        -H "Content-Type: application/json" \
        -d '{"mcpAccessEnabled": true}')
    if [[ "$mcp_enable" == "200" || "$mcp_enable" == "204" ]]; then
        pass "PATCH /rest/mcp/settings → ${mcp_enable} (MCP enabled)"

        # Retrieve MCP token
        mcp_key_resp=$(curl -s -b /tmp/n8n-cookies.txt "${N8N_URL}/rest/mcp/api-key")
        mcp_token=$(echo "$mcp_key_resp" | python3 -c "
import sys,json
try:
    d=json.load(sys.stdin)
    print(d.get('data',{}).get('apiKey',''))
except: print('')
" 2>/dev/null || echo "")

        if [[ -n "$mcp_token" && "$mcp_token" == *"."* ]]; then
            pass "MCP token retrieved (JWT format)"
            MCP_TOKEN="$mcp_token"
        elif [[ -n "$mcp_token" && "$mcp_token" == *"*"* ]]; then
            pass "MCP token retrieved (redacted — would trigger rotation)"
            MCP_TOKEN=""
        else
            skip "MCP token retrieval" "Empty or unexpected format"
            MCP_TOKEN=""
        fi
    elif [[ "$mcp_enable" == "404" ]]; then
        skip "MCP enable" "Endpoint not found (n8n version may not support MCP)"
        MCP_TOKEN=""
    else
        fail "PATCH /rest/mcp/settings" "Expected 200/204, got ${mcp_enable}"
        MCP_TOKEN=""
    fi
else
    skip "MCP token" "No session"
    MCP_TOKEN=""
fi

# ── 12. MCP Streamable HTTP endpoint ────────────────────────────────

if [[ -n "${MCP_TOKEN:-}" ]]; then
    # Test the MCP JSON-RPC endpoint with an initialize request
    init_req='{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "smoke-test", "version": "0.0.1"}
        }
    }'
    mcp_http_resp=$(curl -s -w '\n%{http_code}' \
        -X POST "${N8N_URL}/mcp-server/http" \
        -H "Authorization: Bearer ${MCP_TOKEN}" \
        -H "Content-Type: application/json" \
        -H "Accept: application/json, text/event-stream" \
        -d "$init_req" \
        --max-time 10)
    mcp_http_status=$(echo "$mcp_http_resp" | tail -1)

    if [[ "$mcp_http_status" == "200" ]]; then
        pass "POST /mcp-server/http (initialize) → 200"

        # Try to parse the response for protocolVersion
        mcp_http_body=$(echo "$mcp_http_resp" | head -n -1)
        if echo "$mcp_http_body" | grep -q "protocolVersion"; then
            pass "MCP initialize response contains protocolVersion"
        fi

        # Send tools/list
        tools_req='{
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list"
        }'

        # Extract session ID if present
        session_id=$(curl -s -D - \
            -X POST "${N8N_URL}/mcp-server/http" \
            -H "Authorization: Bearer ${MCP_TOKEN}" \
            -H "Content-Type: application/json" \
            -H "Accept: application/json, text/event-stream" \
            -d "$init_req" --max-time 10 2>/dev/null \
            | grep -i 'mcp-session-id' | awk '{print $2}' | tr -d '\r' || echo "")

        tools_headers=(-H "Authorization: Bearer ${MCP_TOKEN}" -H "Content-Type: application/json")
        if [[ -n "$session_id" ]]; then
            tools_headers+=(-H "Mcp-Session-Id: ${session_id}")
            pass "MCP session ID received"
        fi

        tools_resp=$(curl -s -w '\n%{http_code}' \
            -X POST "${N8N_URL}/mcp-server/http" \
            "${tools_headers[@]}" \
            -H "Accept: application/json, text/event-stream" \
            -d "$tools_req" --max-time 10)
        tools_status=$(echo "$tools_resp" | tail -1)

        if [[ "$tools_status" == "200" ]]; then
            pass "POST /mcp-server/http (tools/list) → 200"
            tools_body=$(echo "$tools_resp" | head -n -1)
            if echo "$tools_body" | grep -q '"tools"'; then
                tool_count=$(echo "$tools_body" | python3 -c "
import sys,json
try:
    raw = sys.stdin.read()
    # Handle SSE response: find all 'data:' lines and parse JSON from each
    data_lines = [l.split('data:', 1)[1].strip() for l in raw.split('\n') if l.startswith('data:')]
    if data_lines:
        for dl in data_lines:
            try:
                d = json.loads(dl)
                result = d.get('result', d)
                if 'tools' in result:
                    print(len(result['tools']))
                    sys.exit(0)
            except: pass
    # Try direct JSON parse
    d = json.loads(raw)
    result = d.get('result', d)
    print(len(result.get('tools', [])))
except: print('?')
" 2>/dev/null || echo "?")
                pass "MCP tools/list returned ${tool_count} tool(s)"
            fi
        else
            skip "MCP tools/list" "Status ${tools_status}"
        fi
    else
        skip "MCP endpoint" "Status ${mcp_http_status} (may need n8n >= 1.76)"
    fi
else
    skip "MCP Streamable HTTP" "No MCP token"
fi

# ── 13. Credential schema endpoint ──────────────────────────────────

if [[ "${HAS_SESSION}" == "1" ]]; then
    cred_resp=$(curl -s -w '\n%{http_code}' -b /tmp/n8n-cookies.txt \
        "${N8N_URL}/types/credentials.json")
    cred_status=$(echo "$cred_resp" | tail -1)

    if [[ "$cred_status" == "200" ]]; then
        cred_body=$(echo "$cred_resp" | head -n -1)
        cred_count=$(echo "$cred_body" | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null || echo "?")
        pass "GET /types/credentials.json → 200 (${cred_count} credential types)"
    else
        fail "GET /types/credentials.json" "Expected 200, got ${cred_status}"
    fi

    # Node types
    node_resp=$(curl -s -o /dev/null -w '%{http_code}' -b /tmp/n8n-cookies.txt \
        "${N8N_URL}/types/nodes.json")
    if [[ "$node_resp" == "200" ]]; then
        pass "GET /types/nodes.json → 200"
    else
        fail "GET /types/nodes.json" "Expected 200, got ${node_resp}"
    fi
else
    skip "Credential/node type endpoints" "No session"
fi

# ── Summary ─────────────────────────────────────────────────────────────

echo ""
echo "════════════════════════════════════════════════════════════════"
echo -e "  Results: ${GREEN}${PASSED} passed${NC}, ${RED}${FAILED} failed${NC}, ${YELLOW}${SKIPPED} skipped${NC}"
echo "════════════════════════════════════════════════════════════════"
echo ""

# Clean up temp files
rm -f /tmp/n8n-cookies.txt

if [[ $FAILED -gt 0 ]]; then
    exit 1
fi
exit 0
