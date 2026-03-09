#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "ERROR: required command '$1' not found"
    exit 1
  fi
}

require_cmd cargo
require_cmd python3

echo "== Build cratebay-mcp =="
cargo build -p cratebay-mcp

bin="$repo_root/target/debug/cratebay-mcp"
if [[ ! -x "$bin" ]]; then
  echo "ERROR: cratebay-mcp binary not found at $bin"
  exit 1
fi

echo "== MCP stdio handshake + tool listing =="
python3 - <<'PY'
import json
import os
import select
import subprocess
import sys
import time

BIN = os.environ.get("CRATEBAY_MCP_BIN", "").strip() or os.path.join(
    os.getcwd(), "target", "debug", "cratebay-mcp"
)

proc = subprocess.Popen(
    [BIN],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
    text=True,
)

def send(obj: dict) -> None:
    line = json.dumps(obj, separators=(",", ":"))
    assert proc.stdin is not None
    proc.stdin.write(line + "\n")
    proc.stdin.flush()

def wait_for_id(expected_id: int, timeout_sec: float = 8.0) -> dict:
    assert proc.stdout is not None
    deadline = time.time() + timeout_sec
    while time.time() < deadline:
        ready, _, _ = select.select([proc.stdout], [], [], 0.2)
        if not ready:
            continue
        line = proc.stdout.readline()
        if not line:
            break
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue
        if msg.get("id") == expected_id:
            return msg
    raise RuntimeError(f"timeout waiting for response id={expected_id}")

try:
    send(
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "cratebay-mcp-smoke", "version": "0.1.0"},
            },
        }
    )
    _ = wait_for_id(1)

    send({"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}})

    send({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}})
    tools_list = wait_for_id(2)
    tools = (tools_list.get("result") or {}).get("tools") or []
    names = {t.get("name") for t in tools if isinstance(t, dict)}
    required = {
        "cratebay_sandbox_templates",
        "cratebay_sandbox_list",
        "cratebay_sandbox_inspect",
        "cratebay_sandbox_exec",
    }
    missing = sorted([n for n in required if n not in names])
    if missing:
        raise RuntimeError(f"missing tools: {missing}")

    send(
        {
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {"name": "cratebay_sandbox_templates", "arguments": {}},
        }
    )
    call = wait_for_id(3)
    if not (call.get("result") or {}).get("content"):
        raise RuntimeError("tool call returned empty content")

finally:
    try:
        proc.terminate()
        proc.wait(timeout=2)
    except Exception:
        proc.kill()

print("cratebay-mcp stdio smoke: PASS")
PY

echo "cratebay-mcp smoke: PASS"

