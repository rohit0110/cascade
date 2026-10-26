#!/usr/bin/env bash
# Toggle an OTel Demo fault flag via the flagd config file (config-only copy in stack/).
#   deploy/demo/fault.sh list
#   deploy/demo/fault.sh paymentFailure 100%     # variants vary per flag; see `list`
#   deploy/demo/fault.sh paymentUnreachable on
#   deploy/demo/fault.sh paymentFailure off      # recover
set -euo pipefail
f="$(cd "$(dirname "$0")" && pwd)/stack/src/flagd/demo.flagd.json"
if [ "${1:-}" = "list" ] || [ $# -ne 2 ]; then
  python3 -I - "$f" <<'PY'
import json, sys
for k, v in json.load(open(sys.argv[1]))["flags"].items():
    print(f"{k:34} now={v['defaultVariant']:6} variants={list(v['variants'])}")
PY
  [ "${1:-}" = "list" ] || { echo "usage: $0 <flag> <variant> | list" >&2; exit 2; }
  exit 0
fi
# Rewrite in place (same inode) so the bind mount and flagd's file watcher see it.
python3 -I - "$f" "$1" "$2" <<'PY'
import json, sys
path, flag, variant = sys.argv[1:]
d = json.load(open(path))
if flag not in d["flags"]: sys.exit(f"unknown flag {flag}")
if variant not in d["flags"][flag]["variants"]:
    sys.exit(f"unknown variant {variant}; choose from {list(d['flags'][flag]['variants'])}")
d["flags"][flag]["defaultVariant"] = variant
with open(path, "r+") as fh:
    fh.seek(0); fh.write(json.dumps(d, indent=2) + "\n"); fh.truncate()
print(f"{flag} -> {variant}")
PY
