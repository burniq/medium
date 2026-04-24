#!/usr/bin/env bash
set -euo pipefail

workdir="$(mktemp -d)"
home_dir="$workdir/home"
mkdir -p "$home_dir"

control_log="$workdir/control-plane.log"
pair_log="$workdir/pair.log"
devices_log="$workdir/devices.log"
sync_log="$workdir/sync.log"
control_addr="127.0.0.1:18080"

OVERLAY_CONTROL_BIND_ADDR="$control_addr" cargo run -p control-plane >"$control_log" 2>&1 &
control_pid=$!

cleanup() {
  kill "$control_pid" 2>/dev/null || true
  wait "$control_pid" 2>/dev/null || true
  rm -rf "$workdir"
}
trap cleanup EXIT

for _ in $(seq 1 30); do
  if curl --fail --silent "http://$control_addr/health" >/dev/null; then
    break
  fi
  sleep 1
done

curl --fail --silent "http://$control_addr/health" >/dev/null

OVERLAY_HOME="$home_dir" cargo run -p linux-client --bin overlay -- \
  pair --server "http://$control_addr" --device macbook >"$pair_log"
grep -q "paired macbook with http://$control_addr" "$pair_log"

OVERLAY_HOME="$home_dir" cargo run -p linux-client --bin overlay -- \
  devices >"$devices_log"
grep -q "node-home ssh overlay@127.0.0.1:2222" "$devices_log"

if OVERLAY_HOME="$home_dir" cargo run -p linux-client --bin overlay -- ssh sync \
  >"$workdir/should-fail.log" 2>&1; then
  echo "ssh sync unexpectedly succeeded without --write-main-config" >&2
  exit 1
fi
grep -q "re-run with --write-main-config" "$workdir/should-fail.log"

OVERLAY_HOME="$home_dir" cargo run -p linux-client --bin overlay -- \
  ssh sync --write-main-config >"$sync_log"
grep -q "synced 1 SSH hosts" "$sync_log"
grep -q "Include ~/.ssh/config.d/overlay.conf" "$home_dir/.ssh/config"
grep -q "Host node-home" "$home_dir/.ssh/config.d/overlay.conf"
grep -q "ProxyCommand overlay proxy ssh --device node-home" "$home_dir/.ssh/config.d/overlay.conf"

OVERLAY_HOME="$home_dir" cargo run -p linux-client --bin overlay -- ssh sync >/dev/null
backup_file="$(find "$home_dir/.ssh/config.d" -name 'overlay.bak-*' -print -quit)"
test -n "$backup_file"
