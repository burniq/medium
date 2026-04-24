#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
archive_dir="$(mktemp -d)"
trap 'rm -rf "$archive_dir"' EXIT

cd "$repo_root"

bash scripts/package.sh "$archive_dir"

test -x "$archive_dir/bin/medium"
test -x "$archive_dir/bin/control-plane"
test -x "$archive_dir/bin/home-node"
test -f "$archive_dir/systemd/medium-control-plane.service"
test -f "$archive_dir/systemd/medium-home-node.service"
test -f "$archive_dir/docs/linux/README.md"
test -f "$archive_dir/docs/linux/install-layout.txt"
test -f "$archive_dir/homebrew/medium.rb"
