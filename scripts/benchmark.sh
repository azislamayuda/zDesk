#!/usr/bin/env bash
set -euo pipefail

seconds="${1:-60}"
if [[ ! "$seconds" =~ ^[1-9][0-9]*$ ]]; then
  echo "Usage: $0 [positive-duration-in-seconds]" >&2
  exit 2
fi

mapfile -t pids < <( { pgrep -x zdesk || true; pgrep -x conky || true; } | sort -nu )
if (( ${#pids[@]} == 0 )); then
  echo "No zdesk or conky process is running." >&2
  exit 1
fi

ticks() {
  local line remaining
  local -a fields
  line="$(<"/proc/$1/stat")"
  remaining="${line#*) }"
  read -r -a fields <<< "$remaining"
  printf '%s\n' "$(( fields[11] + fields[12] ))"
}

rss_kib() {
  awk '$1 == "VmRSS:" { print $2; exit }' "/proc/$1/status"
}

pss_kib() {
  awk '$1 == "Pss:" { print $2; exit }' "/proc/$1/smaps_rollup"
}

declare -A first_tick
declare -A name
desk_count=0
conky_count=0
for pid in "${pids[@]}"; do
  if [[ -r "/proc/$pid/stat" ]]; then
    first_tick[$pid]="$(ticks "$pid")"
    name[$pid]="$(<"/proc/$pid/comm")"
    if [[ "${name[$pid]}" == zdesk ]]; then
      desk_count=$(( desk_count + 1 ))
    elif [[ "${name[$pid]}" == conky ]]; then
      conky_count=$(( conky_count + 1 ))
    fi
  fi
done
if (( desk_count == 0 )); then
  echo "Start zDesk before running the benchmark." >&2
  exit 1
fi

clock_ticks="$(getconf CLK_TCK)"
start_ms="$(date +%s%3N)"
sleep "$seconds"
elapsed_ms=$(( $(date +%s%3N) - start_ms ))

printf 'Interval: %.3f seconds; CPU%% = average of one core\n' "$(awk -v ms="$elapsed_ms" 'BEGIN { print ms / 1000 }')"
printf '%-7s %-16s %12s %12s %10s\n' PID Process RSS_KiB PSS_KiB CPU_pct

desk_rss=0
desk_pss=0
conky_rss=0
conky_pss=0
desk_ticks=0
conky_ticks=0
exited=0
for pid in "${pids[@]}"; do
  if [[ ! -v first_tick[$pid] ]] || [[ ! -r "/proc/$pid/stat" ]]; then
    printf '%-7s %-16s %s\n' "$pid" "${name[$pid]:-?}" 'exited during measurement'
    exited=1
    continue
  fi
  last_tick="$(ticks "$pid")"
  delta=$(( last_tick - first_tick[$pid] ))
  rss="$(rss_kib "$pid")"
  pss="$(pss_kib "$pid")"
  cpu="$(awk -v ticks="$delta" -v hz="$clock_ticks" -v ms="$elapsed_ms" \
    'BEGIN { if (ms > 0) printf "%.3f", ticks * 100000 / (hz * ms); else print "0.000" }')"
  printf '%-7s %-16s %12s %12s %10s\n' "$pid" "${name[$pid]}" "$rss" "$pss" "$cpu"
  if [[ "${name[$pid]}" == zdesk ]]; then
    desk_rss=$(( desk_rss + rss ))
    desk_pss=$(( desk_pss + pss ))
    desk_ticks=$(( desk_ticks + delta ))
  elif [[ "${name[$pid]}" == conky ]]; then
    conky_rss=$(( conky_rss + rss ))
    conky_pss=$(( conky_pss + pss ))
    conky_ticks=$(( conky_ticks + delta ))
  fi
done

for group in zDesk Conky; do
  if [[ "$group" == Conky ]] && (( conky_count == 0 )); then
    continue
  fi
  if [[ "$group" == zDesk ]]; then
    rss="$desk_rss"
    pss="$desk_pss"
    delta="$desk_ticks"
  else
    rss="$conky_rss"
    pss="$conky_pss"
    delta="$conky_ticks"
  fi
  cpu="$(awk -v ticks="$delta" -v hz="$clock_ticks" -v ms="$elapsed_ms" \
    'BEGIN { if (ms > 0) printf "%.3f", ticks * 100000 / (hz * ms); else print "0.000" }')"
  printf '%-24s %12s %12s %10s\n' "Total $group" "$rss" "$pss" "$cpu"
done

echo 'RSS counts shared memory multiple times; PSS distributes shared memory among processes.'
if (( conky_count == 0 )); then
  echo 'Conky is not running; this measurement covers only zDesk.'
fi
if (( exited != 0 )); then
  echo 'The measurement is invalid because a process exited during the interval.' >&2
  exit 1
fi
