#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
#
# SPDX-License-Identifier: Apache-2.0
#
# Runs the loopback campaigns (load, switchover under load) and writes their reports under
# docs/campaigns/. Usage, from the repository root: bench/campaign/run.sh
# LOAD_SECONDS sets the length of the load campaign (60 by default).
set -uo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
out="$root/docs/campaigns"
mkdir -p "$out"
cd "$root" || exit 1
cargo build --release -p igs-iec104-campaign || exit 1
bin="$root/target/release/igs-iec104-campaign"

environment() {
    echo "- Machine: $(uname -sr), $(nproc) CPUs, $(awk -F: '/model name/ {print $2; exit}' /proc/cpuinfo | xargs)"
    local changes=""
    if [ -n "$(git status --porcelain)" ]; then changes=", with uncommitted changes"; fi
    echo "- Commit: $(git rev-parse --short HEAD)${changes}"
    echo "- Command: \`$*\`"
    echo
}

status=0
{ environment "igs-iec104-campaign load --seconds ${LOAD_SECONDS:-60}"; "$bin" load --seconds "${LOAD_SECONDS:-60}"; } \
    > "$out/load.md" || status=1
{ environment "igs-iec104-campaign switchover"; "$bin" switchover; } > "$out/switchover.md" || status=1
exit "$status"
