#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

echo "== Local CI: Rust fmt =="
cargo fmt --check

os_name="$(uname -s)"
if [[ "$os_name" == "Darwin" ]]; then
  clippy_args=(--workspace --exclude cratebay-gui -- -D warnings)
  test_args=(--workspace --exclude cratebay-gui --exclude cratebay-vz -- --test-threads=1)
else
  clippy_args=(--workspace --exclude cratebay-gui --exclude cratebay-vz -- -D warnings)
  test_args=(--workspace --exclude cratebay-gui --exclude cratebay-vz -- --test-threads=1)
fi

echo "== Local CI: Rust clippy =="
cargo clippy "${clippy_args[@]}"

echo "== Local CI: Rust tests =="
cargo test "${test_args[@]}"

if [[ "$os_name" == "Darwin" ]]; then
  if [[ "${CRATEBAY_RUN_VZ_TESTS:-0}" == "1" ]]; then
    echo "== Local CI: cratebay-vz tests =="
    cargo test -p cratebay-vz -- --test-threads=1
  else
    echo "== Local CI: cratebay-vz tests skipped =="
    echo "Set CRATEBAY_RUN_VZ_TESTS=1 to run cratebay-vz tests locally."
  fi
fi

if ! command -v node >/dev/null 2>&1; then
  echo "ERROR: Node.js 22+ is required for frontend checks (node not found)."
  exit 1
fi

node_major="$(node -p "process.versions.node.split('.')[0]")"
node_version="$(node -v)"
if (( node_major < 22 )); then
  echo "ERROR: Node.js 22+ is required. Current: ${node_version}"
  echo "Use: nvm install 24 && nvm use 24"
  exit 1
fi

echo "== Local CI: Frontend checks =="
echo "Node runtime: ${node_version}"
pushd crates/cratebay-gui >/dev/null
npm ci
npm run lint
npm run build
npm run check:i18n
npm run test:unit
popd >/dev/null

echo "== Local CI complete =="
