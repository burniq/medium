#!/usr/bin/env bash
set -euo pipefail

cargo build --release -p control-plane -p home-node -p relay -p linux-client
