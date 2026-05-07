# CPU Monitor

Real-time CPU monitor for Linux. Split into a small backend daemon that samples `/proc` + `lm-sensors` and exposes an HTTP/SSE API, plus a system-tray frontend that renders an icon and menu in the Ubuntu/GNOME panel.

![cpu monitor](cpu_monitor.gif)

## Architecture

```
+-------------------+       HTTP/SSE        +----------------------+
|   cpu-monitord    | <-------------------- |   cpu-monitor-tray   |
| (/proc + sensors) |   /v1/stream JSON     |  (ksni + tiny-skia)  |
+-------------------+                       +----------------------+
        ^                                            ^
        | /proc/stat, /proc/cpuinfo,                 | DBus (StatusNotifierItem)
        | /proc/loadavg, /sys hwmon                  v
        v                                     GNOME / KDE panel
   Linux kernel
```

Both Rust binaries live in a single Cargo workspace under `crates/`:

- `cpu-monitor-core` — shared `Snapshot` / `Cpu` / `Process` / `TempSensor` types serialised with `serde`.
- `cpu-monitord` — backend daemon. Reads `/proc/stat` (delta between samples for usage), `/proc/cpuinfo` (model, frequency), `/proc/loadavg`, `/proc/uptime`, and hwmon sysfs entries (temperatures via `lm-sensors`-style chip enumeration). Holds the latest snapshot in a `watch` channel, serves it over REST + Server-Sent Events. Defaults to `127.0.0.1:9124`.
- `cpu-monitor-tray` — Linux system-tray frontend. Subscribes to `/v1/stream`, composes an icon (CPU silhouette + temperature label + usage-percent donut) with `tiny-skia` + `freetype-rs`, writes it to `~/.cache/cpu-monitor/icons/` and publishes it as a StatusNotifierItem via `ksni`.

A native macOS frontend lives in `front-mac/` as an independent Swift Package (Swift + AppKit + CoreGraphics, zero third-party deps). It consumes the same `/v1/stream` endpoint and renders into the macOS menubar via `NSStatusItem`. See [`front-mac/README.md`](front-mac/README.md).

A declarative Home Assistant integration lives in `home-assistant/` (no custom component — just YAML packages on top of HA's built-in `rest`). 16 sensors per host (host metadata + usage / temperature / frequency / load 1m·5m·15m / uptime / top process; per-core usage and full sensor list as attributes). See [`home-assistant/README.md`](home-assistant/README.md).

## Performance

Measured on the same Ryzen 5 3600 against the original Python script, sampling every second:

| | RSS | CPU |
|---|---|---|
| `cpu_monitor.py` (matplotlib + PIL) | ~120 MB | ~25 % |
| `cpu-monitord` + `cpu-monitor-tray` | ~14 MB | ~0.4 % |

Most of the win comes from rendering the icon with `tiny-skia` instead of matplotlib + PIL writing PNGs each tick, and from reading `/proc` directly instead of spawning `top` / `sensors` for every sample.

## Requirements

- Linux with `/proc` + sysfs hwmon (any modern distro).
- `lm-sensors` for temperatures (`sudo apt install lm-sensors && sudo sensors-detect`). Without it, `temperature_c` is `null` but the rest works.
- DejaVu Sans Mono font (`apt install fonts-dejavu-core`) for the tray icon label.
- A desktop with StatusNotifierItem support. On Ubuntu/GNOME this means the **AppIndicator** extension (`gnome-shell-extension-appindicator`) must be enabled. KDE works out of the box.
- Rust toolchain (`stable`, ≥ 1.85). `rustup` will pick it up automatically from `rust-toolchain.toml`.
- `libfreetype6-dev` at build time (`libfreetype6` at runtime).

## Build

```bash
cargo build --release --workspace
```

Produces two binaries:

- `target/release/cpu-monitord`
- `target/release/cpu-monitor-tray`

## Run

In two terminals (or as services):

```bash
./target/release/cpu-monitord --bind 127.0.0.1 --port 9124
./target/release/cpu-monitor-tray --backend-url http://127.0.0.1:9124
```

Daemon flags:

| Flag | Default | Purpose |
|---|---|---|
| `--bind` | `127.0.0.1` | bind address (no auth, keep loopback unless behind SSH tunnel) |
| `--port` | `9124` | HTTP port |
| `--sample-interval-ms` | `1000` | sampler period |
| `--top-processes` | `5` | top-N CPU consumers per snapshot (`0` disables) |
| `--mock` | off | use `MockSource` instead of `/proc` (tests, CI) |
| `--log-level` | `info` | also via `RUST_LOG` |

Tray flags: `--backend-url`, `--icon-height`, `--dump-icon <path>` (write the next rendered icon to a PNG and exit; useful to inspect what the panel receives without fighting GNOME).

### Quick API smoke test

```bash
curl -s http://127.0.0.1:9124/v1/snapshot | jq
curl -N http://127.0.0.1:9124/v1/stream         # SSE: one event per second
```

## API

| Endpoint | Purpose |
|---|---|
| `GET /healthz` | liveness |
| `GET /v1/info` | backend / kernel / cpu_model metadata |
| `GET /v1/snapshot` | full latest snapshot |
| `GET /v1/cpu` | just the `cpu` object (usage, per-core, temps, freq, load, processes) |
| `GET /v1/cpu/temperatures` | sensor list |
| `GET /v1/cpu/processes` | top processes |
| `GET /v1/stream` | SSE — one snapshot per event |

## macOS frontend

```bash
cd front-mac
./scripts/build-app.sh
open "build/CPU Monitor.app" --args --backend-url http://127.0.0.1:9124
```

The daemon defaults to binding `127.0.0.1` (no auth). To consume metrics from a remote Linux box without exposing the API on the LAN, forward the port over SSH:

```bash
ssh -fN -L 9124:127.0.0.1:9124 <ubuntu-host>
open "build/CPU Monitor.app" --args --backend-url http://127.0.0.1:9124
```

The bundled LaunchAgents in `front-mac/scripts/` install both the tray autostart and a persistent SSH tunnel:

```bash
cd front-mac
./scripts/install-tunnel.sh                  # SSH tunnel as LaunchAgent (KeepAlive)
./scripts/install-launchagent.sh             # tray autostart on login
```

Logs land in `~/Library/Logs/cpu-monitor-tray.{out,err}.log` and `~/Library/Logs/cpu-monitor-tunnel.{out,err}.log`.

## Home Assistant integration

Surface CPU state as native HA sensors with no custom component — just a YAML package on top of `default_config`'s `rest` integration. Polls `/v1/snapshot` every 15 s and exposes 16 entities (host metadata + usage / temperature / frequency / load averages / uptime / top process, with `per_core_usage` and `temperatures` arrays as attributes on `sensor.cpu_usage`).

```bash
# On the raspberry running Home Assistant:
cd home-assistant/tunnel
./install.sh                                 # generates dedicated SSH key, installs systemd user unit
# (paste the printed pubkey line into the CPU host's ~/.ssh/authorized_keys)

# Copy the package and reload HA:
cp ../packages/cpu_monitor.yaml /config/packages/
# Add to /config/configuration.yaml (one-time per HA install):
#   homeassistant:
#     packages: !include_dir_named packages
docker restart homeassistant
```

The dedicated key is restricted with `restrict,port-forwarding,permitopen="127.0.0.1:9124"` so it can only forward to `cpu-monitord` and nothing else. Coexists in the same `/config/packages/` with packages from other monitors (gpu, ram, disk, …). See [`home-assistant/README.md`](home-assistant/README.md) for the full deploy guide and Lovelace dashboard.

## Roadmap

- v2.0: Linux tray frontend (released)
- v2.1: macOS menubar frontend (`front-mac/`, released)
- v2.2: Home Assistant integration (`home-assistant/`, released)
- v2.3: Auth token + LAN bind for remote consumption
- v2.4: Windows tray frontend

## Legacy Python script

The original `cpu_monitor.py` and its `add_to_startup.sh` / `cpu_monitor.sh` helpers live in `legacy/` for reference. They still work standalone (`python3 legacy/cpu_monitor.py`) but are no longer wired into autostart. They will be removed entirely after a soak period on the Rust release.

## Support

Consider giving a **☆ Star** to this repository, if you also want to invite me for a coffee, click on the following button:

[![BuyMeACoffee](https://img.shields.io/badge/Buy_Me_A_Coffee-support_my_work-FFDD00?style=for-the-badge&logo=buy-me-a-coffee&logoColor=white&labelColor=101010)](https://www.buymeacoffee.com/maximofn)
