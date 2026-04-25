# Secure Node Bootstrap And Neutral Node Terminology

## Goal

Medium must bootstrap new nodes safely even when the control endpoint is reached over plain HTTP by IP address. Domain names and HTTPS are useful deployment options, but they must not be required for the initial trust establishment.

This slice also removes `home` from public terminology. Medium should speak about nodes and node agents, not home nodes.

## Terminology

- `node-agent`: the headless process that registers a node and proxies local services.
- `control-plane`: the registry/session coordinator.
- `node`: any device or machine participating in the overlay.
- `control identity`: a long-lived public/private signing keypair owned by the control-plane install.
- `join handshake`: the encrypted bootstrap exchange used by a new client to join the network.

Legacy internal crate names may remain temporarily if changing them would create a large mechanical rename. Public docs, commands, env vars, service names, package layout, and generated configs should use `node-agent` and `node`.

## Security Model

Plain HTTP is treated as an untrusted byte pipe. Medium must not send bearer secrets, shared secrets, session credentials, or device credentials in cleartext during join.

The trust anchor is pinned from the invite:

```text
medium://join?v=1&control=http://192.168.1.10:8080&control_key=<public-key>
```

The invite may use `http://`, `https://`, an IP address, or a DNS name. The `control_key` is mandatory for secure bootstrap.

The join flow:

1. `medium init-control` generates or loads a long-lived control identity keypair.
2. The printed invite includes the control public key.
3. `medium join` parses and stores the expected control public key.
4. The client starts a join handshake by sending an ephemeral public key.
5. The control-plane responds with its ephemeral public key and a signature made by the pinned control identity.
6. The client verifies the signature before deriving any shared secret.
7. Both sides derive an AEAD key with ECDH.
8. Join credentials and follow-up secrets are sent only inside encrypted/authenticated payloads.

For this implementation slice, an explicit server approval step can be added later. The immediate goal is to remove cleartext bearer-secret bootstrap from the invite and establish pinned control identity primitives; the encrypted join payload is the next security slice.

## Addressing Without A Domain

Domain usage is optional.

Control-plane addressing:

- `MEDIUM_CONTROL_PUBLIC_URL` remains the explicit public URL override.
- If it is not set, `MEDIUM_CONTROL_BIND_ADDR` may be used when its host is directly reachable and not `0.0.0.0`.
- If the bind host is `0.0.0.0`, Medium must require an explicit public URL because clients cannot connect to `0.0.0.0`.

Node-agent addressing:

- `MEDIUM_NODE_LISTEN_ADDR` controls where the node-agent listens. Default: `0.0.0.0:17001`.
- `MEDIUM_NODE_PUBLIC_ADDR` controls what address clients receive for direct TCP connection.
- If `MEDIUM_NODE_PUBLIC_ADDR` is not set and `MEDIUM_NODE_LISTEN_ADDR` uses a concrete host, the listen address may be used as the public address.
- If the listen host is `0.0.0.0`, Medium must require `MEDIUM_NODE_PUBLIC_ADDR`.

## Compatibility

Legacy env vars can be accepted as fallbacks during the transition:

- `OVERLAY_CONTROL_URL`
- `MEDIUM_HOME_NODE_BIND_ADDR`
- `OVERLAY_HOME_NODE_BIND_ADDR`

New docs and generated output must prefer:

- `MEDIUM_CONTROL_PUBLIC_URL`
- `MEDIUM_NODE_LISTEN_ADDR`
- `MEDIUM_NODE_PUBLIC_ADDR`

## Non-Goals

- Full crate rename from `home-node` to `node-agent`.
- Full PAKE/OPAQUE implementation.
- Full approval UI.
- Full encrypted join payload and end-to-end encrypted session transport beyond the pinned control identity primitive.

Those are future slices. This slice creates the foundation and removes insecure bearer-secret bootstrap from the public design.

## Acceptance Criteria

- README and packaging docs no longer describe `home-node` as public terminology.
- Bootstrap examples do not require a domain.
- `init-control` can produce an invite with an `http://IP:PORT` control URL plus `control_key`.
- `join` requires and stores `control_key`.
- Tests cover invite parsing, domainless init-control, node address selection, and legacy env fallback.
- Existing e2e packaging and Rust tests pass.
