# Extras

This directory contains optional helper utilities that are not part of the core dumper binary.

## NAS Diagnostics Helper

Script: [extras/nas_diagnose.sh](extras/nas_diagnose.sh)

Use this script to collect client-side diagnostics when a NAS becomes unstable under load (for example, during export).

### What it collects

- Network checks (DNS resolution, ping, common NAS service ports)
- Mount context (findmnt, mount options, disk/inode usage)
- Recent kernel logs, including filtered timeout/reset/storage keywords
- Optional low-volume write/read probe throughput
- Timestamped output directory for side-by-side healthy vs stressed comparisons

### Usage

Required arguments:

- --host: NAS hostname or IP (example: 192.168.1.7)
- --mount: Mounted path on this machine (example: /mnt/photos)

Basic run:

```shell
extras/nas_diagnose.sh --host 192.168.1.7 --mount /mnt/photos
```

Passive (no write/read probe):

```shell
extras/nas_diagnose.sh --host 192.168.1.7 --mount /mnt/photos --skip-write-probe
```

Repeated sampling during another workload:

```shell
extras/nas_diagnose.sh --host 192.168.1.7 --mount /mnt/photos --loops 20 --interval-sec 60
```

### Output

By default, output is written to a timestamped directory in the current working directory:

- nas-diag-YYYYmmdd-HHMMSS/

Inside that directory, report.txt summarizes results and references per-command output/error files.

### Safety notes

- The script is designed to be low impact.
- Use --skip-write-probe if you want passive diagnostics only.
- If you enable probes, keep --probe-mib small on fragile systems.

### Recommended workflow

1. Run one passive baseline capture when the NAS is healthy.
2. Run repeated sampling while a heavy workload is active.
3. Compare both runs, focusing on first appearance of:
   - ping failures,
   - NFS/SMB timeout/reset messages,
   - probe throughput collapse or probe failures.
