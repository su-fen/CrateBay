#!/usr/bin/env bash
set -u -o pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

timestamp="$(date +"%Y%m%d-%H%M%S")"
report_dir="$repo_root/dist/release-readiness"
report_file="$report_dir/report-$timestamp.log"
mkdir -p "$report_dir"
touch "$report_file"

status=0

run_check() {
  local name="$1"
  shift

  echo
  echo "== $name =="
  echo "== $name ==" >>"$report_file"
  echo "[$(date -u +"%Y-%m-%dT%H:%M:%SZ")] $name" >>"$report_file"

  if "$@" >>"$report_file" 2>&1; then
    echo "[PASS] $name"
    echo "[PASS] $name" >>"$report_file"
  else
    echo "[FAIL] $name"
    echo "[FAIL] $name" >>"$report_file"
    status=1
  fi
}

echo "CrateBay pre-v1 release-readiness gate"
echo "Report: $report_file"

run_check "Local CI gate (Rust + frontend)" ./scripts/ci-local.sh
run_check "Tauri GUI check" cargo check -p cratebay-gui
run_check "Tauri GUI tests" cargo test -p cratebay-gui
run_check "AI core scenario gate (>=95%)" \
  cargo test -p cratebay-gui ai_tests::assistant_core_scenarios_success_rate
run_check "AI UI smoke tests (Assistant + Settings)" \
  bash -lc "cd crates/cratebay-gui && npm run test:unit -- src/pages/__tests__/Assistant.test.tsx src/pages/__tests__/Settings.ai.test.tsx"
run_check "Release smoke checklist presence" \
  bash -lc "rg -n 'AI Core Scenario Gate|Cross-platform Installer Smoke|Upgrade Path Validation|coming soon' docs/RELEASE_SMOKE_CHECKLIST.md"
run_check "Release wording guard (must not claim released)" \
  bash -lc "if rg -n '(已发布|正式发布|已上线|正式上线|is now live|now live|officially released|已经发布)' README.md README.zh.md docs/TUTORIAL.md docs/TUTORIAL.zh.md website/index.html website/script.js; then exit 1; fi"
run_check "Coming-soon wording guard (required)" \
  bash -lc "rg -n '(coming soon|即将发布|即将提供)' README.md README.zh.md docs/TUTORIAL.md docs/TUTORIAL.zh.md website/index.html website/script.js"

echo
if [[ $status -eq 0 ]]; then
  echo "Release-readiness: PASS"
else
  echo "Release-readiness: FAIL (see $report_file)"
fi

exit $status
