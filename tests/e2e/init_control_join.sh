#!/usr/bin/env bash
set -euo pipefail

workdir="$(mktemp -d)"
server_root="$workdir/server-root"
client_home="$workdir/client-home"
bin_dir="$workdir/bin"
mkdir -p "$server_root" "$client_home" "$bin_dir"

control_addr="127.0.0.1:18081"
home_addr="127.0.0.1:17002"
target_addr="127.0.0.1:2223"
control_config="$server_root/etc/medium/control.toml"
node_config="$server_root/etc/medium/node.toml"
control_log="$workdir/control-plane.log"
home_log="$workdir/home-node.log"
devices_log="$workdir/devices.log"
sync_log="$workdir/sync.log"
ssh_stdout="$workdir/ssh.stdout"
ssh_stderr="$workdir/ssh.stderr"
ssh_pid=""
target_log="$workdir/target.log"
target_ready="$workdir/target.ready"
control_pid=""
home_pid=""
target_pid=""

cleanup() {
  if [ -n "$target_pid" ]; then
    kill "$target_pid" 2>/dev/null || true
    wait "$target_pid" 2>/dev/null || true
  fi
  if [ -n "$ssh_pid" ]; then
    kill "$ssh_pid" 2>/dev/null || true
    wait "$ssh_pid" 2>/dev/null || true
  fi
  if [ -n "$home_pid" ]; then
    kill "$home_pid" 2>/dev/null || true
    wait "$home_pid" 2>/dev/null || true
  fi
  if [ -n "$control_pid" ]; then
    kill "$control_pid" 2>/dev/null || true
    wait "$control_pid" 2>/dev/null || true
  fi
  rm -rf "$workdir"
}
trap cleanup EXIT

toml_value() {
  local key="$1"
  local path="$2"
  sed -n "s/^$key = \"\\(.*\\)\"$/\\1/p" "$path"
}

wait_http() {
  local url="$1"
  for _ in $(seq 1 30); do
    if curl --fail --silent "$url" >/dev/null; then
      return 0
    fi
    sleep 1
  done

  curl --fail --silent "$url" >/dev/null
}

wait_tcp() {
  local host="$1"
  local port="$2"
  for _ in $(seq 1 30); do
    if nc -z "$host" "$port" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done

  nc -z "$host" "$port" >/dev/null 2>&1
}

init_log="$workdir/init-control.log"
MEDIUM_ROOT="$server_root" \
MEDIUM_CONTROL_BIND_ADDR="$control_addr" \
MEDIUM_CONTROL_PUBLIC_URL="http://$control_addr" \
MEDIUM_HOME_NODE_BIND_ADDR="$home_addr" \
cargo run -p linux-client --bin medium -- init-control >"$init_log"

invite="$(sed -n 's/.*generated invite //p' "$init_log")"
test -n "$invite"

shared_secret="$(toml_value "shared_secret" "$control_config")"
database_url="$(toml_value "database_url" "$control_config")"
test -n "$shared_secret"
test -n "$database_url"

# The production bootstrap owns the node config. The e2e redirects only the
# service target so it can prove the SSH path without touching the host SSHD.
sed -i.bak "s#^target = \".*\"#target = \"$target_addr\"#" "$node_config"

OVERLAY_CONTROL_BIND_ADDR="$control_addr" \
OVERLAY_CONTROL_DATABASE_URL="$database_url" \
OVERLAY_SHARED_SECRET="$shared_secret" \
cargo run -p control-plane >"$control_log" 2>&1 &
control_pid=$!
wait_http "http://$control_addr/health"

OVERLAY_CONTROL_URL="http://$control_addr" \
OVERLAY_SHARED_SECRET="$shared_secret" \
cargo run -p home-node -- --config "$node_config" >"$home_log" 2>&1 &
home_pid=$!
wait_tcp 127.0.0.1 17002

OVERLAY_HOME="$client_home" MEDIUM_DEVICE_NAME="macbook" \
cargo run -p linux-client --bin medium -- join "$invite" >"$workdir/join.log"

grep -q "joined macbook via http://$control_addr using invite v1" "$workdir/join.log"

OVERLAY_HOME="$client_home" cargo run -p linux-client --bin medium -- devices >"$devices_log"
grep -q "node-home ssh overlay@127.0.0.1:17002" "$devices_log"

mkdir -p "$client_home/.ssh/config.d"
OVERLAY_HOME="$client_home" cargo run -p linux-client --bin medium -- \
  ssh sync --write-main-config >"$sync_log"
grep -q "synced 1 SSH hosts" "$sync_log"
grep -q "Include ~/.ssh/config.d/medium.conf" "$client_home/.ssh/config"
grep -q "Host node-home" "$client_home/.ssh/config.d/medium.conf"
grep -q "ProxyCommand medium proxy ssh --device node-home" "$client_home/.ssh/config.d/medium.conf"

cat >"$bin_dir/medium" <<EOF
#!/usr/bin/env bash
cd "$(pwd)"
OVERLAY_HOME="$client_home" exec "$(pwd)/target/debug/medium" "\$@"
EOF
chmod 0755 "$bin_dir/medium"

python3 - "$target_log" <<'PY' &
import socket
import sys

target_log = sys.argv[1]
ready_path = target_log.rsplit("/", 1)[0] + "/target.ready"
with socket.socket() as server:
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind(("127.0.0.1", 2223))
    server.listen(1)
    with open(ready_path, "w") as ready:
        ready.write("ready\n")
    conn, _ = server.accept()
    with conn:
        with open(target_log, "wb") as out:
            out.write(b"connected\n")
        conn.sendall(b"SSH-2.0-MediumE2E\r\n")
        try:
            data = conn.recv(256)
        except ConnectionResetError:
            data = b""
        with open(target_log, "ab") as out:
            out.write(data)
PY
target_pid=$!
for _ in $(seq 1 30); do
  if [ -f "$target_ready" ]; then
    break
  fi
  sleep 1
done
test -f "$target_ready"

PATH="$bin_dir:$PATH" HOME="$client_home" ssh \
  -F "$client_home/.ssh/config" \
  -o BatchMode=yes \
  -o StrictHostKeyChecking=no \
  -o UserKnownHostsFile="$workdir/known_hosts" \
  -o ConnectTimeout=5 \
  node-home true >"$ssh_stdout" 2>"$ssh_stderr" &
ssh_pid=$!

wait "$target_pid"
target_pid=""
for _ in $(seq 1 10); do
  if ! kill -0 "$ssh_pid" 2>/dev/null; then
    break
  fi
  sleep 1
done
if kill -0 "$ssh_pid" 2>/dev/null; then
  kill "$ssh_pid" 2>/dev/null || true
fi
wait "$ssh_pid" 2>/dev/null || true
ssh_pid=""
grep -q "connected" "$target_log"
grep -q "SSH-2.0-" "$target_log"
