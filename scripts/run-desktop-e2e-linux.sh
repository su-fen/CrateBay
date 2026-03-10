#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "desktop Tauri WebDriver smoke currently runs on Linux only"
  exit 1
fi

export NVM_DIR="${NVM_DIR:-$HOME/.nvm}"
if [[ -z "${CI:-}" && -s "$NVM_DIR/nvm.sh" ]]; then
  set +u
  . "$NVM_DIR/nvm.sh"
  set -u
  nvm use 24 >/dev/null 2>&1 || nvm use 22 >/dev/null 2>&1 || nvm use --lts >/dev/null 2>&1 || true
fi

if ! command -v node >/dev/null 2>&1; then
  echo "Node.js 22+ is required"
  exit 1
fi

if ! command -v tauri-driver >/dev/null 2>&1; then
  echo "tauri-driver is required in PATH"
  exit 1
fi

if ! command -v WebKitWebDriver >/dev/null 2>&1; then
  echo "WebKitWebDriver is required in PATH"
  exit 1
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "docker is required in PATH"
  exit 1
fi

if ! docker info >/dev/null 2>&1; then
  echo "Docker daemon is not available"
  exit 1
fi

artifact_dir="$repo_root/dist/desktop-smoke"
driver_log="$artifact_dir/tauri-driver.log"
test_log="$artifact_dir/desktop-smoke-test.log"
rm -rf "$artifact_dir"
mkdir -p "$artifact_dir/config" "$artifact_dir/data" "$artifact_dir/logs"

export CRATEBAY_CONFIG_DIR="$artifact_dir/config"
export CRATEBAY_DATA_DIR="$artifact_dir/data"
export CRATEBAY_LOG_DIR="$artifact_dir/logs"
export RUST_LOG="${RUST_LOG:-info}"
export CRATEBAY_DESKTOP_E2E_WORKDIR="$repo_root"

printf 'artifact_dir=%s\n' "$artifact_dir" | tee "$artifact_dir/run-context.txt"
printf 'config_dir=%s\n' "$CRATEBAY_CONFIG_DIR" >> "$artifact_dir/run-context.txt"
printf 'data_dir=%s\n' "$CRATEBAY_DATA_DIR" >> "$artifact_dir/run-context.txt"
printf 'log_dir=%s\n' "$CRATEBAY_LOG_DIR" >> "$artifact_dir/run-context.txt"
printf 'workdir=%s\n' "$CRATEBAY_DESKTOP_E2E_WORKDIR" >> "$artifact_dir/run-context.txt"

docker version > "$artifact_dir/docker-version.txt"
docker info > "$artifact_dir/docker-info.txt"

pushd crates/cratebay-gui >/dev/null
npm ci
npm run build
popd >/dev/null

cargo build -p cratebay-gui --features custom-protocol

app_path="$repo_root/target/debug/cratebay-gui"
if [[ ! -x "$app_path" ]]; then
  echo "Built app not found at $app_path"
  exit 1
fi

tauri-driver --native-driver "$(command -v WebKitWebDriver)" > "$driver_log" 2>&1 &
driver_pid=$!
trap 'kill "$driver_pid" >/dev/null 2>&1 || true' EXIT
sleep 3

CRATEBAY_DESKTOP_E2E_APP="$app_path" \
TAURI_DRIVER_URL="http://127.0.0.1:4444" \
cargo test -p cratebay-gui --test desktop_smoke -- --ignored --nocapture --test-threads=1 2>&1 | tee "$test_log"

docker ps -a > "$artifact_dir/docker-ps.txt"
find "$artifact_dir" -maxdepth 2 -type f | sort > "$artifact_dir/artifacts.txt"
