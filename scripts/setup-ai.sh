#!/usr/bin/env bash
set -euo pipefail

# ── setup-ai.sh ───────────────────────────────────────────────────────────────
#
# Optional bootstrap for CrateBay's AI infrastructure roadmap:
#   - Local model runtime (Ollama-first)
#   - Agent sandboxes (OpenSandbox-compatible path)
#   - MCP server management
#
# This script is intentionally conservative:
#   - Default mode only checks prerequisites.
#   - Use --install to attempt installing what it can (platform dependent).
#
# Usage:
#   bash scripts/setup-ai.sh
#   bash scripts/setup-ai.sh --install
#   bash scripts/setup-ai.sh --init-opensandbox-config
#

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

do_install=false
init_opensandbox_config=false

usage() {
  cat <<'EOF'
CrateBay AI bootstrap

Usage:
  bash scripts/setup-ai.sh [--install] [--init-opensandbox-config]

Options:
  --install                 Attempt to install missing tools (best-effort).
  --init-opensandbox-config Copy tools/opensandbox/sandbox.example.toml into ~/.cratebay/opensandbox.toml
  -h, --help                Show help.
EOF
}

while [[ $# -gt 0 ]]; do
  case "${1}" in
    --install) do_install=true; shift ;;
    --init-opensandbox-config) init_opensandbox_config=true; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown argument: ${1}"; echo ""; usage; exit 1 ;;
  esac
done

os="$(uname -s | tr '[:upper:]' '[:lower:]')"

have_cmd() { command -v "$1" >/dev/null 2>&1; }

echo "=== CrateBay AI Bootstrap ==="
echo "Repo: ${REPO_ROOT}"
echo "OS  : ${os}"
echo ""

echo "== Checks =="

if have_cmd docker; then
  echo "  ✓ docker        : $(docker --version | head -n 1)"
  if docker compose version >/dev/null 2>&1; then
    echo "  ✓ docker compose: $(docker compose version | head -n 1)"
  else
    echo "  ⚠ docker compose: missing (install Docker Desktop / compose plugin)"
  fi
else
  echo "  ⚠ docker        : missing (install Docker Desktop / Colima / OrbStack)"
  echo "  ⚠ docker compose: missing (depends on Docker install)"
fi

if have_cmd ollama; then
  echo "  ✓ ollama        : $(ollama --version 2>/dev/null || echo 'installed')"
else
  echo "  ⚠ ollama        : missing (Ollama-first local model runtime)"
fi

if have_cmd python3; then
  echo "  ✓ python3       : $(python3 --version 2>&1)"
else
  echo "  ⚠ python3       : missing (required for opensandbox-server install)"
fi

if have_cmd uv; then
  echo "  ✓ uv            : $(uv --version 2>&1 | head -n 1)"
else
  echo "  ⚠ uv            : missing (optional; recommended by OpenSandbox docs)"
fi

if have_cmd opensandbox-server; then
  echo "  ✓ opensandbox-server: installed"
else
  echo "  ⚠ opensandbox-server: missing (optional; for Agent Sandboxes backend)"
fi

echo ""

if [[ "${do_install}" == "true" ]]; then
  echo "== Install (best-effort) =="

  case "${os}" in
    darwin*)
      if ! have_cmd brew; then
        echo "  ⚠ Homebrew not found; skipping automated installs on macOS."
        echo "    Install brew, then re-run with --install."
      else
        if ! have_cmd ollama; then
          echo "  Installing ollama via brew..."
          brew install ollama
        fi

        if ! have_cmd uv; then
          echo "  Installing uv via brew..."
          brew install uv
        fi

        if ! have_cmd python3; then
          echo "  Installing python via brew..."
          brew install python@3.11
        fi

        if ! have_cmd opensandbox-server; then
          if have_cmd uv; then
            echo "  Installing opensandbox-server via uv..."
            uv pip install opensandbox-server
          elif have_cmd python3; then
            echo "  Installing opensandbox-server via pip (user install)..."
            python3 -m pip install --user opensandbox-server
          else
            echo "  ⚠ python3 not found; cannot install opensandbox-server."
          fi
        fi
      fi
      ;;
    linux*)
      if ! have_cmd python3; then
        if have_cmd apt-get; then
          echo "  Installing python3 via apt-get (requires sudo)..."
          sudo apt-get update
          sudo apt-get install -y python3 python3-pip
        else
          echo "  ⚠ python3 missing and apt-get not found; install python3 manually."
        fi
      fi

      if ! have_cmd opensandbox-server && have_cmd python3; then
        echo "  Installing opensandbox-server via pip (user install)..."
        python3 -m pip install --user opensandbox-server
      fi

      if ! have_cmd ollama; then
        echo "  ⚠ ollama install is not automated here."
        echo "    Please follow Ollama's official install instructions for Linux."
      fi
      ;;
    msys*|cygwin*|mingw*)
      echo "  ⚠ Automated install is not implemented for Windows shell environments."
      echo "    Install Docker Desktop and Ollama manually, then re-run without --install."
      ;;
    *)
      echo "  ⚠ Unknown OS '${os}'; skipping automated installs."
      ;;
  esac

  echo ""
fi

if [[ "${init_opensandbox_config}" == "true" ]]; then
  echo "== OpenSandbox config =="
  src="${REPO_ROOT}/tools/opensandbox/sandbox.example.toml"
  dst_dir="${HOME}/.cratebay"
  dst="${dst_dir}/opensandbox.toml"

  mkdir -p "${dst_dir}"
  if [[ -f "${dst}" ]]; then
    echo "  ⚠ ${dst} already exists; not overwriting."
  else
    cp "${src}" "${dst}"
    echo "  ✓ wrote ${dst}"
  fi
  echo ""
fi

echo "== Next steps =="
echo "  - Read the AI infra proposal: docs/VISION.md (or docs/VISION.zh.md)"
echo "  - OpenSandbox local scaffold : tools/opensandbox/README.md"
echo ""
echo "=== Done ==="

