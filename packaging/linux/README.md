# Medium Linux Archive Layout

`scripts/package.sh` builds a distributable archive tree and a release tarball
for the current branch.

The archive contains:

- `bin/medium` for the end-user CLI
- `bin/control-plane` for the Linux server control service
- `bin/node-agent` for the Linux server node-agent service
- `bin/relay` for TCP relay fallback
- `systemd/medium-control-plane.service`, `systemd/medium-node-agent.service`, and `systemd/medium-relay.service` template files
- `docs/linux/install-layout.txt` with the expected install paths
- `homebrew/medium.rb` as the macOS/Homebrew formula template shipped with the release assets

The release tarball is written as:

```text
medium-${MEDIUM_VERSION:-0.0.1}-${MEDIUM_TARGET:-<detected-target>}.tar.gz
```

`scripts/install.sh` installs from this tarball by default and does not require
Cargo on the target host.

GitHub Actions publishes release tarballs automatically for Linux x86_64,
Linux aarch64, macOS arm64, and macOS x86_64 when a `v*.*.*` tag is pushed.

The Linux server install layout matches the current runtime assumptions:

- config files under `/etc/medium`
- state under `/var/lib/medium`
- generated systemd units under `/etc/systemd/system`
- binaries under `/usr/bin`

The archive is intentionally passive. Installing the files does not bootstrap a network or start services until the operator runs `medium init-control` or `medium init-node`.

The `systemd/*.service` files in the archive are source templates, not final install-ready units. The current workflow is:

1. Place the binaries on the target host.
2. Keep the packaged `systemd/*.service` files as reference assets in the release bundle.
3. Run `medium init-control` on the control host.
4. Run `medium init-node '<node invite>'` on each service node.
5. Let the init commands render the final units into `/etc/systemd/system` with the real binary, state, and config paths for each machine.
