#!/usr/bin/env bash
# The LRP service-change sessions that have walks (LRP/SESSIONS.md),
# in order, against a FRESH `just qa` (its store is in memory: a later
# session forks the picks an earlier one published, so run them in
# one server's life, from session 1). Each prints PASS/FAIL per check;
# the script carries on past a failed session and exits 1 when any
# failed. Pictures land in .qa/lrp/; the browser profile is .qa/lrp-profile.
#
#   LRP/run.sh                 # every session with a walk
#   LRP/run.sh 1               # only these
#   BASE=http://127.0.0.1:5177 LRP_SERVER=http://127.0.0.1:8095 LRP/run.sh
#                              # a second loop on other ports
set -u
cd "$(dirname "$0")/.."
out=.qa/lrp
mkdir -p "$out"
export LRP_SERVER="${LRP_SERVER:-http://127.0.0.1:8091}"
base="${BASE:-http://127.0.0.1:5174}"
sessions=(${@:-1 2})
shot() { timeout 900 node tools/ui-shot.mjs service-change --base "$base" --profile lrp-profile "$@" 2>&1 | grep -vE '^  (hash|status):|^shot |^    at '; return "${PIPESTATUS[0]}"; }
failed=()

for s in "${sessions[@]}"; do
  case "$s" in
    1) echo "== 1. evening span, 9 and 92"
       shot --fresh --hash '#stage=calendar' --wait 1500 --steps LRP/walks/s01-evening-span.mjs --out "$out/s01.png" || failed+=(1) ;;
    2) echo "== 2. Felida weekends"
       shot --hash '#stage=calendar' --wait 1500 --steps LRP/walks/s02-felida-weekends.mjs --out "$out/s02.png" || failed+=(2) ;;
    *) echo "no walk for session $s"; failed+=("$s") ;;
  esac
done

if [ ${#failed[@]} -eq 0 ]; then echo "LRP: PASS"; else echo "LRP: FAIL (sessions ${failed[*]})"; exit 1; fi
