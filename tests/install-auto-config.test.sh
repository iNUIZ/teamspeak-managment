#!/usr/bin/env bash
set -Eeuo pipefail
fixture=$(mktemp -d)
trap 'rm -rf -- "$fixture"' EXIT
export TEAMHUB_ROOT="$fixture" TEAMHUB_PROJECT=testhub
source "$(dirname "${BASH_SOURCE[0]}")/../install.sh"
printf 'COMPOSE_PROJECT_NAME=testhub\nTS3_API_KEY=test-key-3\nTS6_API_KEY=test-key-6\n' > "$fixture/.env"
manager_present=0
central_manager=0
ts3_present=0
ts6_present=0
network_attached=1
touch "$fixture/events" "$fixture/registered"

docker() {
  case "$1" in
    ps)
      if [[ $* == *service=manager* ]]; then
        if [[ $* == *project=testhub* && $manager_present == 1 ]] || [[ $* == *project=teamhub\ * && $central_manager == 1 ]]; then printf 'manager-id\n'; fi
      elif [[ $* == *service=ts3* && $ts3_present == 1 ]]; then printf 'ts3-id\n'
      elif [[ $* == *service=ts6* && $ts6_present == 1 ]]; then printf 'ts6-id\n'; fi
      ;;
    network)
      if [[ $2 == inspect ]]; then printf 'testhub\n'; else printf 'network-connect\n' >> "$fixture/events"; fi
      ;;
    inspect)
      if [[ $2 == manager-id ]]; then
        [[ $network_attached == 0 ]] || printf 'testhub\n'
      else printf '/testhub-%s-1\n' "${2%-id}"; fi
      ;;
    exec)
      if [[ $* == *sync-connection* ]]; then
        local name host key
        IFS= read -r -d '' name
        IFS= read -r -d '' host
        IFS= read -r -d '' key
        [[ -n $name && -n $key && $host == testhub-ts*-1 ]]
        if grep -Fxq "$host" "$fixture/registered"; then printf 'unchanged\n'; else
          printf '%s\n' "$host" >> "$fixture/registered"
          printf 'changed\n'
        fi
      fi
      ;;
    restart) printf 'restart\n' >> "$fixture/events" ;;
    *) return 1 ;;
  esac
}

# Server first: defer until Manager exists, without registering anything.
ts3_present=1
sync_manager_connections
[[ ! -s $fixture/registered ]]
manager_present=1
sync_manager_connections
[[ $(wc -l < "$fixture/registered") == 1 ]]

# Manager first, then another server: add TS6 alongside TS3, without duplicating TS3.
ts6_present=1
sync_manager_connections
[[ $(wc -l < "$fixture/registered") == 2 ]]
restarts=$(wc -l < "$fixture/events")
sync_manager_connections
[[ $(wc -l < "$fixture/events") == "$restarts" ]]

# A second stack can connect its servers to the existing primary Manager.
manager_present=0
central_manager=1
network_attached=0
sync_manager_connections
grep -Fxq network-connect "$fixture/events"
printf 'PASS: both installation orders, TS3+TS6 coexistence, idempotence and cross-stack network linking\n'
