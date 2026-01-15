# joy2qlc ✅

Convert flight joystick/gamepad input into system keypresses or HTTP/REST calls.

---

## Overview

This project reads events from a joystick or gamepad and maps them to actions such as simulating system keypresses or sending REST calls to a remote service. It's intended for flight joysticks, HOTAS, and other controllers.

## Features

- Read joystick/gamepad input (feature: `joystick`) 🔧
- Simulate keypresses (feature: `simulate-keys`) ⌨️
- Send HTTP/REST events as a client (feature: `rest-client`) 🌐
- Run a small REST receiver (feature: `rest-server`) 🛰️

## Quick start

1. Install Rust and Cargo (https://rustup.rs)
2. Install platform prerequisites (macOS):
   - `brew install sdl2` (required by some joystick backends such as `gilrs`)
3. Build and run with desired features. Examples:

```bash
# Build and run with joystick reading + simulate keys
cargo run --features "joystick simulate-keys"

# Build with REST client and joystick
cargo run --features "joystick rest-client"

# Build the REST server
cargo run --features "rest-server"
```

## Notes / Configuration

- Feature flags keep dependencies optional so you only build what you need.
- On macOS you might need to grant accessibility permissions for key simulation (Enigo) to control the keyboard.
- Mapping joystick buttons/axes to actions is handled by `mappings.toml` so you can change mappings without recompiling or restarting the app (it is hot-reloaded at runtime).

### Config mappings (mappings.toml) 🔁

The config file is `mappings.toml` (TOML). Each mapping specifies an input and an action. Supported input types:

- `button` — joystick button names (use the Debug name from `gilrs`, e.g. `South`, `East`, `North`)
- `axis` — axis names (eg. `LeftStickX`, `RightTrigger`) — (axis support can be extended)

Supported action types:

- `key` — simulate a system keypress (requires `--features simulate-keys`). Example:

```toml
[[mappings]]
input = { type = "button", code = "South" }
action = { type = "key", key = "Space" }
```

- `rest` — make an HTTP call (requires `--features rest-client`). Example:

```toml
[[mappings]]
input = { type = "button", code = "East" }
action = { type = "rest", method = "POST", url = "http://localhost:8080/event", body = "{ \"event\": \"east_pressed\" }" }
```

- `exec` — run an external command (always available). Useful for invoking QLC+ commands or helper scripts. Example:

```toml
[[mappings]]
input = { type = "button", code = "North" }
action = { type = "exec", cmd = "/usr/local/bin/qlc_trigger.sh", args = ["scene1"] }
```

### Controlling QLC+

QLC+ can be controlled several ways; common approaches:

- Use QLC+'s web/HTTP or network API if available and expose endpoints your mapping can POST to (use the `rest` action).
- Create small helper scripts that call QLC+'s CLI, send OSC/UDP messages, or call QLC+'s remote API, and call them using `exec` from a mapping.

Add mappings to `mappings.toml` and the running app will pick them up automatically.

#### Example functions for a helper script (suggested)

If you provide a helper script (such as `scripts/qlc_trigger.sh`) you can accept a small set of function names and parameters. These are example function names you could support in your script:

- `StartShow` - (no params) start the current show
- `StopShow` - (no params) stop the show
- `RunScene` - (param: scene name) run a named scene
- `SetFixture` - (params: fixture_id, intensity) set intensity for a fixture
- `NextCue` / `PrevCue` - (no params) step cues
- `ToggleChannel` - (params: channel_id) toggle a channel on/off

These are examples — adapt the script to match your QLC+ integration method (OSC, UDP, REST, CLI). An example `mappings.toml` entry that runs a helper script:

```toml
[[mappings]]
input = { type = "button", code = "North" }
action = { type = "exec", cmd = "/usr/local/bin/qlc_trigger.sh", args = ["RunScene","Intro"] }
```

## Roadmap / Ideas

- Add a `mappings.toml` to configure mappings without recompiling.
- Add a Web UI to configure mappings live.
- Support multiple joystick backends (hidapi) for edge cases.

---

If you'd like, I can also add: a sample `mappings.toml`, a small CLI for loading configs, or an example mapping implementation. 🔧

---

## macOS Tray / Launch at Login (menu bar icon) 🍎

A native macOS menu bar (tray) is provided by the optional feature `tray`. It gives you quick access to:

- **Open Config** — open `mappings.toml` in Visual Studio Code
- **Restart** — spawn a new instance and exit the current one
- **Toggle Open at Login** — register/remove a LaunchAgent plist under `~/Library/LaunchAgents`
- **Quit** — stop the app

Enable and run with:

```bash
cargo run --features "joystick simulate-keys tray"
```

Notes & tips:

- The tray menu uses a LaunchAgent plist to implement "Open at Login". The plist is written to `~/Library/LaunchAgents` and points to the running executable.
- Key simulation still requires macOS Accessibility permission to work when the app is not focused — grant that in System Settings → Privacy & Security → Accessibility.
- If the `code` CLI isn't available, the tray will fallback to `open -a "Visual Studio Code" mappings.toml`.
