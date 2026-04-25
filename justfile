set shell := ["bash", "-cu"]

rust-test:
  cargo test --workspace

android-test:
  cd apps/android && gradle test

smoke:
  bash tests/e2e/smoke.sh

package:
  bash scripts/package.sh

e2e-package:
  bash tests/e2e/package_layout.sh

e2e-macos-ssh:
  bash tests/e2e/macos_ssh_sync.sh

e2e-init-control-join:
  bash tests/e2e/init_control_join.sh
