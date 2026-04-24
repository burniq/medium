#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:-$repo_root/dist/package}"

cd "$repo_root"

cargo build --release -p control-plane -p home-node -p linux-client

mkdir -p \
  "$out_dir/bin" \
  "$out_dir/systemd" \
  "$out_dir/docs/linux" \
  "$out_dir/homebrew"

install -m 0755 target/release/medium "$out_dir/bin/medium"
install -m 0755 target/release/control-plane "$out_dir/bin/control-plane"
install -m 0755 target/release/home-node "$out_dir/bin/home-node"

install -m 0644 packaging/systemd/medium-control-plane.service \
  "$out_dir/systemd/medium-control-plane.service"
install -m 0644 packaging/systemd/medium-home-node.service \
  "$out_dir/systemd/medium-home-node.service"

install -m 0644 packaging/linux/README.md "$out_dir/docs/linux/README.md"
install -m 0644 packaging/linux/install-layout.txt \
  "$out_dir/docs/linux/install-layout.txt"
install -m 0644 packaging/homebrew/medium.rb "$out_dir/homebrew/medium.rb"
