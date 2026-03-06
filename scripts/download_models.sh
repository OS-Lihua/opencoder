#!/bin/bash
set -euo pipefail

# Download models.dev snapshot for embedding in the binary.
# Run before release to update the bundled model database.

DEST="crates/provider/src/models-snapshot.json"

echo "Downloading models.dev snapshot..."
curl -sS --max-time 30 "https://models.dev/api.json" -o "$DEST"

# Validate JSON
if ! python3 -c "import json; json.load(open('$DEST'))" 2>/dev/null; then
    if ! jq empty "$DEST" 2>/dev/null; then
        echo "Error: downloaded file is not valid JSON"
        rm -f "$DEST"
        exit 1
    fi
fi

SIZE=$(du -h "$DEST" | cut -f1)
echo "Saved to $DEST ($SIZE)"
