#!/bin/sh
# Fetches and runs the latest plzplz installer from GitHub releases.
# The installer verifies artifact checksums before installing.
set -eu

installer=$(curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/k88hudson/plzplz/releases/latest/download/plzplz-installer.sh)

printf '%s\n' "$installer" | sh -s -- "$@"
