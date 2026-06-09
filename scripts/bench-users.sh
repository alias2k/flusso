#!/usr/bin/env bash
#
# bench-users.sh — production-like load benchmark for flusso.
#
# Approximates N concurrent users by running several parallel writer sessions,
# each committing SMALL (user-sized) transactions in a steady stream — the
# realistic shape (many small, concurrent commits), not one session firing huge
# bursts. The user count drives the target rate; the rest is derived:
#
#   target txn/s  =  USERS / WRITE_INTERVAL      (each user writes ~every Ns)
#   sleep_ms      =  WRITERS * 1000 / target     (per-writer pacing to hit it)
#
# The ACHIEVABLE rate is host-bound (Postgres `ORDER BY random()` scans cost more
# as tables grow, and your machine also runs the containers). Watch the live
# monitor: a sustainable rate keeps lag ~flat; exceeding flusso's capacity pins
# in_flight and grows the backlog (the ETA appears, telling you the drain time).
#
# Usage:
#   ./scripts/bench-users.sh [USERS]
#   ./scripts/bench-users.sh 20000
#   WRITERS=16 WRITE_INTERVAL=10 DURATION=900 ./scripts/bench-users.sh 50000
#
# Env knobs: WRITERS, OPS_PER_TICK, WRITE_INTERVAL, DURATION, DB_URL,
#            FLUSSO_HTTP, PROM, INTERVAL, NO_COLOR
#
# Ctrl-C stops every writer cleanly.

set -euo pipefail

# ── config (override via env) ────────────────────────────────────────────────
USERS_DEFAULT=20000
DB_URL="${DATABASE_URL:-postgres://postgres:postgres@127.0.0.1:5432/flusso}"
WRITERS="${WRITERS:-8}"                 # concurrent writer sessions (concurrency)
OPS_PER_TICK="${OPS_PER_TICK:-3}"       # ops per committed transaction (keep small)
WRITE_INTERVAL="${WRITE_INTERVAL:-20}"  # avg seconds between writes, per user
DURATION="${DURATION:-600}"             # seconds to run
FLUSSO_HTTP="${FLUSSO_HTTP:-127.0.0.1:9464}"
PROM="${PROM:-127.0.0.1:9090}"          # prometheus base (for the ETA); empty to skip
INTERVAL="${INTERVAL:-5}"               # monitor refresh, seconds

# ── arg parsing: optional positional USERS ───────────────────────────────────
case "${1:-}" in
  -h|--help) sed -n '2,33p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
  "")         USERS="${USERS:-$USERS_DEFAULT}" ;;
  *[!0-9]*)   echo "error: USERS must be a positive integer (got '$1')" >&2; exit 2 ;;
  *)          USERS="$1" ;;
esac
[ "${USERS:-0}" -gt 0 ] || { echo "error: USERS must be > 0" >&2; exit 2; }

# ── environment / tooling ────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"
LOAD_SQL="${LOAD_SQL:-dev/load.sql}"

command -v psql    >/dev/null || { echo "error: psql not found on PATH" >&2; exit 1; }
command -v python3 >/dev/null || { echo "error: python3 not found on PATH" >&2; exit 1; }
[ -f "$LOAD_SQL" ] || { echo "error: $LOAD_SQL not found (run from the flusso repo)" >&2; exit 1; }

if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then COLOR=1; else COLOR=0; fi
if [ "$COLOR" = 1 ]; then
  RED=$'\033[31m'; GRN=$'\033[32m'; DIM=$'\033[2m'; RST=$'\033[0m'
else
  RED=""; GRN=""; DIM=""; RST=""
fi
DB_DISPLAY="${DB_URL##*@}"   # strip user:pass@ for display

# ── derive the load parameters from the user count ───────────────────────────
read -r TARGET_TX SLEEP_MS EST_CHANGES <<EOF
$(python3 -c "
u=$USERS; iv=$WRITE_INTERVAL; w=$WRITERS; ops=$OPS_PER_TICK
tx = u / iv if iv > 0 else u
sleep_ms = max(0, round(w * 1000 / tx)) if tx > 0 else 0
print(round(tx), sleep_ms, round(tx * ops))
")
EOF

logdir="$(mktemp -d)"
pids=()
start_epoch=$SECONDS

cleanup() {
  trap - INT TERM EXIT
  if [ "${#pids[@]}" -gt 0 ]; then
    for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || true; done
    wait 2>/dev/null || true
  fi
  printf '\n%s  writer logs: %s%s\n' "$DIM" "$logdir" "$RST"
}
trap cleanup INT TERM EXIT

# ── banner ───────────────────────────────────────────────────────────────────
python3 - "$COLOR" "$USERS" "$WRITE_INTERVAL" "$TARGET_TX" "$EST_CHANGES" \
                   "$WRITERS" "$OPS_PER_TICK" "$SLEEP_MS" "$DURATION" "$DB_DISPLAY" <<'PY'
import sys
color, users, iv, tx, changes, writers, ops, sleep_ms, duration, db = sys.argv[1:11]
C = color == "1"
def cyan(s): return f"\033[36m{s}\033[0m" if C else s
W = 64
def rule(l, r): return cyan(l + "─" * (W - 2) + r)
def row(label, value):
    s = f"  {label:<17}{value}"
    return "│" + s + " " * max(0, W - 2 - len(s)) + "│"
title = "flusso · production-load benchmark"
lpad = (W - 2 - len(title)) // 2
print()
print(rule("╭", "╮"))
print("│" + " " * lpad + title + " " * (W - 2 - lpad - len(title)) + "│")
print(rule("├", "┤"))
print(row("simulated users", f"{int(users):,}"))
print(row("write cadence",   f"~every {iv}s/user  →  ~{int(tx):,} txn/s"))
print(row("est. change rate", f"~{int(changes):,} changes/s"))
print(row("writers",         f"{writers}    ops/txn {ops}    sleep {sleep_ms}ms"))
print(row("duration",        f"{duration}s"))
print(row("target",          db))
print(rule("╰", "╯"))
PY

# ── preflight: is the DB reachable? ──────────────────────────────────────────
if ! psql "$DB_URL" -tAc 'SELECT 1' >/dev/null 2>&1; then
  printf '\n%s  ✗ cannot reach Postgres at %s%s\n' "$RED" "$DB_DISPLAY" "$RST"
  printf '%s    is the stack up?  docker compose up -d%s\n' "$DIM" "$RST"
  exit 1
fi

# ── (re)define the load procedure, then spawn the writers ────────────────────
psql "$DB_URL" -q -f "$LOAD_SQL"
for i in $(seq 1 "$WRITERS"); do
  psql "$DB_URL" -q -c \
    "CALL simulate_production(duration_secs => $DURATION, ops_per_tick => $OPS_PER_TICK, sleep_ms => $SLEEP_MS)" \
    >"$logdir/writer-$i.log" 2>&1 &
  pids+=("$!")
done
printf '\n%s  ▶ %d writers running for %ds%s  %s(Ctrl-C to stop)%s\n\n' "$GRN" "$WRITERS" "$DURATION" "$RST" "$DIM" "$RST"

# ── live monitor ─────────────────────────────────────────────────────────────
prom_q() {   # one prometheus instant query → scalar value ("" on any failure)
  [ -n "$PROM" ] || { echo ""; return 0; }
  local q
  q="$(python3 -c 'import urllib.parse,sys; print(urllib.parse.quote(sys.argv[1]))' "$1")"
  curl -fsS "http://$PROM/api/v1/query?query=$q" 2>/dev/null \
    | python3 -c "import sys,json
try:
    r=json.load(sys.stdin)['data']['result']; print(r[0]['value'][1] if r else '')
except Exception:
    print('')" 2>/dev/null || echo ""
}

# column header
python3 - "$COLOR" "HEADER" "" "" "" "" "" <<'PY'
import sys
C = sys.argv[1] == "1"
hdr = f"  {'time':>5}  {'cap/s':>8}  {'com/s':>8}  {'in flight':>10}  {'lag':>11}  {'eta':>10}  status"
print(f"\033[2m{hdr}\033[0m" if C else hdr)
PY

while :; do
  elapsed=$((SECONDS - start_epoch))
  status_json="$(curl -fsS "http://$FLUSSO_HTTP/status" 2>/dev/null || echo '{}')"
  cap="$(prom_q 'rate(flusso_changes_captured_total[1m])')"
  com="$(prom_q 'rate(flusso_changes_committed_total[1m])')"
  eta="$(prom_q 'flusso:backlog_drain_eta_seconds')"
  rate="$(prom_q 'flusso:slot_lag_bytes_rate5m')"

  python3 - "$COLOR" "$elapsed" "$status_json" "$cap" "$com" "$eta" "$rate" <<'PY'
import sys, json
color, elapsed, status_json, cap, com, eta, rate = sys.argv[1:8]
C = color == "1"
GREEN, YELLOW, RED, DIM = "32", "33", "31", "2"

def col(plain, w, code=None, align=">"):
    p = plain.rjust(w) if align == ">" else plain.ljust(w)
    return f"\033[{code}m{p}\033[0m" if (C and code) else p

def num(v):
    try: return f"{float(v):,.0f}"
    except Exception: return "·"

try: d = json.loads(status_json)
except Exception: d = {}
infl = d.get("changes_in_flight")
lag_b = d.get("slot_lag_bytes")
lag = f"{lag_b / 1048576:,.1f} MB" if isinstance(lag_b, (int, float)) else "·"
try: r = float(rate)
except Exception: r = None
try: eta_min = float(eta) / 60
except Exception: eta_min = None

# lag-trend arrow from the drain rate
if   r is None:        arrow = col("·", 1, DIM)
elif r >  5000:        arrow = col("▲", 1, RED)     # growing
elif r < -5000:        arrow = col("▼", 1, GREEN)   # draining
else:                  arrow = col("▬", 1, DIM)     # flat

eta_plain = "caught up" if eta_min is None else f"{eta_min:.1f} min"
eta_code  = GREEN if eta_min is None else YELLOW

try: infl_i = int(infl)
except Exception: infl_i = None
if   infl_i is None:           status = col("connecting…", 0, DIM, "<")
elif r is not None and r>5000: status = col("⤓ falling behind", 0, RED, "<")
elif eta_min is not None:      status = col("⤴ draining", 0, YELLOW, "<")
elif infl_i >= 512:            status = col("• busy", 0, YELLOW, "<")
else:                          status = col("✓ keeping up", 0, GREEN, "<")
infl_txt = "·" if infl_i is None else f"{infl_i:,}"

print("  " + "  ".join([
    col(f"{elapsed}s", 5, DIM),
    col(num(cap), 8),
    col(num(com), 8),
    col(infl_txt, 10),
    col(lag, 9) + " " + arrow,
    col(eta_plain, 10, eta_code),
]) + "  " + status)
PY

  alive=0
  for pid in "${pids[@]}"; do
    if kill -0 "$pid" 2>/dev/null; then alive=1; break; fi
  done
  if [ "$alive" -eq 0 ]; then
    printf '\n%s  ✓ all writers finished after %ds%s\n' "$GRN" "$((SECONDS - start_epoch))" "$RST"
    break
  fi
  sleep "$INTERVAL"
done
