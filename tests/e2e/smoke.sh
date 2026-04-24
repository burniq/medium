#!/usr/bin/env bash
set -euo pipefail

curl --fail http://127.0.0.1:8080/health >/dev/null
