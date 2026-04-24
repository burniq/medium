#!/usr/bin/env bash
set -euo pipefail

workdir="$(mktemp -d)"
home_dir="$workdir/home"
mkdir -p "$home_dir"

control_log="$workdir/control-plane.log"
home_log="$workdir/home-node.log"
pair_log="$workdir/pair.log"
devices_log="$workdir/devices.log"
sync_log="$workdir/sync.log"
banner_log="$workdir/banner.log"
control_addr="127.0.0.1:18080"
home_addr="127.0.0.1:17001"
target_addr="127.0.0.1:2222"
shared_secret="local-secret"
home_config="$workdir/home-node.toml"
db_url="sqlite://$workdir/control-plane.db"

cat >"$home_config" <<EOF
node_id = "node-home"
node_label = "node-home"
bind_addr = "$home_addr"

[[services]]
id = "svc_home_ssh"
kind = "ssh"
user_name = "overlay"
target = "$target_addr"
EOF

OVERLAY_CONTROL_BIND_ADDR="$control_addr" \
OVERLAY_SHARED_SECRET="$shared_secret" \
OVERLAY_CONTROL_DATABASE_URL="$db_url" \
cargo run -p control-plane >"$control_log" 2>&1 &
control_pid=$!

OVERLAY_SHARED_SECRET="$shared_secret" \
OVERLAY_CONTROL_URL="http://$control_addr" \
cargo run -p home-node -- --config "$home_config" >"$home_log" 2>&1 &
home_pid=$!

cleanup() {
  kill "$control_pid" 2>/dev/null || true
  wait "$control_pid" 2>/dev/null || true
  kill "$home_pid" 2>/dev/null || true
  wait "$home_pid" 2>/dev/null || true
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

for _ in $(seq 1 30); do
  if nc -z 127.0.0.1 17001 >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

nc -z 127.0.0.1 17001 >/dev/null 2>&1

OVERLAY_HOME="$home_dir" cargo run -p linux-client --bin overlay -- \
  pair --server "http://$control_addr" --device macbook >"$pair_log"
grep -q "paired macbook with http://$control_addr" "$pair_log"

OVERLAY_HOME="$home_dir" cargo run -p linux-client --bin overlay -- \
  devices >"$devices_log"
grep -q "node-home ssh overlay@127.0.0.1:17001" "$devices_log"

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

python3 -c 'import socket; s=socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1); s.bind(("127.0.0.1", 2222)); s.listen(1); conn, _ = s.accept(); conn.sendall(b"SSH-2.0-OverlayTest\r\n"); conn.close(); s.close()' &
target_pid=$!
printf '' | OVERLAY_HOME="$home_dir" cargo run -p linux-client --bin overlay -- \
  proxy ssh --device node-home >"$banner_log"
wait "$target_pid"
grep -q "SSH-2.0-OverlayTest" "$banner_log"

OVERLAY_HOME="$home_dir" cargo run -p linux-client --bin overlay -- ssh sync >/dev/null
backup_file="$(find "$home_dir/.ssh/config.d" -name 'overlay.bak-*' -print -quit)"
test -n "$backup_file"
