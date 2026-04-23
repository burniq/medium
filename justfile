set shell := ["bash", "-cu"]

db-up:
  podman compose up -d postgres

rust-test:
  cargo test --workspace

android-test:
  cd apps/android && gradle test
