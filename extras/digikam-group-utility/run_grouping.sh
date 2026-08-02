#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 /path/to/exported/images [additional cli args]"
  exit 1
fi

export_dir="$1"
shift || true

python3 "$SCRIPT_DIR/src/cli.py" "$export_dir" "$@"
