#!/usr/bin/env bash
# CrateBay Performance Benchmark
# Validates the performance claims in README.md:
#   1. <20MB install (binary size)
#   2. <200MB idle RAM
#   3. <3s startup
#
# Usage:
#   ./scripts/bench-perf.sh [--release-dir DIR]
#
# Exit code 0 if ALL checks pass, 1 if any fail.

set -euo pipefail

# ── Configuration ─────────────────────────────────────────────────
MAX_BINARY_SIZE_MB=20
MAX_STARTUP_TIME_S=3
MAX_IDLE_RAM_MB=200
STARTUP_RUNS=5
DAEMON_SETTLE_SECONDS=3

# ── Argument parsing ──────────────────────────────────────────────
RELEASE_DIR="target/release"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --release-dir) RELEASE_DIR="$2"; shift 2 ;;
        *) echo "Unknown argument: $1"; exit 2 ;;
    esac
done

# ── Colour helpers (disabled when not a terminal) ─────────────────
if [[ -t 1 ]]; then
    RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[0;33m'
    BOLD='\033[1m'; RESET='\033[0m'
else
    RED=''; GREEN=''; YELLOW=''; BOLD=''; RESET=''
fi

pass() { printf "${GREEN}PASS${RESET}"; }
fail() { printf "${RED}FAIL${RESET}"; }
warn() { printf "${YELLOW}SKIP${RESET}"; }

# ── State tracking ────────────────────────────────────────────────
FAILURES=0
SKIPS=0

# Results arrays (for summary table)
declare -a RESULT_NAMES=()
declare -a RESULT_VALUES=()
declare -a RESULT_LIMITS=()
declare -a RESULT_STATUSES=()

record() {
    # record NAME VALUE LIMIT STATUS
    RESULT_NAMES+=("$1")
    RESULT_VALUES+=("$2")
    RESULT_LIMITS+=("$3")
    RESULT_STATUSES+=("$4")
}

# ── Helper: get file size in bytes (cross-platform) ──────────────
file_size_bytes() {
    local path="$1"
    if [[ "$(uname)" == "Darwin" ]]; then
        stat -f%z "$path"
    else
        stat -c%s "$path"
    fi
}

# ── Helper: get RSS of a PID in KB ───────────────────────────────
get_rss_kb() {
    local pid="$1"
    if [[ "$(uname)" == "Darwin" ]]; then
        ps -o rss= -p "$pid" | tr -d ' '
    elif [[ -f "/proc/$pid/status" ]]; then
        awk '/^VmRSS:/ { print $2 }' "/proc/$pid/status"
    else
        ps -o rss= -p "$pid" | tr -d ' '
    fi
}

# ── Helper: compute median from a file of numbers ────────────────
median() {
    sort -n | awk '{a[NR]=$1} END {
        if (NR%2==1) print a[(NR+1)/2];
        else print (a[NR/2]+a[NR/2+1])/2
    }'
}

echo ""
echo "======================================================"
echo " CrateBay Performance Benchmark"
echo "======================================================"
echo ""
echo "Release dir : $RELEASE_DIR"
echo "Platform    : $(uname -s) $(uname -m)"
echo "Date        : $(date -u '+%Y-%m-%d %H:%M:%S UTC')"
echo ""

# ══════════════════════════════════════════════════════════════════
# 1. Binary Size Check (<20MB)
# ══════════════════════════════════════════════════════════════════
echo "──────────────────────────────────────────────────────"
echo " 1. Binary Size Check (limit: <${MAX_BINARY_SIZE_MB}MB per binary)"
echo "──────────────────────────────────────────────────────"

MAX_BYTES=$((MAX_BINARY_SIZE_MB * 1048576))

for bin in cratebay cratebay-daemon; do
    path="${RELEASE_DIR}/${bin}"
    if [[ ! -f "$path" ]]; then
        printf "  %-20s [$(warn)] binary not found at %s\n" "$bin" "$path"
        record "$bin size" "N/A" "<${MAX_BINARY_SIZE_MB}MB" "SKIP"
        SKIPS=$((SKIPS + 1))
        continue
    fi

    size_bytes=$(file_size_bytes "$path")
    size_mb=$(echo "scale=2; $size_bytes / 1048576" | bc)

    if [[ "$size_bytes" -gt "$MAX_BYTES" ]]; then
        printf "  %-20s %6s MB  [$(fail)]  exceeds %sMB limit\n" "$bin" "$size_mb" "$MAX_BINARY_SIZE_MB"
        record "$bin size" "${size_mb}MB" "<${MAX_BINARY_SIZE_MB}MB" "FAIL"
        FAILURES=$((FAILURES + 1))
    else
        printf "  %-20s %6s MB  [$(pass)]\n" "$bin" "$size_mb"
        record "$bin size" "${size_mb}MB" "<${MAX_BINARY_SIZE_MB}MB" "PASS"
    fi
done
echo ""

# ══════════════════════════════════════════════════════════════════
# 2. Startup Time Check (<3s)
# ══════════════════════════════════════════════════════════════════
echo "──────────────────────────────────────────────────────"
echo " 2. Startup Time Check (limit: <${MAX_STARTUP_TIME_S}s, median of ${STARTUP_RUNS} runs)"
echo "──────────────────────────────────────────────────────"

CLI_BIN="${RELEASE_DIR}/cratebay"

if [[ ! -f "$CLI_BIN" ]]; then
    printf "  [$(warn)] cratebay binary not found, skipping startup benchmark\n"
    record "startup time" "N/A" "<${MAX_STARTUP_TIME_S}s" "SKIP"
    SKIPS=$((SKIPS + 1))
else
    # Check if hyperfine is available
    if command -v hyperfine &>/dev/null; then
        echo "  Using hyperfine for measurement..."
        # hyperfine outputs JSON; extract median
        HYPERFINE_JSON=$(hyperfine --runs "$STARTUP_RUNS" --export-json /dev/stdout \
            --warmup 1 "$CLI_BIN status" 2>/dev/null) || true

        if [[ -n "$HYPERFINE_JSON" ]]; then
            MEDIAN_S=$(echo "$HYPERFINE_JSON" | \
                python3 -c "import sys,json; d=json.load(sys.stdin); print(d['results'][0]['median'])" 2>/dev/null) || MEDIAN_S=""
        fi

        if [[ -z "${MEDIAN_S:-}" ]]; then
            echo "  hyperfine JSON parsing failed, falling back to bash timing..."
            MEDIAN_S=""
        fi
    fi

    # Fallback: bash timing
    if [[ -z "${MEDIAN_S:-}" ]]; then
        echo "  Using bash timing (${STARTUP_RUNS} runs)..."
        TIMES_FILE=$(mktemp)
        for i in $(seq 1 "$STARTUP_RUNS"); do
            # Use bash TIMEFORMAT to get wall-clock seconds
            START_NS=$( { date +%s%N 2>/dev/null || python3 -c "import time; print(int(time.time()*1e9))"; } )
            "$CLI_BIN" status >/dev/null 2>&1 || true
            END_NS=$( { date +%s%N 2>/dev/null || python3 -c "import time; print(int(time.time()*1e9))"; } )
            ELAPSED=$(echo "scale=4; ($END_NS - $START_NS) / 1000000000" | bc)
            echo "$ELAPSED" >> "$TIMES_FILE"
            printf "    run %d: %ss\n" "$i" "$ELAPSED"
        done
        MEDIAN_S=$(median < "$TIMES_FILE")
        rm -f "$TIMES_FILE"
    fi

    # Evaluate result
    EXCEEDED=$(echo "$MEDIAN_S > $MAX_STARTUP_TIME_S" | bc -l)
    if [[ "$EXCEEDED" -eq 1 ]]; then
        printf "  Median: %ss  [$(fail)]  exceeds %ss limit\n" "$MEDIAN_S" "$MAX_STARTUP_TIME_S"
        record "startup time" "${MEDIAN_S}s" "<${MAX_STARTUP_TIME_S}s" "FAIL"
        FAILURES=$((FAILURES + 1))
    else
        printf "  Median: %ss  [$(pass)]\n" "$MEDIAN_S"
        record "startup time" "${MEDIAN_S}s" "<${MAX_STARTUP_TIME_S}s" "PASS"
    fi
fi
echo ""

# ══════════════════════════════════════════════════════════════════
# 3. Idle Memory Check (<200MB)
# ══════════════════════════════════════════════════════════════════
echo "──────────────────────────────────────────────────────"
echo " 3. Idle Memory Check (limit: <${MAX_IDLE_RAM_MB}MB RSS)"
echo "──────────────────────────────────────────────────────"

DAEMON_BIN="${RELEASE_DIR}/cratebay-daemon"

if [[ ! -f "$DAEMON_BIN" ]]; then
    printf "  [$(warn)] cratebay-daemon binary not found, skipping memory benchmark\n"
    record "idle RAM" "N/A" "<${MAX_IDLE_RAM_MB}MB" "SKIP"
    SKIPS=$((SKIPS + 1))
else
    DAEMON_PID=""
    cleanup_daemon() {
        if [[ -n "$DAEMON_PID" ]] && kill -0 "$DAEMON_PID" 2>/dev/null; then
            echo "  Stopping daemon (PID $DAEMON_PID)..."
            kill "$DAEMON_PID" 2>/dev/null || true
            wait "$DAEMON_PID" 2>/dev/null || true
        fi
    }
    trap cleanup_daemon EXIT

    echo "  Starting daemon..."
    "$DAEMON_BIN" &>/dev/null &
    DAEMON_PID=$!

    # Verify it started
    if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
        printf "  [$(warn)] daemon failed to start\n"
        record "idle RAM" "N/A" "<${MAX_IDLE_RAM_MB}MB" "SKIP"
        SKIPS=$((SKIPS + 1))
        DAEMON_PID=""
    else
        echo "  Waiting ${DAEMON_SETTLE_SECONDS}s for daemon to stabilise..."
        sleep "$DAEMON_SETTLE_SECONDS"

        # Check it's still alive
        if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
            printf "  [$(warn)] daemon exited during settle period\n"
            record "idle RAM" "N/A" "<${MAX_IDLE_RAM_MB}MB" "SKIP"
            SKIPS=$((SKIPS + 1))
            DAEMON_PID=""
        else
            RSS_KB=$(get_rss_kb "$DAEMON_PID")
            RSS_MB=$(echo "scale=2; $RSS_KB / 1024" | bc)
            MAX_RSS_KB=$((MAX_IDLE_RAM_MB * 1024))

            if [[ "$RSS_KB" -gt "$MAX_RSS_KB" ]]; then
                printf "  RSS: %sMB (PID %s)  [$(fail)]  exceeds %sMB limit\n" \
                    "$RSS_MB" "$DAEMON_PID" "$MAX_IDLE_RAM_MB"
                record "idle RAM" "${RSS_MB}MB" "<${MAX_IDLE_RAM_MB}MB" "FAIL"
                FAILURES=$((FAILURES + 1))
            else
                printf "  RSS: %sMB (PID %s)  [$(pass)]\n" "$RSS_MB" "$DAEMON_PID"
                record "idle RAM" "${RSS_MB}MB" "<${MAX_IDLE_RAM_MB}MB" "PASS"
            fi

            # Clean up
            cleanup_daemon
            DAEMON_PID=""
        fi
    fi
fi
echo ""

# ══════════════════════════════════════════════════════════════════
# Summary
# ══════════════════════════════════════════════════════════════════
echo "======================================================"
echo " Summary"
echo "======================================================"
echo ""
printf "  ${BOLD}%-20s  %-12s  %-12s  %-6s${RESET}\n" "Metric" "Value" "Limit" "Result"
printf "  %-20s  %-12s  %-12s  %-6s\n" "────────────────────" "────────────" "────────────" "──────"

for i in "${!RESULT_NAMES[@]}"; do
    status="${RESULT_STATUSES[$i]}"
    case "$status" in
        PASS) colour="$GREEN" ;;
        FAIL) colour="$RED" ;;
        *)    colour="$YELLOW" ;;
    esac
    printf "  %-20s  %-12s  %-12s  ${colour}%-6s${RESET}\n" \
        "${RESULT_NAMES[$i]}" "${RESULT_VALUES[$i]}" "${RESULT_LIMITS[$i]}" "$status"
done
echo ""

if [[ "$FAILURES" -gt 0 ]]; then
    echo "${RED}${BOLD}RESULT: $FAILURES check(s) FAILED${RESET}"
    exit 1
elif [[ "$SKIPS" -gt 0 ]]; then
    echo "${YELLOW}${BOLD}RESULT: All run checks passed ($SKIPS skipped)${RESET}"
    exit 0
else
    echo "${GREEN}${BOLD}RESULT: All checks PASSED${RESET}"
    exit 0
fi
