#!/usr/bin/env bash
# Regenerate hardware/case/renders/*.png from typoena-case.scad.
# Usage:  ./render.sh            (all views)
#         ./render.sh assembled  (one view by output name)
set -euo pipefail
cd "$(dirname "$0")"
SCAD=typoena-case.scad
OUT=renders
COMMON=(--imgsize=1100,825 --colorscheme=Tomorrow --viewall --autocenter)

# name        show          camera (transx,y,z, rotx,y,z, dist auto via --viewall)
VIEWS=(
  "assembled  assembled     0,0,0,62,0,22,0"
  "front34    assembled     0,0,0,62,0,205,0"
  "body       body          0,0,0,62,0,22,0"
  "bracket    bracket        0,0,0,55,0,25,0"
  "baseplate  baseplate     0,0,0,52,0,205,0"
  "section    section       0,0,0,90,0,90,0"
  "plan       plan          0,0,0,58,0,205,0"
  "plan-up    plan_up       0,0,0,0,0,180,0"
  "plan-down  plan_down     0,0,0,0,0,0,0"
  "print      print_plate   0,0,0,55,0,25,0"
  "nameplate  assembled     88,6,26,58,0,0,62"
)

render() {
  local name=$1 show=$2 cam=$3
  echo "→ $name ($show)"
  # the nameplate is a tight close-up: use the camera distance as-is (no --viewall)
  local flags=("${COMMON[@]}")
  [ "$name" = "nameplate" ] && flags=(--imgsize=1100,825 --colorscheme=Tomorrow)
  openscad -o "$OUT/$name.png" "${flags[@]}" --camera="$cam" -D "show=\"$show\"" "$SCAD" 2>/dev/null
}

for v in "${VIEWS[@]}"; do
  read -r name show cam <<<"$v"
  [ $# -eq 0 ] || [ "$1" = "$name" ] && render "$name" "$show" "$cam"
done
echo "done"
