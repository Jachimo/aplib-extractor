#!/usr/bin/env bash

# Lightweight NAS diagnosis helper for client-side investigation.
# Collects network, mount, kernel-log, and optional low-volume read/write probe data.

set -u
set -o pipefail

HOST=""
MOUNT_PATH=""
OUT_DIR=""
PROBE_MIB=8
TIMEOUT_SEC=10
LOOPS=1
INTERVAL_SEC=30
SKIP_WRITE_PROBE=0

usage() {
  cat <<'EOF'
Usage:
  extras/nas_diagnose.sh --host <nas_host_or_ip> --mount <mounted_path> [options]

Required:
  --host HOST             NAS hostname or IP (example: 192.168.1.7)
  --mount PATH            Mounted NAS path to probe (example: /mnt/photos)

Options:
  --out DIR               Output directory (default: ./nas-diag-YYYYmmdd-HHMMSS)
  --probe-mib N           Probe file size in MiB for read/write test (default: 8)
  --timeout-sec N         Timeout in seconds for probe/network commands (default: 10)
  --loops N               Number of probe loops (default: 1)
  --interval-sec N        Seconds between loops (default: 30)
  --skip-write-probe      Do not write/read probe file; gather passive diagnostics only
  --help                  Show this help

Examples:
  extras/nas_diagnose.sh --host 192.168.1.7 --mount /mnt/photos
  extras/nas_diagnose.sh --host 192.168.1.7 --mount /mnt/photos --loops 20 --interval-sec 60
  extras/nas_diagnose.sh --host 192.168.1.7 --mount /mnt/photos --skip-write-probe
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --host)
      HOST="${2:-}"
      shift 2
      ;;
    --mount)
      MOUNT_PATH="${2:-}"
      shift 2
      ;;
    --out)
      OUT_DIR="${2:-}"
      shift 2
      ;;
    --probe-mib)
      PROBE_MIB="${2:-}"
      shift 2
      ;;
    --timeout-sec)
      TIMEOUT_SEC="${2:-}"
      shift 2
      ;;
    --loops)
      LOOPS="${2:-}"
      shift 2
      ;;
    --interval-sec)
      INTERVAL_SEC="${2:-}"
      shift 2
      ;;
    --skip-write-probe)
      SKIP_WRITE_PROBE=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

if [[ -z "$HOST" || -z "$MOUNT_PATH" ]]; then
  echo "Error: --host and --mount are required." >&2
  usage
  exit 2
fi

if ! [[ "$PROBE_MIB" =~ ^[0-9]+$ && "$PROBE_MIB" -gt 0 ]]; then
  echo "Error: --probe-mib must be a positive integer." >&2
  exit 2
fi

if ! [[ "$TIMEOUT_SEC" =~ ^[0-9]+$ && "$TIMEOUT_SEC" -gt 0 ]]; then
  echo "Error: --timeout-sec must be a positive integer." >&2
  exit 2
fi

if ! [[ "$LOOPS" =~ ^[0-9]+$ && "$LOOPS" -gt 0 ]]; then
  echo "Error: --loops must be a positive integer." >&2
  exit 2
fi

if ! [[ "$INTERVAL_SEC" =~ ^[0-9]+$ && "$INTERVAL_SEC" -ge 0 ]]; then
  echo "Error: --interval-sec must be a non-negative integer." >&2
  exit 2
fi

if [[ -z "$OUT_DIR" ]]; then
  OUT_DIR="./nas-diag-$(date +%Y%m%d-%H%M%S)"
fi

mkdir -p "$OUT_DIR"
REPORT="$OUT_DIR/report.txt"

log() {
  printf '[%s] %s\n' "$(date -Iseconds)" "$*" | tee -a "$REPORT"
}

section() {
  printf '\n=== %s ===\n' "$*" | tee -a "$REPORT"
}

safe_timeout() {
  if command -v timeout >/dev/null 2>&1; then
    timeout "${TIMEOUT_SEC}s" "$@"
  else
    "$@"
  fi
}

run_capture() {
  local label="$1"
  shift
  local file_stub
  file_stub=$(printf '%s' "$label" | tr ' ' '_' | tr -c 'A-Za-z0-9_.-' '_')
  local out_file="$OUT_DIR/${file_stub}.out"
  local err_file="$OUT_DIR/${file_stub}.err"

  log "Running: $label"
  safe_timeout "$@" >"$out_file" 2>"$err_file"
  local rc=$?
  log "Result: $label (exit=$rc, out=$(basename "$out_file"), err=$(basename "$err_file"))"
  return "$rc"
}

mib_per_sec() {
  local mib="$1"
  local ms="$2"
  awk -v mib="$mib" -v ms="$ms" 'BEGIN { if (ms <= 0) print "NA"; else printf "%.2f", (mib * 1000.0) / ms }'
}

capture_mount_context() {
  section "Mount Context"
  run_capture "findmnt_target" findmnt -T "$MOUNT_PATH" -o TARGET,SOURCE,FSTYPE,OPTIONS -n
  run_capture "mount_grep" sh -c "mount | grep -F -- '$MOUNT_PATH' || true"
  run_capture "df_h" df -h "$MOUNT_PATH"
  run_capture "df_i" df -i "$MOUNT_PATH"
}

capture_network_context() {
  section "Network Context"
  run_capture "getent_hosts" getent hosts "$HOST"
  run_capture "ping" ping -c 2 -W 2 "$HOST"

  if command -v nc >/dev/null 2>&1; then
    run_capture "tcp_2049_nfs" nc -z -w 2 "$HOST" 2049
    run_capture "tcp_445_smb" nc -z -w 2 "$HOST" 445
    run_capture "tcp_80_http" nc -z -w 2 "$HOST" 80
    run_capture "tcp_443_https" nc -z -w 2 "$HOST" 443
  else
    log "Skipping TCP port checks: nc not found"
  fi
}

capture_kernel_context() {
  section "Kernel Context"
  run_capture "journalctl_kernel_recent" sh -c "journalctl -k --since '-20 min' 2>/dev/null || true"
  run_capture "journalctl_kernel_filtered" sh -c "journalctl -k --since '-20 min' 2>/dev/null | grep -E -i 'nfs|cifs|smb|raid|md|i/o|timeout|reset|disconnect|hung' || true"

  if [[ -r /proc/self/mountstats ]]; then
    run_capture "proc_mountstats" cat /proc/self/mountstats
    run_capture "proc_mountstats_filtered" sh -c "grep -E -i 'nfs|events|bytes|timeout|age|ops' /proc/self/mountstats || true"
  fi
}

probe_mount_path() {
  local loop_idx="$1"
  section "Probe Loop ${loop_idx}/${LOOPS}"

  run_capture "probe_stat_mount_loop_${loop_idx}" stat "$MOUNT_PATH"
  run_capture "probe_list_mount_loop_${loop_idx}" sh -c "ls -la '$MOUNT_PATH' | head -n 100"

  if [[ "$SKIP_WRITE_PROBE" -eq 1 ]]; then
    log "Skipping write/read probe due to --skip-write-probe"
    return 0
  fi

  local probe_file="$MOUNT_PATH/.nas_diag_probe_$$_${loop_idx}.bin"
  local write_start write_end write_ms read_start read_end read_ms

  write_start=$(date +%s%3N)
  safe_timeout dd if=/dev/zero of="$probe_file" bs=1M count="$PROBE_MIB" conv=fsync status=none
  local write_rc=$?
  write_end=$(date +%s%3N)
  write_ms=$((write_end - write_start))

  if [[ "$write_rc" -eq 0 ]]; then
    log "Write probe: ${PROBE_MIB} MiB in ${write_ms} ms ($(mib_per_sec "$PROBE_MIB" "$write_ms") MiB/s)"
  else
    log "Write probe FAILED: exit=$write_rc after ${write_ms} ms"
    rm -f "$probe_file" >/dev/null 2>&1 || true
    return 1
  fi

  read_start=$(date +%s%3N)
  safe_timeout dd if="$probe_file" of=/dev/null bs=1M status=none
  local read_rc=$?
  read_end=$(date +%s%3N)
  read_ms=$((read_end - read_start))

  if [[ "$read_rc" -eq 0 ]]; then
    log "Read probe:  ${PROBE_MIB} MiB in ${read_ms} ms ($(mib_per_sec "$PROBE_MIB" "$read_ms") MiB/s)"
  else
    log "Read probe FAILED: exit=$read_rc after ${read_ms} ms"
  fi

  rm -f "$probe_file" >/dev/null 2>&1 || true
  return "$read_rc"
}

write_summary() {
  section "Quick Interpretation"

  local ping_rc=1
  if grep -q -E -i 'bytes from|icmp_seq=' "$OUT_DIR/ping.out" 2>/dev/null; then
    ping_rc=0
  fi

  if grep -q -E -i 'not responding|timed out|server .* not responding|I/O error|reset|hung task' "$OUT_DIR/journalctl_kernel_filtered.out" 2>/dev/null; then
    log "Signal: Kernel logs show transport/storage errors (NFS/SMB timeout/reset/hung)."
  else
    log "Signal: No obvious timeout/reset/hung keywords in recent kernel logs."
  fi

  if [[ "$ping_rc" -ne 0 ]]; then
    log "Signal: Ping failed or was unreliable in this capture."
  else
    log "Signal: Ping reachable during this capture."
  fi

  log "Interpretation: If failures appear only during heavy load, suspect marginal disk/RAID health or NAS controller/firmware limits under sustained I/O."
  log "Next: Compare this report against a low-load run and a high-load run to identify what changes first (ping, mount ops, kernel timeout messages, probe throughput)."
}

{
  echo "NAS Diagnose Report"
  echo "Generated: $(date -Iseconds)"
  echo "Host: $HOST"
  echo "Mount path: $MOUNT_PATH"
  echo "Probe MiB: $PROBE_MIB"
  echo "Timeout sec: $TIMEOUT_SEC"
  echo "Loops: $LOOPS"
  echo "Interval sec: $INTERVAL_SEC"
  echo "Skip write probe: $SKIP_WRITE_PROBE"
} >"$REPORT"

log "Output directory: $OUT_DIR"
capture_network_context
capture_mount_context
capture_kernel_context

loop=1
while [[ "$loop" -le "$LOOPS" ]]; do
  probe_mount_path "$loop" || true
  if [[ "$loop" -lt "$LOOPS" && "$INTERVAL_SEC" -gt 0 ]]; then
    log "Sleeping ${INTERVAL_SEC}s before next loop"
    sleep "$INTERVAL_SEC"
  fi
  loop=$((loop + 1))
done

capture_kernel_context
write_summary

log "Done. See report: $REPORT"
