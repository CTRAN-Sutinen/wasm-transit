#!/usr/bin/env bash
# The route 99 end-to-end test, every step in order, against `just qa`
# (a fresh one: its store is in memory, so every run starts clean).
# Each step prints PASS/FAIL per check; the script carries on past a
# failed step and exits 1 when any failed. Pictures land in
# .qa/route-99/.
#
#   qa/route-99/run.sh            # every step
#   qa/route-99/run.sh 3 4        # only these steps
set -u
cd "$(dirname "$0")/../.."
out=.qa/route-99
mkdir -p "$out"
steps=(${@:-1 2 3 4})
shot() { timeout 900 node tools/ui-shot.mjs "$@" 2>&1 | grep -vE '^  (hash|status):|^shot |^    at '; return "${PIPESTATUS[0]}"; }
failed=()

for s in "${steps[@]}"; do
  case "$s" in
    1) echo "== 1. stops"
       shot stops --fresh --hash "#on=$(date +%F)" --wait 500 --steps qa/route-99/1-stops.mjs --out "$out/1-stops.png" || failed+=(1) ;;
    2) echo "== 2. service change"
       shot service-change --hash '#stage=calendar' --wait 1200 --steps qa/route-99/2-service-change.mjs --out "$out/2-sc.png" || failed+=(2) ;;
    3) echo "== 3. ride now"
       shot ride-now --wait 1500 --steps qa/route-99/3-ride-now.mjs --out "$out/3-ride.png" || failed+=(3) ;;
    4) echo "== 4. stop display"
       shot stop-display --size 1920x1080 --wait 1500 --steps qa/route-99/4-stop-display.mjs --out "$out/4-display.png" || failed+=(4) ;;
  esac
done

if [ ${#failed[@]} -eq 0 ]; then echo "route 99: PASS"; else echo "route 99: FAIL (steps ${failed[*]})"; exit 1; fi
