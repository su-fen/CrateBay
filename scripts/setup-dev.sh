#!/usr/bin/env bash
set -euo pipefail

# ── setup-dev.sh ──────────────────────────────────────────────────────────────
#
# One-time development environment setup for the CrateBay repository.
#
# What it does:
#   1. Configures repo-local git user identity
#   2. Sets core.hooksPath to .githooks so pre-commit and commit-msg hooks run
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

# ── Verify hooks are executable ───────────────────────────────────────────────
echo ""
echo "Ensuring hooks are executable..."
chmod +x .githooks/*
ls -la .githooks/

echo ""
echo "=== Setup Complete ==="
echo ""
echo "Git hooks installed:"
echo "  - pre-commit  : identity check, doc i18n, app i18n, cargo check"
echo "  - commit-msg  : conventional commits format validation"
echo ""
echo "You can now start developing. Happy hacking!"
