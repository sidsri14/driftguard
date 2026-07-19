#!/usr/bin/env sh
set -eu

project_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
binary="$project_root/target/debug/driftguard"

printf '%s\n' 'Building DriftGuard...'
cargo build --quiet --manifest-path "$project_root/Cargo.toml"

printf '\n%s\n' '1. Diagnose the fixed example'
(cd "$project_root/examples/fixed-ai-app" && "$binary" doctor)

printf '\n%s\n' '2. Show a deployment contract failure'
set +e
(cd "$project_root/examples/broken-ai-app" && "$binary" check)
broken_status=$?
set -e
if [ "$broken_status" -ne 1 ]; then
    printf 'Broken example returned unexpected exit code %s\n' "$broken_status" >&2
    exit 2
fi

printf '\n%s\n' '3. Show the corrected project passing'
(cd "$project_root/examples/fixed-ai-app" && "$binary" check)

printf '\n%s\n' 'Demo complete.'
