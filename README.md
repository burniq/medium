# Medium

Medium is a personal service-access overlay for reaching your own machines from anywhere.

The first practical target is simple: install Medium on a Linux home server and on a client machine, join the client to the server, then use regular SSH:

```sh
ssh node-home
```

Medium is currently an early MVP. The implemented path focuses on a control plane, a headless home-node agent, SQLite-backed registry state, and generated SSH config.

## Install

From the GitHub project:

```sh
curl -fsSL https://raw.githubusercontent.com/k1t-ops/medium/main/scripts/install.sh | sh
```

For a fork or private repo:

```sh
curl -fsSL https://raw.githubusercontent.com/k1t-ops/medium/main/scripts/install.sh | MEDIUM_REPO=k1t-ops/medium sh
```

The installer builds from source with Cargo and installs these binaries into `/usr/local/bin` by default:

- `medium`
- `control-plane`
- `home-node`

Use a different install prefix if needed:

```sh
curl -fsSL https://raw.githubusercontent.com/k1t-ops/medium/main/scripts/install.sh | PREFIX="$HOME/.local" sh
```

## Server Bootstrap

Run this on the Linux host that will act as your first Medium server:

```sh
sudo MEDIUM_CONTROL_PUBLIC_URL="https://control.example.com" \
  MEDIUM_HOME_NODE_BIND_ADDR="home.example.com:17001" \
  medium init-control
```

`medium init-control` creates the server config under `/etc/medium`, state under `/var/lib/medium`, renders systemd units, and prints a `medium://join?...` invite.

After bootstrap, check status:

```sh
medium doctor
```

## Client Join

Run this on a client machine:

```sh
medium join 'medium://join?v=1&control=https://control.example.com&token=...'
medium devices
medium ssh sync
ssh node-home
```

`medium ssh sync` writes a Medium-managed SSH include file and keeps the main SSH config limited to a single `Include`.

## Development

Common local commands:

```sh
just rust-test
just e2e-init-control-join
just e2e-package
just package
```

The packaged Linux layout is documented in `packaging/linux/README.md`.
