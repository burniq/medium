# Medium Packaging And Bootstrap V1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a package-first bootstrap flow for `Medium` where a Linux server can run `medium init-control`, a Linux or macOS client can run `medium join <invite>`, and the client can immediately use `medium ssh sync` followed by `ssh <node_name>`.

**Architecture:** Extend the existing Rust CLI into a single `medium` entrypoint that owns install-time and bootstrap-time operations, keep `control-plane` and `home-node` as service binaries behind systemd on Linux, and persist node/bootstrap state in standard platform directories. Reuse the current SQLite registry, session-open path, and SSH sync flow, but add production-oriented config rendering, invite generation, service management, and deployment packaging assets.

**Tech Stack:** Rust, tokio, axum, sqlx/sqlite, systemd units, Homebrew formula assets, shell packaging helpers, existing SSH managed-config flow

---

### Task 1: Rename User-Facing Surface From `overlay` To `medium`

**Files:**
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/apps/linux-client/Cargo.toml`
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/apps/linux-client/src/cli.rs`
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/apps/linux-client/src/app.rs`
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/apps/linux-client/src/paths.rs`
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/apps/linux-client/src/ssh.rs`
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/apps/linux-client/tests/app.rs`
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/apps/linux-client/tests/ssh_sync.rs`
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/tests/e2e/macos_ssh_sync.sh`

- [ ] **Step 1: Write failing tests for the renamed CLI surface**

```rust
#[test]
fn summary_mentions_medium_name() {
    assert!(linux_client::app::summary().contains("Medium"));
}

#[test]
fn ssh_sync_uses_medium_managed_file_name() {
    let paths = linux_client::paths::AppPaths::from_home("/tmp/example-home");
    assert!(paths.overlay_ssh_config_path.ends_with("medium.conf"));
}
```

- [ ] **Step 2: Run targeted tests to verify the old `overlay` naming fails**

Run: `cargo test -p linux-client summary_mentions_medium_name ssh_sync_uses_medium_managed_file_name -- --exact`

Expected: FAIL because current strings and managed filename still reference `overlay`.

- [ ] **Step 3: Rename the user-facing binary, usage text, summary text, and managed SSH filename**

```toml
[[bin]]
name = "medium"
path = "src/main.rs"
```

```rust
const USAGE: &str = "usage: medium [init-control | join <invite> | devices | ssh sync [--write-main-config] | proxy ssh --device <name> | doctor | run --config <path> | info | normalize-label <value>]";
```

```rust
overlay_ssh_config_path: ssh_config_dir.join("medium.conf"),
```

- [ ] **Step 4: Update tests and e2e scripts to invoke `medium` instead of `overlay`**

```bash
cargo run -p linux-client --bin medium -- devices
```

```bash
managed_config="$HOME/.ssh/config.d/medium.conf"
```

- [ ] **Step 5: Run verification**

Run: `cargo test -p linux-client`

Expected: PASS with renamed CLI fixtures and SSH sync tests.

- [ ] **Step 6: Commit**

```bash
git add apps/linux-client tests/e2e/macos_ssh_sync.sh
git commit -m "refactor: rename cli surface to medium"
```

### Task 2: Add Platform-Aware App Paths And Persistent State Layout

**Files:**
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/apps/linux-client/src/paths.rs`
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/apps/linux-client/src/state.rs`
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/apps/linux-client/tests/app.rs`
- Create: `/Users/nikita/dev/homeworks/personal-overlay/apps/linux-client/tests/paths.rs`

- [ ] **Step 1: Write failing tests for Linux and macOS path layouts**

```rust
#[test]
fn linux_paths_use_xdg_layout() {
    let paths = AppPaths::for_linux_home("/home/tester");
    assert_eq!(paths.app_config_dir, PathBuf::from("/home/tester/.config/medium"));
    assert_eq!(paths.state_dir, PathBuf::from("/home/tester/.local/share/medium"));
}

#[test]
fn macos_paths_use_application_support() {
    let paths = AppPaths::for_macos_home("/Users/tester");
    assert_eq!(
        paths.app_config_dir,
        PathBuf::from("/Users/tester/Library/Application Support/Medium/config")
    );
}
```

- [ ] **Step 2: Run targeted tests to verify the current single-layout code fails**

Run: `cargo test -p linux-client --test paths`

Expected: FAIL because `AppPaths` currently only supports `~/.config/overlay`.

- [ ] **Step 3: Extend `AppPaths` with platform-aware constructors and explicit state/config dirs**

```rust
pub struct AppPaths {
    pub home_dir: PathBuf,
    pub app_config_dir: PathBuf,
    pub state_dir: PathBuf,
    pub state_path: PathBuf,
    // ...
}
```

```rust
pub fn from_env() -> anyhow::Result<Self> {
    if let Some(home) = std::env::var_os("MEDIUM_HOME") {
        return Ok(Self::for_current_platform(home));
    }

    let home = std::env::var_os("HOME").ok_or_else(|| anyhow::anyhow!("HOME is not set"))?;
    Ok(Self::for_current_platform(home))
}
```

- [ ] **Step 4: Update state loading/saving to create directories under the new state path**

```rust
std::fs::create_dir_all(&paths.app_config_dir)?;
std::fs::create_dir_all(&paths.state_dir)?;
```

- [ ] **Step 5: Run verification**

Run: `cargo test -p linux-client --test paths`

Expected: PASS with Linux/macOS path coverage.

- [ ] **Step 6: Commit**

```bash
git add apps/linux-client/src/paths.rs apps/linux-client/src/state.rs apps/linux-client/tests/paths.rs
git commit -m "feat: add medium platform path layout"
```

### Task 3: Introduce Versioned Invite Parsing And Join State

**Files:**
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/apps/linux-client/src/state.rs`
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/apps/linux-client/src/client_api.rs`
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/apps/linux-client/src/cli.rs`
- Create: `/Users/nikita/dev/homeworks/personal-overlay/apps/linux-client/src/invite.rs`
- Create: `/Users/nikita/dev/homeworks/personal-overlay/apps/linux-client/tests/invite.rs`

- [ ] **Step 1: Write failing tests for invite parsing**

```rust
#[test]
fn parses_versioned_join_invite() {
    let invite = parse_invite("medium://join?v=1&control=http://127.0.0.1:8080&token=abc123").unwrap();
    assert_eq!(invite.version, 1);
    assert_eq!(invite.control_url, "http://127.0.0.1:8080");
    assert_eq!(invite.bootstrap_token, "abc123");
}

#[test]
fn rejects_invite_with_unknown_scheme() {
    assert!(parse_invite("overlay://join?v=1").is_err());
}
```

- [ ] **Step 2: Run targeted tests to verify they fail**

Run: `cargo test -p linux-client --test invite`

Expected: FAIL because invite parsing does not exist yet.

- [ ] **Step 3: Add an `Invite` parser and store join metadata in `AppState`**

```rust
pub struct Invite {
    pub version: u32,
    pub control_url: String,
    pub bootstrap_token: String,
}
```

```rust
pub struct AppState {
    pub node_name: String,
    pub server_url: String,
    pub bootstrap_code: String,
    pub invite_version: u32,
}
```

- [ ] **Step 4: Add `join` command support while preserving `pair` as a dev-only alias**

```rust
Command::Join { invite } => {
    let invite = invite::parse_invite(&invite)?;
    let state = client_api::join(&invite).await?;
    state.save(&paths)?;
    Ok(Some(format!("joined {} via {}", state.node_name, state.server_url)))
}
```

- [ ] **Step 5: Run verification**

Run: `cargo test -p linux-client --test invite`

Expected: PASS with valid and invalid invite cases.

- [ ] **Step 6: Commit**

```bash
git add apps/linux-client/src/invite.rs apps/linux-client/src/state.rs apps/linux-client/src/cli.rs apps/linux-client/tests/invite.rs
git commit -m "feat: add versioned medium invite parsing"
```

### Task 4: Add Bootstrap Token Issuance To Control Plane

**Files:**
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/crates/overlay-protocol/src/messages.rs`
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/services/control-plane/src/routes/pairing.rs`
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/services/control-plane/src/routes/mod.rs`
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/services/control-plane/src/app.rs`
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/services/control-plane/tests/pairing.rs`

- [ ] **Step 1: Write failing test for invite/bootstrap token issuance**

```rust
#[tokio::test]
async fn bootstrap_route_returns_medium_join_invite() {
    let response = issue_bootstrap(/* ... */).await.unwrap();
    assert!(response.invite.starts_with("medium://join?v=1&control="));
}
```

- [ ] **Step 2: Run the pairing test to verify it fails**

Run: `cargo test -p control-plane --test pairing`

Expected: FAIL because the route currently only returns a bootstrap code shape.

- [ ] **Step 3: Extend the bootstrap response to return a versioned invite string**

```rust
pub struct BootstrapInviteResponse {
    pub invite: String,
    pub bootstrap_token: String,
    pub expires_at: Option<String>,
}
```

```rust
let invite = format!("medium://join?v=1&control={control_url}&token={bootstrap_token}");
```

- [ ] **Step 4: Update the route and tests to use the new response**

```rust
Json(BootstrapInviteResponse {
    invite,
    bootstrap_token,
    expires_at: None,
})
```

- [ ] **Step 5: Run verification**

Run: `cargo test -p control-plane --test pairing`

Expected: PASS with an invite string that encodes the control URL and token.

- [ ] **Step 6: Commit**

```bash
git add crates/overlay-protocol/src/messages.rs services/control-plane/src/routes/pairing.rs services/control-plane/tests/pairing.rs
git commit -m "feat: issue versioned medium invites"
```

### Task 5: Implement `medium init-control` With Idempotent Server Bootstrap

**Files:**
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/apps/linux-client/src/cli.rs`
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/apps/linux-client/src/client_api.rs`
- Create: `/Users/nikita/dev/homeworks/personal-overlay/apps/linux-client/src/install.rs`
- Create: `/Users/nikita/dev/homeworks/personal-overlay/apps/linux-client/tests/init_control.rs`

- [ ] **Step 1: Write failing tests for server bootstrap rendering**

```rust
#[test]
fn init_control_creates_expected_paths_and_files() {
    let temp = tempfile::tempdir().unwrap();
    let report = init_control_at(temp.path(), "127.0.0.1:8080").unwrap();
    assert!(report.control_config_path.exists());
    assert!(report.node_config_path.exists());
    assert!(report.database_path.exists());
}

#[test]
fn init_control_refuses_existing_install_without_reconfigure() {
    let temp = tempfile::tempdir().unwrap();
    init_control_at(temp.path(), "127.0.0.1:8080").unwrap();
    assert!(init_control_at(temp.path(), "127.0.0.1:8080").is_err());
}
```

- [ ] **Step 2: Run the new test file to verify failure**

Run: `cargo test -p linux-client --test init_control`

Expected: FAIL because no install/bootstrap module exists yet.

- [ ] **Step 3: Implement filesystem bootstrap for `/etc/medium` and `/var/lib/medium` layouts**

```rust
pub struct InitControlReport {
    pub control_config_path: PathBuf,
    pub node_config_path: PathBuf,
    pub database_path: PathBuf,
    pub invite: String,
}
```

```rust
write_control_config(&control_config_path, bind_addr, database_path, shared_secret)?;
write_home_node_config(&node_config_path, "node-home", "svc_home_ssh", "127.0.0.1:22")?;
```

- [ ] **Step 4: Wire `medium init-control` into the CLI with `--reconfigure` support**

```rust
Command::InitControl { reconfigure } => {
    let report = install::init_control(reconfigure).await?;
    Ok(Some(format!(
        "initialized Medium control at {} and generated invite {}",
        report.control_config_path.display(),
        report.invite
    )))
}
```

- [ ] **Step 5: Run verification**

Run: `cargo test -p linux-client --test init_control`

Expected: PASS with idempotent bootstrap semantics.

- [ ] **Step 6: Commit**

```bash
git add apps/linux-client/src/install.rs apps/linux-client/src/cli.rs apps/linux-client/tests/init_control.rs
git commit -m "feat: add medium init-control bootstrap"
```

### Task 6: Add Linux Service Units And Service Management Hooks

**Files:**
- Create: `/Users/nikita/dev/homeworks/personal-overlay/packaging/systemd/medium-control-plane.service`
- Create: `/Users/nikita/dev/homeworks/personal-overlay/packaging/systemd/medium-home-node.service`
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/apps/linux-client/src/install.rs`
- Create: `/Users/nikita/dev/homeworks/personal-overlay/apps/linux-client/tests/systemd.rs`

- [ ] **Step 1: Write failing tests for systemd unit rendering**

```rust
#[test]
fn renders_control_plane_unit_with_medium_paths() {
    let unit = render_control_plane_unit("/usr/bin/medium");
    assert!(unit.contains("Environment=MEDIUM_CONTROL_DATABASE_URL=sqlite:/var/lib/medium/control-plane.db"));
    assert!(unit.contains("ExecStart=/usr/bin/medium serve control-plane"));
}
```

- [ ] **Step 2: Run the systemd test file to verify failure**

Run: `cargo test -p linux-client --test systemd`

Expected: FAIL because no unit rendering exists yet.

- [ ] **Step 3: Create checked-in unit templates and render helpers**

```ini
[Service]
ExecStart=/usr/bin/medium serve control-plane --config /etc/medium/control.toml
Environment=MEDIUM_CONTROL_DATABASE_URL=sqlite:///var/lib/medium/control-plane.db
WorkingDirectory=/var/lib/medium
```

```ini
[Service]
ExecStart=/usr/bin/medium serve home-node --config /etc/medium/node.toml
Environment=MEDIUM_CONTROL_URL=http://127.0.0.1:8080
```

- [ ] **Step 4: Add service-management hooks for `systemctl enable --now` in `init-control`**

```rust
run_command("systemctl", &["daemon-reload"])?;
run_command("systemctl", &["enable", "--now", "medium-control-plane.service"])?;
run_command("systemctl", &["enable", "--now", "medium-home-node.service"])?;
```

- [ ] **Step 5: Run verification**

Run: `cargo test -p linux-client --test systemd`

Expected: PASS with unit rendering and command hook coverage.

- [ ] **Step 6: Commit**

```bash
git add packaging/systemd apps/linux-client/src/install.rs apps/linux-client/tests/systemd.rs
git commit -m "feat: add medium systemd service assets"
```

### Task 7: Add `medium doctor` Diagnostics

**Files:**
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/apps/linux-client/src/cli.rs`
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/apps/linux-client/src/install.rs`
- Create: `/Users/nikita/dev/homeworks/personal-overlay/apps/linux-client/src/doctor.rs`
- Create: `/Users/nikita/dev/homeworks/personal-overlay/apps/linux-client/tests/doctor.rs`

- [ ] **Step 1: Write failing tests for doctor output**

```rust
#[test]
fn doctor_reports_missing_join_state() {
    let report = doctor::inspect(&AppPaths::for_linux_home("/tmp/missing")).unwrap();
    assert!(report.lines.iter().any(|line| line.contains("join-state: missing")));
}
```

- [ ] **Step 2: Run the doctor test file to verify failure**

Run: `cargo test -p linux-client --test doctor`

Expected: FAIL because doctor inspection does not exist yet.

- [ ] **Step 3: Implement path, config, DB, SSH, and service checks**

```rust
pub struct DoctorReport {
    pub lines: Vec<String>,
}
```

```rust
lines.push(format!("config-dir: {}", status(paths.app_config_dir.exists())));
lines.push(format!("state-file: {}", status(paths.state_path.exists())));
lines.push(format!("ssh-include: {}", status(paths.overlay_ssh_config_path.exists())));
```

- [ ] **Step 4: Wire the `doctor` command into the CLI**

```rust
Command::Doctor => {
    let report = doctor::inspect(&paths)?;
    Ok(Some(report.render()))
}
```

- [ ] **Step 5: Run verification**

Run: `cargo test -p linux-client --test doctor`

Expected: PASS with readable diagnostics for missing and present state.

- [ ] **Step 6: Commit**

```bash
git add apps/linux-client/src/doctor.rs apps/linux-client/src/cli.rs apps/linux-client/tests/doctor.rs
git commit -m "feat: add medium doctor diagnostics"
```

### Task 8: Add Packaging Assets For Homebrew And Linux Archive Distribution

**Files:**
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/scripts/package.sh`
- Create: `/Users/nikita/dev/homeworks/personal-overlay/packaging/homebrew/medium.rb`
- Create: `/Users/nikita/dev/homeworks/personal-overlay/packaging/linux/README.md`
- Create: `/Users/nikita/dev/homeworks/personal-overlay/packaging/linux/install-layout.txt`
- Create: `/Users/nikita/dev/homeworks/personal-overlay/tests/e2e/package_layout.sh`

- [ ] **Step 1: Write failing packaging layout test**

```bash
#!/usr/bin/env bash
set -euo pipefail

archive_dir="$(mktemp -d)"
bash scripts/package.sh "$archive_dir"
test -f "$archive_dir/bin/medium"
test -f "$archive_dir/systemd/medium-control-plane.service"
test -f "$archive_dir/systemd/medium-home-node.service"
```

- [ ] **Step 2: Run the packaging layout test to verify failure**

Run: `bash tests/e2e/package_layout.sh`

Expected: FAIL because packaging currently only builds release binaries.

- [ ] **Step 3: Extend packaging script to emit a structured artifact directory**

```bash
out_dir="${1:-dist/package}"
mkdir -p "$out_dir/bin" "$out_dir/systemd" "$out_dir/examples"
cp target/release/medium "$out_dir/bin/medium"
cp packaging/systemd/*.service "$out_dir/systemd/"
cp services/home-node/config.example.toml "$out_dir/examples/node.toml"
```

- [ ] **Step 4: Add Homebrew formula template and Linux package layout docs**

```ruby
class Medium < Formula
  desc "Personal service-access overlay"
  homepage "https://example.invalid/medium"
  url "https://example.invalid/medium.tar.gz"
  version "0.1.0"

  def install
    bin.install "medium"
  end
end
```

- [ ] **Step 5: Run verification**

Run: `bash tests/e2e/package_layout.sh`

Expected: PASS with a distributable artifact tree.

- [ ] **Step 6: Commit**

```bash
git add scripts/package.sh packaging tests/e2e/package_layout.sh
git commit -m "feat: add medium packaging assets"
```

### Task 9: Cover End-To-End Bootstrap Flow With Linux Server + macOS/Linux Client Simulation

**Files:**
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/tests/e2e/macos_ssh_sync.sh`
- Create: `/Users/nikita/dev/homeworks/personal-overlay/tests/e2e/init_control_join.sh`
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/justfile`

- [ ] **Step 1: Write the end-to-end script for `init-control -> join -> ssh sync -> ssh`**

```bash
#!/usr/bin/env bash
set -euo pipefail

workdir="$(mktemp -d)"
server_root="$workdir/server"
client_home="$workdir/client-home"

cargo run -p linux-client --bin medium -- init-control --root "$server_root" >"$workdir/init.log"
invite="$(tail -n1 "$workdir/init.log")"
MEDIUM_HOME="$client_home" cargo run -p linux-client --bin medium -- join "$invite"
MEDIUM_HOME="$client_home" cargo run -p linux-client --bin medium -- ssh sync --write-main-config
```

- [ ] **Step 2: Run the new e2e script to verify failure**

Run: `bash tests/e2e/init_control_join.sh`

Expected: FAIL because `init-control`, `join`, and service bootstrap are not all implemented yet.

- [ ] **Step 3: Update the script to start the generated server services or equivalent local processes**

```bash
MEDIUM_ROOT="$server_root" cargo run -p linux-client --bin medium -- serve control-plane --config "$server_root/etc/medium/control.toml" &
MEDIUM_ROOT="$server_root" cargo run -p linux-client --bin medium -- serve home-node --config "$server_root/etc/medium/node.toml" &
```

- [ ] **Step 4: Add `just` targets for the new e2e path**

```make
e2e-bootstrap:
  bash tests/e2e/init_control_join.sh
```

- [ ] **Step 5: Run full verification**

Run: `cargo test --workspace`

Expected: PASS

Run: `bash tests/e2e/init_control_join.sh`

Expected: PASS with a generated invite, successful join, generated `medium.conf`, and a working `ssh <node_name>` path through `ProxyCommand`.

Run: `bash scripts/package.sh`

Expected: PASS with distributable assets.

- [ ] **Step 6: Commit**

```bash
git add tests/e2e justfile
git commit -m "test: add medium bootstrap e2e coverage"
```

### Task 10: Add Launch Instructions And Deployment Docs

**Files:**
- Create: `/Users/nikita/dev/homeworks/personal-overlay/README.md`
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/packaging/linux/README.md`
- Modify: `/Users/nikita/dev/homeworks/personal-overlay/docs/adr/0001-v1-stack.md`

- [ ] **Step 1: Draft the server installation instructions**

```md
## Linux server

1. Install the `medium` package.
2. Run `sudo medium init-control`.
3. Copy the invite printed by the command.
```

- [ ] **Step 2: Draft the client installation instructions**

```md
## macOS or Linux client

1. Install `medium`.
2. Run `medium join '<invite>'`.
3. Run `medium ssh sync`.
4. Connect with `ssh <node_name>`.
```

- [ ] **Step 3: Document diagnostics and recovery**

```md
Use `medium doctor` to inspect config, join state, and SSH integration.
If the managed SSH config needs to be rebuilt, run `medium ssh sync` again.
```

- [ ] **Step 4: Run doc sanity verification**

Run: `rg -n "overlay|OVERLAY_" README.md packaging docs/adr/0001-v1-stack.md`

Expected: only intentional low-level compatibility references remain.

- [ ] **Step 5: Commit**

```bash
git add README.md packaging/linux/README.md docs/adr/0001-v1-stack.md
git commit -m "docs: add medium install instructions"
```

---

## Self-Review

### Spec Coverage

- Package-first rollout: covered by Tasks 5, 6, 8, 10.
- `medium init-control`: covered by Task 5.
- `medium join <invite>`: covered by Task 3.
- `medium doctor`: covered by Task 7.
- Linux server + Linux/macOS clients: covered by Tasks 2, 5, 6, 8, 9.
- Service model and paths: covered by Tasks 2, 5, 6.
- Invite format: covered by Tasks 3 and 4.
- Acceptance flow `init-control -> join -> ssh sync -> ssh`: covered by Task 9.

### Placeholder Scan

- No `TBD`, `TODO`, or unresolved placeholders remain in the plan body.
- Every implementation task names concrete files and verification commands.

### Type Consistency

- `medium init-control`, `medium join`, and `medium doctor` are introduced consistently as CLI commands.
- `AppPaths`, `Invite`, `InitControlReport`, and `DoctorReport` are referenced consistently across tasks.
