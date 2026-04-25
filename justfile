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

e2e-macos-ssh:
  bash tests/e2e/macos_ssh_sync.sh

e2e-init-control-join:
  bash tests/e2e/init_control_join.sh
