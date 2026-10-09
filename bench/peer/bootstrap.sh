#!/bin/bash
# SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
#
# SPDX-License-Identifier: Apache-2.0
#
# Starts Fledge inside the reference peer container and creates the single
# service described by the JSON file given as first argument:
#   {"name": ..., "type": "north"|"south", "plugin": ..., "config": {item: object}}
# Every config item is passed to Fledge as a JSON string value. Once the
# service runs, /tmp/bench-ready is created (polled by the bench helpers).
set -euo pipefail

definition=${1:?usage: bootstrap.sh <service definition JSON>}
api=http://localhost:8081/fledge

# Containers have no kernel log and no service manager.
sed -i '/imklog/s/^/#/' /etc/rsyslog.conf
rsyslogd

/usr/local/fledge/bin/fledge start
for _ in {1..90}; do
    curl -fsS "$api/ping" >/dev/null 2>&1 && break
    sleep 1
done

token=$(curl -fsS -X POST "$api/login" -H 'content-type: application/json' \
    -d '{"username":"admin","password":"fledge"}' | jq -er '.token')
auth=(-H "authorization: $token")

name=$(jq -er '.name' "$definition")
body=$(jq -c '{name, type, plugin, enabled: true,
               config: (.config | map_values({value: tojson}))}' "$definition")
curl -fsS "${auth[@]}" -X POST "$api/service" -H 'content-type: application/json' \
    -d "$body" >/dev/null

for _ in {1..90}; do
    if curl -fsS "${auth[@]}" "$api/service?name=$name" |
        jq -e --arg n "$name" '.services[] | select(.name == $n and .status == "running")' \
            >/dev/null; then
        touch /tmp/bench-ready
        echo "bench: service $name running"
        break
    fi
    sleep 1
done
[[ -f /tmp/bench-ready ]] || { echo "bench: service $name did not start" >&2; exit 1; }

exec tail -F /var/log/syslog
