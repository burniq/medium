# Medium Linux Archive Layout

`scripts/package.sh` builds a distributable archive tree for the current branch.

The archive contains:

- `bin/medium` for the end-user CLI
- `bin/control-plane` for the Linux server control service
- `bin/home-node` for the Linux server home-node service
- `systemd/medium-control-plane.service` and `systemd/medium-home-node.service`
- `docs/linux/install-layout.txt` with the expected install paths
- `homebrew/medium.rb` as the macOS/Homebrew formula template shipped with the release assets

The Linux server install layout matches the current runtime assumptions:

- config files under `/etc/medium`
- state under `/var/lib/medium`
- systemd units under `/etc/systemd/system`
- binaries under `/usr/bin`

The archive is intentionally passive. Installing the files does not bootstrap a network or start services until the operator runs `medium init-control`.
