# Medium Linux Archive Layout

`scripts/package.sh` builds a distributable archive tree for the current branch.

The archive contains:

- `bin/medium` for the end-user CLI
- `bin/control-plane` for the Linux server control service
- `bin/home-node` for the Linux server home-node service
- `systemd/medium-control-plane.service` and `systemd/medium-home-node.service` template files
- `docs/linux/install-layout.txt` with the expected install paths
- `homebrew/medium.rb` as the macOS/Homebrew formula template shipped with the release assets

The Linux server install layout matches the current runtime assumptions:

- config files under `/etc/medium`
- state under `/var/lib/medium`
- generated systemd units under `/etc/systemd/system`
- binaries under `/usr/bin`

The archive is intentionally passive. Installing the files does not bootstrap a network or start services until the operator runs `medium init-control`.

The `systemd/*.service` files in the archive are source templates, not final install-ready units. The current workflow is:

1. Place the binaries on the target host.
2. Keep the packaged `systemd/*.service` files as reference assets in the release bundle.
3. Run `medium init-control` on the server host.
4. Let `medium init-control` render the final units into `/etc/systemd/system` with the real binary, state, and config paths for that machine.
