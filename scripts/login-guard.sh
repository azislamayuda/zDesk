#!/usr/bin/env bash
set -euo pipefail

umask 077
state_dir="${HOME:?}/.cache/zdesk"
marker="$state_dir/startup-pending"
boot_id=$(< /proc/sys/kernel/random/boot_id)

log() {
  printf 'zdesk login guard: %s\n' "$*" >&2
}

pending() {
  [[ -e "$marker" || -L "$marker" ]]
}

case "${1:-}" in
  check)
    if pending; then
      previous_boot="unknown"
      if [[ -r "$marker" ]]; then
        IFS= read -r previous_boot < "$marker" || true
      fi
      log "automatic start skipped: previous attempt is pending (boot $previous_boot); run '$0 reset' after logging in to retry"
      exit 1
    fi
    if [[ -z "${DISPLAY:-}" || -z "${WAYLAND_DISPLAY:-}" \
       || -z "${XAUTHORITY:-}" || ! -r "${XAUTHORITY:-}" ]]; then
      log 'automatic start skipped: DISPLAY, WAYLAND_DISPLAY, or XAUTHORITY is unavailable'
      exit 1
    fi
    ;;
  arm)
    mkdir -p -- "$state_dir"
    temp=$(mktemp "$state_dir/.startup.XXXXXXXX")
    trap 'rm -f -- "$temp"' EXIT
    printf '%s\n' "$boot_id" > "$temp"
    if ! ln -- "$temp" "$marker" 2>/dev/null; then
      log 'automatic start skipped: an attempt is already pending'
      exit 1
    fi
    rm -f -- "$temp"
    trap - EXIT
    log "startup armed (boot $boot_id)"
    ;;
  stopped)
    if [[ "${SERVICE_RESULT:-}" != success ]]; then
      log "service ended with result ${SERVICE_RESULT:-unknown}; keeping recovery marker"
      exit 0
    fi
    if ! pending; then
      log 'no recovery marker exists'
      exit 0
    fi
    marked_boot=""
    if [[ -r "$marker" ]]; then
      IFS= read -r marked_boot < "$marker" || true
    fi
    if [[ "$marked_boot" != "$boot_id" ]]; then
      log 'recovery marker belongs to another attempt; keeping it'
      exit 0
    fi
    rm -- "$marker"
    log 'service stopped cleanly; recovery marker cleared'
    ;;
  reset)
    if pending; then
      rm -- "$marker"
      log 'recovery marker cleared; the next automatic start may run'
    else
      log 'no recovery marker exists'
    fi
    ;;
  *)
    printf 'usage: %s {check|arm|stopped|reset}\n' "$0" >&2
    exit 2
    ;;
esac
