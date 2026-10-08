#!/bin/bash
# shot.sh <mode> <hover> <empty> <name>: open the preview ring, capture it, close it.
D="$(cd "$(dirname "$0")" && pwd)"
X=40; Y=80; W=274
OPENLOGI_RING_PREVIEW="$1" OPENLOGI_RING_PREVIEW_HOVER="$2" OPENLOGI_RING_PREVIEW_EMPTY="$3" \
  OPENLOGI_RING_PREVIEW_AT="$X,$Y" "$D/openlogi-overlay-preview" >"$D/preview.log" 2>&1 &
PID=$!
( sleep 8; kill -9 "$PID" 2>/dev/null ) &   # hard self-termination
sleep 1.6
screencapture -x -R "$X,$Y,$W,$W" "$D/$4.png"
kill "$PID" 2>/dev/null; sleep 0.3; kill -9 "$PID" 2>/dev/null
kill -0 "$PID" 2>/dev/null && echo "STILL RUNNING $PID" || echo "closed $4"
