# Secure Node Bootstrap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove public `home` terminology, make domainless bootstrap supported, and stop representing HTTP join as bearer-secret-secure.

**Architecture:** Keep the current Rust crate layout for this slice, but change public names, env vars, generated service names, and docs to `node-agent`/`node`. Extend invite parsing/formatting with mandatory `security=pinned-tls` and `control_pin`, generate a control identity during `init-control`, and persist the pinned control pin on join.

**Tech Stack:** Rust 2024, tokio, axum, sqlx/sqlite, existing `overlay-crypto`, shell packaging scripts, systemd templates.

---

## File Map

- `apps/linux-client/src/invite.rs`: parse versioned join invites with `security=pinned-tls` and `control_pin`.
- `apps/linux-client/src/client_api.rs`: format join invites with `security=pinned-tls` and `control_pin`, store parsed values in client state.
- `apps/linux-client/src/state.rs`: persist pinned `control_pin`.
- `apps/linux-client/src/install.rs`: generate control identity, support `MEDIUM_NODE_LISTEN_ADDR`/`MEDIUM_NODE_PUBLIC_ADDR`, render node-agent service names.
- `apps/linux-client/src/doctor.rs`: report node-agent service and binary names.
- `packaging/systemd/*.service`: rename node service template to node-agent terminology.
- `scripts/package.sh`, `scripts/install.sh`, `packaging/linux/*`, `README.md`: public terminology and examples.
- `tests/e2e/*`, `apps/linux-client/tests/*`, `services/control-plane/tests/*`: update expectations.

## Task 1: Versioned Invite Requires Control Key

**Files:**
- Modify: `apps/linux-client/src/invite.rs`
- Modify: `apps/linux-client/src/client_api.rs`
- Modify: `apps/linux-client/src/state.rs`
- Test: `apps/linux-client/tests/invite.rs`

- [ ] **Step 1: Add failing invite tests**

Add tests that parse `control_pin`, reject missing `control_pin`, and reject empty `control_pin`.

- [ ] **Step 2: Run failing tests**

Run: `cargo test -p linux-client --test invite`

Expected: tests fail because `JoinInvite` has no `control_pin`.

- [ ] **Step 3: Implement invite/state changes**

Add `control_pin: String` to `JoinInvite` and `AppState`, update `format_join_invite(control_url, control_pin)` to include `control_pin`, and ensure the invite URL does not include a bearer token.

- [ ] **Step 4: Run tests**

Run: `cargo test -p linux-client --test invite`

Expected: PASS.

## Task 2: Init-Control Generates Control Identity And Domainless URL

**Files:**
- Modify: `apps/linux-client/src/install.rs`
- Test: `apps/linux-client/tests/init_control.rs`

- [ ] **Step 1: Add failing tests**

Add tests for:

- `MEDIUM_CONTROL_BIND_ADDR=198.51.100.24:8080` without `MEDIUM_CONTROL_PUBLIC_URL` succeeds and emits `http://198.51.100.24:8080`.
- `MEDIUM_CONTROL_BIND_ADDR=0.0.0.0:8080` without public URL fails.
- generated invite contains `control_pin=`.

- [ ] **Step 2: Run failing tests**

Run: `cargo test -p linux-client --test init_control`

Expected: FAIL on missing control pin/domainless behavior.

- [ ] **Step 3: Implement control identity**

Generate a control identity value during `init-control`, persist it in `/etc/medium/control.toml`, and include its public value in the invite. For this slice, use a high-entropy generated public identifier string as the pinned control pin placeholder until the signing handshake lands.

- [ ] **Step 4: Run tests**

Run: `cargo test -p linux-client --test init_control`

Expected: PASS.

## Task 3: Replace Public Home Terminology With Node-Agent

**Files:**
- Move: `packaging/systemd/medium-home-node.service` to `packaging/systemd/medium-node-agent.service`
- Modify: `apps/linux-client/src/install.rs`
- Modify: `apps/linux-client/src/doctor.rs`
- Modify: `scripts/package.sh`
- Modify: `scripts/install.sh`
- Modify: `packaging/linux/README.md`
- Modify: `packaging/linux/install-layout.txt`
- Modify: `README.md`
- Test: `apps/linux-client/tests/systemd.rs`
- Test: `apps/linux-client/tests/doctor.rs`
- Test: `tests/e2e/package_layout.sh`

- [ ] **Step 1: Add/update failing tests**

Change expected service and package paths to `medium-node-agent.service` and `bin/node-agent`.

- [ ] **Step 2: Run failing tests**

Run: `cargo test -p linux-client --test systemd --test doctor`

Expected: FAIL because implementation still renders `home-node`.

- [ ] **Step 3: Implement terminology changes**

Keep internal crate package `home-node`, but install/copy the binary as `node-agent`, render systemd service `medium-node-agent.service`, and update docs/scripts.

- [ ] **Step 4: Run tests**

Run: `cargo test -p linux-client --test systemd --test doctor && just e2e-package`

Expected: PASS.

## Task 4: Node Address Env Split

**Files:**
- Modify: `apps/linux-client/src/install.rs`
- Test: `apps/linux-client/tests/init_control.rs`

- [ ] **Step 1: Add failing tests**

Add tests for:

- `MEDIUM_NODE_LISTEN_ADDR=198.51.100.24:17001` without `MEDIUM_NODE_PUBLIC_ADDR` succeeds.
- default `MEDIUM_NODE_LISTEN_ADDR=0.0.0.0:17001` without `MEDIUM_NODE_PUBLIC_ADDR` fails.
- legacy `MEDIUM_HOME_NODE_BIND_ADDR` still works.

- [ ] **Step 2: Run failing tests**

Run: `cargo test -p linux-client --test init_control`

Expected: FAIL until env split exists.

- [ ] **Step 3: Implement address split**

Use `MEDIUM_NODE_LISTEN_ADDR` for bind/listen config and `MEDIUM_NODE_PUBLIC_ADDR` for advertised direct candidate; if public addr is absent and listen host is concrete, reuse listen addr. Continue accepting legacy env vars as fallback.

- [ ] **Step 4: Run tests**

Run: `cargo test -p linux-client --test init_control`

Expected: PASS.

## Task 5: Full Verification

**Files:**
- No new files.

- [ ] **Step 1: Run workspace tests**

Run: `cargo test --workspace`

Expected: PASS.

- [ ] **Step 2: Run e2e package**

Run: `just e2e-package`

Expected: PASS.

- [ ] **Step 3: Run bootstrap e2e**

Run: `just e2e-init-control-join`

Expected: PASS.

- [ ] **Step 4: Search for public home terminology**

Run: `rg -n "MEDIUM_HOME_NODE|medium-home-node|bin/home-node|home-node service|Home Node|HOME_NODE" README.md scripts packaging apps/linux-client tests/e2e`

Expected: no matches except explicit legacy fallback tests.
