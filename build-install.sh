#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

cargo build --release

cp target/release/jm "$HOME/.local/bin/jm"

echo "Installed jm to $HOME/.local/bin/jm"
