#!/bin/bash
set -euo pipefail

# Bump version across all workspace Cargo.toml files.
#
# Usage:
#   ./scripts/version-bump.sh 0.2.0
#   ./scripts/version-bump.sh patch    # auto-increment: 0.1.0 → 0.1.1
#   ./scripts/version-bump.sh minor    # auto-increment: 0.1.0 → 0.2.0
#   ./scripts/version-bump.sh major    # auto-increment: 0.1.0 → 1.0.0

if [ $# -ne 1 ]; then
    echo "Usage: $0 <version|patch|minor|major>"
    exit 1
fi

# Read current version
CURRENT=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)"/\1/')
echo "Current version: ${CURRENT}"

# Determine new version
INPUT="$1"
IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT"

case "$INPUT" in
    patch) NEW_VERSION="${MAJOR}.${MINOR}.$((PATCH + 1))" ;;
    minor) NEW_VERSION="${MAJOR}.$((MINOR + 1)).0" ;;
    major) NEW_VERSION="$((MAJOR + 1)).0.0" ;;
    *)
        # Validate semver format
        if [[ ! "$INPUT" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
            echo "Error: invalid version format '${INPUT}'. Expected X.Y.Z or patch/minor/major"
            exit 1
        fi
        NEW_VERSION="$INPUT"
        ;;
esac

echo "New version: ${NEW_VERSION}"
echo ""

# Update workspace version in root Cargo.toml
sed -i.bak "s/^version = \"${CURRENT}\"/version = \"${NEW_VERSION}\"/" Cargo.toml
rm -f Cargo.toml.bak

# Count updated files
COUNT=1
echo "Updated: Cargo.toml (workspace)"

# Update lockfile
cargo update --workspace 2>/dev/null || true

echo ""
echo "✅ Version bumped: ${CURRENT} → ${NEW_VERSION}"
echo ""
echo "Next steps:"
echo "  git add -A"
echo "  git commit -m 'chore: release v${NEW_VERSION}'"
echo "  git tag v${NEW_VERSION}"
echo "  git push origin master --tags"
