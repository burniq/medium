#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
archive_dir="$(mktemp -d)"
default_dist_dir="$repo_root/dist"
default_out_dir="$default_dist_dir/package"
release_dir="$(mktemp -d)"
stale_file="$default_out_dir/stale.txt"
trap 'rm -rf "$archive_dir" "$default_dist_dir" "$release_dir"' EXIT

cd "$repo_root"

MEDIUM_VERSION=0.0.1 MEDIUM_TARGET=linux-x86_64 bash scripts/package.sh "$archive_dir" "$release_dir"

test -x "$archive_dir/bin/medium"
test -x "$archive_dir/bin/control-plane"
test -x "$archive_dir/bin/node-agent"
test -x "$archive_dir/bin/relay"
test -f "$archive_dir/systemd/medium-control-plane.service"
test -f "$archive_dir/systemd/medium-node-agent.service"
test -f "$archive_dir/systemd/medium-relay.service"
test -f "$archive_dir/docs/linux/README.md"
test -f "$archive_dir/docs/linux/install-layout.txt"
test -f "$archive_dir/homebrew/medium.rb"
grep -Fq 'bin.install "bin/medium"' "$archive_dir/homebrew/medium.rb"
test -f "$release_dir/medium-0.0.1-linux-x86_64.tar.gz"
tar -tzf "$release_dir/medium-0.0.1-linux-x86_64.tar.gz" >"$release_dir/archive-contents.txt"
grep -Fqx "bin/medium" "$release_dir/archive-contents.txt"

mkdir -p "$default_out_dir"
printf 'stale\n' >"$stale_file"

bash scripts/package.sh

test -d "$default_out_dir"
test ! -e "$stale_file"
test -f "$repo_root/dist/medium-0.0.1-$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m).tar.gz"
