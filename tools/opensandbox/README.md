# OpenSandbox (Local) — Quick Setup for CrateBay Integration

CrateBay’s AI sandbox direction treats OpenSandbox as an **optional local runtime** that CrateBay can manage and call via API.

This folder contains a small, source-controlled setup scaffold.

## What you get here

- `sandbox.example.toml`: a conservative example config (Docker runtime, host/bridge networking notes).

## Local install/run (recommended for dev)

1) Install `opensandbox-server`

- With `uv` (recommended by OpenSandbox docs):

```bash
uv pip install opensandbox-server
```

- Or with `pip`:

```bash
python3 -m pip install --user opensandbox-server
```

2) Create a config file

```bash
cp ./sandbox.example.toml ~/.sandbox.toml
```

3) Start the server

```bash
opensandbox-server --config ~/.sandbox.toml
```

The API docs are usually exposed at:

- `http://localhost:8080/docs`

## Notes

- **Docker socket access**: OpenSandbox needs to talk to Docker to create sandboxes.
- **Networking mode**:
  - `host`: simplest; higher performance; typically single active sandbox at a time.
  - `bridge`: isolated networking; requires correct `host_ip` for endpoint resolution in some deployments.

