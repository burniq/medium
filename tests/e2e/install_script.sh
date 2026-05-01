#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
workdir="$(mktemp -d)"
layout_dir="$workdir/package"
release_dir="$workdir/release"
prefix="$workdir/prefix"
bin_dir="$workdir/bin"
sudo_log="$workdir/sudo.log"
trap 'rm -rf "$workdir"' EXIT

mkdir -p "$bin_dir"

cd "$repo_root"

MEDIUM_VERSION=9.9.9 MEDIUM_TARGET=linux-x86_64 bash scripts/package.sh "$layout_dir" "$release_dir"

for cmd in chmod curl find id install mkdir mktemp rm sh tar uname sed tr; do
  command_path="$(command -v "$cmd")"
  ln -s "$command_path" "$bin_dir/$cmd"
done
cat >"$bin_dir/sudo" <<'SUDO'
#!/usr/bin/env sh
printf '%s\n' "$*" >>"$MEDIUM_TEST_SUDO_LOG"
if [ -n "${MEDIUM_TEST_SUDO_FIX_DIR:-}" ]; then
  chmod 0755 "$MEDIUM_TEST_SUDO_FIX_DIR"
fi
exec "$@"
SUDO
chmod +x "$bin_dir/sudo"

PATH="$bin_dir" \
  MEDIUM_VERSION=9.9.9 \
  MEDIUM_TARGET=linux-x86_64 \
  MEDIUM_RELEASE_BASE_URL="file://$release_dir" \
  PREFIX="$prefix" \
  sh scripts/install.sh

test -x "$prefix/bin/medium"
test -x "$prefix/bin/control-plane"
test -x "$prefix/bin/node-agent"
test -x "$prefix/bin/relay"

if PATH="$bin_dir" cargo --version >/dev/null 2>&1; then
  echo "test PATH unexpectedly contains cargo" >&2
  exit 1
fi

readonly_prefix="$workdir/readonly-prefix"
mkdir -p "$readonly_prefix/bin"
chmod 0555 "$readonly_prefix/bin"
trap 'chmod 0755 "$readonly_prefix/bin" 2>/dev/null || true; rm -rf "$workdir"' EXIT

PATH="$bin_dir" \
  MEDIUM_VERSION=9.9.9 \
  MEDIUM_TARGET=linux-x86_64 \
  MEDIUM_RELEASE_BASE_URL="file://$release_dir" \
  MEDIUM_TEST_SUDO_LOG="$sudo_log" \
  MEDIUM_TEST_SUDO_FIX_DIR="$readonly_prefix/bin" \
  PREFIX="$readonly_prefix" \
  sh scripts/install.sh

grep -Fq "mkdir -p $readonly_prefix/bin" "$sudo_log"
grep -Fq "install -m 0755" "$sudo_log"
test -x "$readonly_prefix/bin/medium"
