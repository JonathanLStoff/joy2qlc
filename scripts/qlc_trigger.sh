#!/usr/bin/env bash
# Example helper script that forwards simple commands to a QLC+ HTTP endpoint
# or prints a message for local testing. Edit this to match your QLC+ setup.

set -euo pipefail

QLC_URL=${QLC_REST_URL:-http://localhost:9999/qlc}
CMD="$1"
ARG="${2-}"

echo "[qlc_trigger] sending command=${CMD} arg=${ARG} to ${QLC_URL}"

# Example: POST JSON payload; customize to match your QLC+ API
RESP=$(curl -sSf -X POST "$QLC_URL" -H "Content-Type: application/json" -d "{\"cmd\":\"$CMD\",\"arg\":\"$ARG\"}" || true)

if [ -n "$RESP" ]; then
  echo "[qlc_trigger] response: $RESP"
else
  echo "[qlc_trigger] no response or request failed"
fi
