#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
archive_dir="$(mktemp -d)"
default_out_dir="$repo_root/dist/package"
stale_file="$default_out_dir/stale.txt"
trap 'rm -rf "$archive_dir" "$default_out_dir"' EXIT

cd "$repo_root"

bash scripts/package.sh "$archive_dir"

test -x "$archive_dir/bin/medium"
test -x "$archive_dir/bin/control-plane"
test -x "$archive_dir/bin/node-agent"
test -f "$archive_dir/systemd/medium-control-plane.service"
test -f "$archive_dir/systemd/medium-node-agent.service"
test -f "$archive_dir/docs/linux/README.md"
test -f "$archive_dir/docs/linux/install-layout.txt"
test -f "$archive_dir/homebrew/medium.rb"
grep -Fq 'bin.install "bin/medium"' "$archive_dir/homebrew/medium.rb"

mkdir -p "$default_out_dir"
printf 'stale\n' >"$stale_file"

bash scripts/package.sh

test -d "$default_out_dir"
test ! -e "$stale_file"
