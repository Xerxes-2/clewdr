# ClewdR

<p align="center">
  <img src="./assets/clewdr-logo.svg" alt="ClewdR" height="60">
</p>

ClewdR is a Rust proxy for Claude — both Claude.ai and Claude Code — serving native Claude and OpenAI-compatible endpoints from a single binary.

It runs as one static executable on Linux, macOS, Windows and Android, with a Docker image available, and typically uses `<10 MB` RAM, starts in `<1 s`, and weighs `~15 MB`.

## Quick Start

1. Download the latest release for your platform from GitHub.
   Linux/macOS example:
   ```bash
   curl -L -o clewdr.tar.gz https://github.com/Xerxes-2/clewdr/releases/latest/download/clewdr-linux-x64.tar.gz
   tar -xzf clewdr.tar.gz && cd clewdr-linux-x64
   chmod +x clewdr
   ```
2. Run the binary:
   ```bash
   ./clewdr
   ```
3. Open `http://127.0.0.1:8484` and enter the admin password shown in the console (or container logs if using Docker).

## Adding Cookies

ClewdR needs at least one Claude.ai cookie before it can serve requests.

1. Export your Claude.ai cookies (e.g. via browser devtools).
2. Paste them into the `Claude` tab as `cookie: value` pairs and save. ClewdR tracks their status automatically.
3. Optionally set an outbound proxy or fingerprint overrides if Claude blocks your region.

The remaining tabs cover everything else. `Dashboard` shows health, connected clients, and rate-limit status; `Settings` rotates the admin password, sets upstream proxies, and reloads config without restarting.

If you forget the password, delete `clewdr.toml` and start the binary again. Docker users can mount a persistent folder for that file.

## Connecting a Client

All paths below are relative to `http://127.0.0.1:8484`.

| | Claude.ai | Claude Code |
|---|---|---|
| Native Claude | `/v1/messages` | `/code/v1/messages` |
| OpenAI-compatible | `/v1/chat/completions` | `/code/v1/chat/completions` |
| Model list | `/v1/models` | `/code/v1/models` |
| Token counting | — | `/code/v1/messages/count_tokens` |

Streaming works on every endpoint. The API password is printed to the console on startup, separately from the admin password.

SillyTavern:

```json
{
  "api_url": "http://127.0.0.1:8484/v1/chat/completions",
  "api_key": "password-from-console",
  "model": "claude-3-sonnet-20240229"
}
```

Any other OpenAI-compatible client — Continue, Cursor and the rest — works the same way: point its API base at `http://127.0.0.1:8484/v1/` and use the API password as the key.

## Building from Source

The frontend compiles to WebAssembly into `static/`, which the server then serves. That directory is gitignored, so it has to be built first, or `cargo run` starts a server with no UI. `cargo xtask` handles the ordering:

```bash
cargo xtask check     # report on the required toolchain pieces
cargo xtask build     # release build of the frontend and the server
cargo xtask dev       # both, with frontend hot reload, on :3000
cargo xtask lint      # clippy over every valid feature combination
cargo xtask fmt       # format (always via nightly)
cargo xtask ci        # everything CI runs
```

Building the frontend needs `rustup target add wasm32-unknown-unknown` and `cargo binstall trunk`. Running `cargo xtask` itself needs nothing.

Two gotchas if you bypass xtask. Formatting must go through **nightly**, because `.rustfmt.toml` uses nightly-only options that stable silently ignores.

`--all-features` also fails: `embed-resource`/`external-resource` and `portable`/`xdg` are mutually exclusive pairs enforced in `build.rs`.

## Resources

- Wiki: <https://github.com/Xerxes-2/clewdr/wiki>

## Thanks

- [wreq](https://github.com/0x676e67/wreq) for the fingerprinting library.
- [Clewd](https://github.com/teralomaniac/clewd) for many upstream ideas.
- [Clove](https://github.com/mirrorange/clove) for Claude Code helpers.
