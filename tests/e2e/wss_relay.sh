#!/usr/bin/env bash
set -euo pipefail

workdir="$(mktemp -d)"
server_root="$workdir/server-root"
client_home="$workdir/client-home"
mkdir -p "$server_root" "$client_home"

read -r control_port node_port target_port relay_port <<EOF
$(python3 - <<'PY'
import socket

ports = []
sockets = []
for _ in range(4):
    sock = socket.socket()
    sock.bind(("127.0.0.1", 0))
    ports.append(sock.getsockname()[1])
    sockets.append(sock)
print(*ports)
for sock in sockets:
    sock.close()
PY
)
EOF

control_addr="127.0.0.1:$control_port"
node_addr="127.0.0.1:$node_port"
target_addr="127.0.0.1:$target_port"
relay_addr="127.0.0.1:$relay_port"
relay_url="ws://$relay_addr/medium/v1/relay"
control_config="$server_root/etc/medium/control.toml"
node_config="$server_root/etc/medium/node.toml"
control_log="$workdir/control-plane.log"
node_log="$workdir/home-node.log"
relay_log="$workdir/relay.log"
target_log="$workdir/target.log"
proxy_stdout="$workdir/proxy.stdout"
proxy_stderr="$workdir/proxy.stderr"
control_pid=""
node_pid=""
relay_pid=""
target_pid=""
proxy_pid=""

cleanup() {
  local status=$?
  if [ "$status" -ne 0 ]; then
    echo "wss relay e2e failed; logs from $workdir:" >&2
    for path in \
      "$workdir/init-control.log" \
      "$workdir/init-node.log" \
      "$relay_log" \
      "$control_log" \
      "$node_log" \
      "$workdir/join.log" \
      "$proxy_stdout" \
      "$proxy_stderr" \
      "$target_log"; do
      if [ -f "$path" ]; then
        echo "===== $path =====" >&2
        cat "$path" >&2
      fi
    done
  fi
  for pid in "$proxy_pid" "$target_pid" "$node_pid" "$control_pid" "$relay_pid"; do
    if [ -n "$pid" ]; then
      kill "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
    fi
  done
  rm -rf "$workdir"
  exit "$status"
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
    if curl --fail --silent --insecure "$url" >/dev/null; then
      return 0
    fi
    sleep 1
  done
  curl --fail --silent --insecure "$url" >/dev/null
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

MEDIUM_ROOT="$server_root" \
MEDIUM_CONTROL_BIND_ADDR="$control_addr" \
MEDIUM_CONTROL_PUBLIC_URL="https://$control_addr" \
MEDIUM_WSS_RELAY_URL="wss://$relay_addr/medium/v1/relay" \
cargo run -p medium-cli --bin medium -- init-control >"$workdir/init-control.log"

invite="$(sed -n 's/^initialized Medium control .* generated invite //p' "$workdir/init-control.log")"
node_invite="$(sed -n 's/^generated node invite //p' "$workdir/init-control.log")"
test -n "$invite"
test -n "$node_invite"

shared_secret="$(toml_value "shared_secret" "$control_config")"
database_url="$(toml_value "database_url" "$control_config")"
control_pin="$(toml_value "control_pin" "$control_config")"
tls_cert_path="$(toml_value "tls_cert_path" "$control_config")"
tls_key_path="$(toml_value "tls_key_path" "$control_config")"
test -n "$shared_secret"
test -n "$database_url"
test -n "$control_pin"
test -n "$tls_cert_path"
test -n "$tls_key_path"

MEDIUM_ROOT="$server_root" \
MEDIUM_NODE_LISTEN_ADDR="$node_addr" \
MEDIUM_NODE_PUBLIC_ADDR="127.0.0.1:1" \
cargo run -p medium-cli --bin medium -- init-node "$node_invite" >"$workdir/init-node.log"
sed -i.bak "s#^target = \".*\"#target = \"$target_addr\"#" "$node_config"

MEDIUM_RELAY_MODE=wss \
MEDIUM_RELAY_BIND_ADDR="$relay_addr" \
MEDIUM_RELAY_SHARED_SECRET="$shared_secret" \
cargo run -p relay >"$relay_log" 2>&1 &
relay_pid=$!
wait_tcp 127.0.0.1 "$relay_port"

OVERLAY_CONTROL_BIND_ADDR="$control_addr" \
OVERLAY_CONTROL_DATABASE_URL="$database_url" \
OVERLAY_SHARED_SECRET="$shared_secret" \
MEDIUM_CONTROL_PIN="$control_pin" \
MEDIUM_CONTROL_TLS_CERT_PATH="$tls_cert_path" \
MEDIUM_CONTROL_TLS_KEY_PATH="$tls_key_path" \
MEDIUM_WSS_RELAY_URL="$relay_url" \
cargo run -p control-plane >"$control_log" 2>&1 &
control_pid=$!
wait_http "https://$control_addr/health"

OVERLAY_CONTROL_URL="https://$control_addr" \
OVERLAY_SHARED_SECRET="$shared_secret" \
MEDIUM_CONTROL_PIN="$control_pin" \
MEDIUM_WSS_RELAY_URL="$relay_url" \
cargo run -p home-node -- --config "$node_config" >"$node_log" 2>&1 &
node_pid=$!
wait_tcp 127.0.0.1 "$node_port"

OVERLAY_HOME="$client_home" MEDIUM_DEVICE_NAME="macbook" \
cargo run -p medium-cli --bin medium -- join "$invite" >"$workdir/join.log"
grep -q "joined macbook via https://$control_addr using invite v1" "$workdir/join.log"

python3 - "$target_addr" "$target_log" <<'PY' &
import socket
import sys

host, port = sys.argv[1].rsplit(":", 1)
target_log = sys.argv[2]
with socket.socket() as server:
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind((host, int(port)))
    server.listen(1)
    with open(target_log + ".ready", "w") as ready:
        ready.write("ready\n")
    conn, _ = server.accept()
    with conn:
        conn.sendall(b"SSH-2.0-MediumWssE2E\r\n")
        with open(target_log, "wb") as out:
            out.write(b"connected\n")
PY
target_pid=$!
for _ in $(seq 1 30); do
  if [ -f "$target_log.ready" ]; then
    break
  fi
  sleep 1
done
test -f "$target_log.ready"

OVERLAY_HOME="$client_home" cargo run -p medium-cli --bin medium -- \
  proxy ssh --device node-1 >"$proxy_stdout" 2>"$proxy_stderr" &
proxy_pid=$!

for _ in $(seq 1 30); do
  if grep -q "SSH-2.0-MediumWssE2E" "$proxy_stdout" 2>/dev/null; then
    break
  fi
  sleep 1
done
grep -q "SSH-2.0-MediumWssE2E" "$proxy_stdout"
grep -q "connected" "$target_log"
