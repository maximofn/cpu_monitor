# front-mac — frontend macOS para cpu-monitord

Status bar app nativo (Swift + AppKit) que consume el HTTP+SSE de `cpu-monitord`
y muestra un icono dinámico en la barra superior del Mac, con menú desplegable
con detalle de uso, temperaturas, frecuencia, load average, per-core y top
procesos.

Réplica funcional del tray Linux (`crates/cpu-monitor-tray`). Mismo schema
(`/v1/snapshot`, `/v1/stream`), mismos colores, mismos umbrales. Sin Dock icon
ni ventanas — `LSUIElement = true`.

## Requisitos

- macOS 13 (Ventura) o superior.
- Swift 5.9+ (Xcode 15+ o `swift` en línea de comandos via toolchain).
- Un `cpu-monitord` corriendo y accesible por red (puerto default 9124).

## Build

```bash
cd front-mac
swift build -c release
```

El binario sale en `.build/release/CPUMonitorTray`. Para iterar:

```bash
.build/release/CPUMonitorTray --backend-url http://127.0.0.1:9124
```

Para uso real, empaquetar en `.app` (sin Dock icon):

```bash
./scripts/build-app.sh
open "build/CPU Monitor.app" --args --backend-url http://192.168.1.50:9124
```

## CLI

| Flag                     | Default                       | Descripción                                |
| ------------------------ | ----------------------------- | ------------------------------------------ |
| `--backend-url <URL>`    | `http://127.0.0.1:9124`       | Base del API (env: `CPU_MONITOR_TRAY_URL`) |
| `--icon-height <PT>`     | `22`                          | Altura lógica del icono en la barra        |
| `--log-level <LEVEL>`    | `info`                        | trace/debug/info/warn/error (OSLog)        |
| `--dump-icon <PATH>`     | —                             | Pinta el snapshot actual a PNG y sale      |
| `--version`              | —                             | Versión                                    |
| `-h`, `--help`           | —                             | Ayuda                                      |

## Autostart en login

Hay dos LaunchAgents que conviene instalar juntos: uno mantiene el túnel SSH
contra la máquina remota donde corre el daemon, otro lanza el tray.

```bash
./scripts/install-tunnel.sh                 # SSH tunnel — KeepAlive, reconecta solo
./scripts/install-launchagent.sh            # Tray app — arranca al login

# desinstalar
./scripts/install-tunnel.sh uninstall
./scripts/install-launchagent.sh uninstall
```

### Tray (`com.maximofn.cpu-monitor-tray`)

`scripts/com.maximofn.cpu-monitor-tray.plist` lanza el binario del bundle
directamente (no `open`) — `KeepAlive=false` para que no relance si el usuario
cierra desde el menú, `RunAtLoad=true` para arrancar al login,
`ProcessType=Interactive` para evitar throttling de tareas de fondo. Logs en
`~/Library/Logs/cpu-monitor-tray.{out,err}.log`.

Tras re-empaquetar con `build-app.sh` no hay que reinstalar el agent (la ruta
absoluta del bundle no cambia), pero sí conviene
`launchctl kickstart -k gui/$(id -u)/com.maximofn.cpu-monitor-tray` para
que la app reinicie con el binario nuevo.

### Túnel SSH (`com.maximofn.cpu-monitor-tunnel`)

`cpu-monitord` bindea `127.0.0.1` por defecto (sin auth), así que para
consumirlo desde el Mac hay que tunelear `9124 → wallabot:9124` por SSH.
`scripts/com.maximofn.cpu-monitor-tunnel.plist` lo gobierna con
`KeepAlive=true` + `ServerAliveInterval=30` + `ExitOnForwardFailure=yes`
(reconecta cada `ThrottleInterval=10s` si la conexión se cae). Equivale a
HTTPS+auth gratis — cifrado y autenticación los pone SSH con tus claves.

El plist está pre-configurado para `wallabot` (ver `~/.ssh/config`). Si tu host
es otro, edítalo antes de instalar:

```bash
sed -i '' 's|<string>wallabot</string>|<string>tu-host</string>|' \
    scripts/com.maximofn.cpu-monitor-tunnel.plist
./scripts/install-tunnel.sh
```

Logs en `~/Library/Logs/cpu-monitor-tunnel.{out,err}.log`. Si la clave SSH
tiene passphrase y no está en el Keychain, aquí verás `Permission denied
(publickey)` al login — desbloquéala una vez con `ssh-add --apple-use-keychain`
para que macOS la suelte transparente a los LaunchAgents.

### Iteración manual (sin agents)

```bash
ssh -fN -L 9124:127.0.0.1:9124 <ubuntu-host>
open "build/CPU Monitor.app" --args --backend-url http://127.0.0.1:9124
```

## Schema y compatibilidad

`Models.swift` replica `crates/cpu-monitor-core/src/model.rs`. Si añades campos
al `Snapshot`/`Cpu`/`Process` en Rust, **replícalos aquí** o el JSON decode
ignorará los nuevos campos en silencio. La API está versionada por path
(`/v1/...`).

## Diferencias con el tray Linux

- Renderer Core Graphics + Core Text (no tiny-skia + freetype). Geometría
  (donut, gaps, layout) y colores portados 1:1.
- Fuente del sistema: SF Pro con `monospacedDigitSystemFont` (los dígitos no
  saltan al cambiar de 99% a 100%, como hacen el reloj y la batería).
- Sin archivos PNG en `~/.cache` — `NSStatusItem` acepta `NSImage` en memoria.
- Texto blanco hardcoded: `effectiveAppearance` del status button discrepa del
  color visible de la barra. Blanco combina con cualquier wallpaper razonable.

## Verificación rápida

```bash
# 1. Backend mock en otra máquina (o Linux local):
cargo run -p cpu-monitord --release -- --mock --bind 0.0.0.0 --port 9124

# 2. Frontend Mac apuntando al mock:
.build/release/CPUMonitorTray --backend-url http://<host>:9124

# 3. Volcar el icono a un PNG sin tocar la status bar:
.build/release/CPUMonitorTray --backend-url http://<host>:9124 \
    --dump-icon /tmp/cpu-icon.png
```
