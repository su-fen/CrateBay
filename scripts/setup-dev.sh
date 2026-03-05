#!/usr/bin/env bash
set -euo pipefail

# ── setup-dev.sh ──────────────────────────────────────────────────────────────
#
# One-time development environment setup for the CrateBay repository.
#
# What it does:
#   1. Configures repo-local git user identity
#   2. Sets core.hooksPath to .githooks so pre-commit/pre-push/commit-msg run
#   3. Checks local Node.js runtime (required: 22+, recommend 22/24 LTS)
#
# Usage:
#   bash scripts/setup-dev.sh
#
# You only need to run this once after cloning.

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

echo "=== CrateBay Dev Environment Setup ==="
echo ""

# ── Git identity ──────────────────────────────────────────────────────────────
echo "Configuring repo-local git identity..."
git config --local user.name "coder-hhx"
git config --local user.email "houhaixu_email@163.com"
echo "  user.name  = $(git config --local user.name)"
echo "  user.email = $(git config --local user.email)"

# ── Git hooks ─────────────────────────────────────────────────────────────────
echo ""
echo "Configuring git hooks path..."
git config --local core.hooksPath .githooks
echo "  core.hooksPath = $(git config --local core.hooksPath)"

# ── Node runtime check ────────────────────────────────────────────────────────
echo ""
echo "Checking Node.js runtime (required: 22+, recommend 22/24 LTS)..."
if command -v node >/dev/null 2>&1; then
  node_version="$(node -v)"
  node_major="$(node -p "process.versions.node.split('.')[0]")"
  echo "  node = ${node_version}"
  if (( node_major < 22 )); then
    echo "  WARNING: Node.js 22+ is required by local CI."
    echo "  Run: nvm install 24 && nvm use 24"
  fi
else
  echo "  WARNING: node not found. Install Node.js 22+."
fi

# ── Verify hooks are executable ───────────────────────────────────────────────
echo ""
echo "Ensuring hooks are executable..."
chmod +x .githooks/*
ls -la .githooks/

echo ""
echo "=== Setup Complete ==="
echo ""
echo "Git hooks installed:"
echo "  - pre-commit  : upstream sync check, doc i18n, app i18n, cargo check"
echo "  - pre-push    : local CI gate (fmt, clippy, tests, frontend checks)"
echo "  - commit-msg  : conventional commits format validation"
echo ""
echo "You can now start developing. Happy hacking!"
