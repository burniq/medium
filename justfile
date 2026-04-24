set shell := ["bash", "-cu"]

db-up:
  podman compose up -d postgres

rust-test:
  cargo test --workspace

android-test:
  cd apps/android && gradle test

e2e-up:
  podman compose -f tests/e2e/docker-compose.yml up -d

smoke:
  bash tests/e2e/smoke.sh
