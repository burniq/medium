#!/usr/bin/env sh
set -eu

repo="${MEDIUM_REPO:-k1t-ops/medium}"
ref="${MEDIUM_REF:-main}"
prefix="${PREFIX:-/usr/local}"

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "medium installer: missing required command: $1" >&2
    exit 1
  fi
}

need cargo
need curl
need find
need install
need mktemp
need sed
need tar

workdir="$(mktemp -d "${TMPDIR:-/tmp}/medium-install.XXXXXX")"
cleanup() {
  rm -rf "$workdir"
}
trap cleanup EXIT INT TERM

archive="$workdir/source.tar.gz"
url="https://github.com/$repo/archive/$ref.tar.gz"

echo "medium installer: downloading $url"
curl -fsSL "$url" -o "$archive"
tar -xzf "$archive" -C "$workdir"

src_dir="$(find "$workdir" -mindepth 1 -maxdepth 1 -type d | sed -n '1p')"
if [ -z "$src_dir" ]; then
  echo "medium installer: failed to locate unpacked source directory" >&2
  exit 1
fi

echo "medium installer: building release binaries"
(
  cd "$src_dir"
  cargo build --release -p control-plane -p home-node -p linux-client
)

sudo_cmd=""
if [ "$(id -u)" -ne 0 ] && ! mkdir -p "$prefix/bin" 2>/dev/null; then
  sudo_cmd="sudo"
fi

echo "medium installer: installing into $prefix/bin"
$sudo_cmd mkdir -p "$prefix/bin"
for bin in medium control-plane; do
  $sudo_cmd install -m 0755 "$src_dir/target/release/$bin" "$prefix/bin/$bin"
done
$sudo_cmd install -m 0755 "$src_dir/target/release/home-node" "$prefix/bin/node-agent"

echo "medium installer: installed medium"
echo "next server step: sudo MEDIUM_CONTROL_PUBLIC_URL=http://192.0.2.10:8080 MEDIUM_NODE_PUBLIC_ADDR=192.0.2.10:17001 medium init-control"
