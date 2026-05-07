# Home Assistant integration

Expone el estado de la CPU (`cpu-monitord`, puerto 9124) en Home Assistant
como sensores nativos. Sin custom component: solo configuración YAML usando
la integración `rest` que viene con `default_config`.

## Arquitectura

```
[ Ubuntu (wallabot) ]                 [ Raspberry (raspihome) ]
  cpu-monitord                            Home Assistant (Docker, host net)
  127.0.0.1:9124  ◄──── ssh -L ─────  127.0.0.1:9124
                                            │
                                            └─► sensor.rest (scan_interval=15s)
```

Túnel SSH **forward** desde raspihome: abre `127.0.0.1:9124` en la pi y reenvía
cada conexión al loopback de wallabot. Persistencia en raspihome (systemd user
unit con linger); en wallabot solo una clave pública restringida en
`~/.ssh/authorized_keys` (cero servicios nuevos).

HA corre en Docker con `--network host`, así que `127.0.0.1` desde dentro del
contenedor es el loopback de raspihome.

## Instalación

### 1) Túnel SSH desde raspihome

En raspihome:

```bash
# Pre-req: linger habilitado para tu usuario:
#   sudo loginctl enable-linger "$USER"
cd /ruta/al/repo/cpu_monitor/home-assistant/tunnel
./install.sh
```

`install.sh`:
1. Genera `~/.ssh/id_ed25519_cpu_tunnel` (sin passphrase, dedicada).
2. Te imprime la línea para añadir a `~/.ssh/authorized_keys` en wallabot:
   `restrict,port-forwarding,permitopen="127.0.0.1:9124" ssh-ed25519 AAA...`
3. Verifica que `ssh wallabot@wallabot` autentica.
4. Instala `cpu-monitor-ha-tunnel.service` como user systemd unit y lo arranca.

Verifica:

```bash
systemctl --user status cpu-monitor-ha-tunnel.service
curl -fsS http://127.0.0.1:9124/healthz       # raíz, no bajo /v1
curl -fsS http://127.0.0.1:9124/v1/info | jq
```

### 2) Paquete de Home Assistant

Habilita packages en `configuration.yaml` (una sola vez por instalación de HA;
si ya viene de otro monitor de la familia, salta este paso):

```yaml
homeassistant:
  packages: !include_dir_named packages
```

Copia el paquete:

```bash
ssh raspihome 'mkdir -p /home/raspihome/docker/homeassistant/packages'
scp packages/cpu_monitor.yaml raspihome:/home/raspihome/docker/homeassistant/packages/
```

Comprueba la config y recarga:

```bash
# Comprueba YAML
ssh raspihome 'docker exec homeassistant python -m homeassistant --script check_config -c /config'
# Recarga via Developer Tools → YAML → Reload REST entities
# o reinicia el contenedor:
ssh raspihome 'docker restart homeassistant'
```

Tras recargar, en HA aparecen las entidades:

```
sensor.cpu_monitor_host        sensor.cpu_usage              sensor.cpu_load_1m
sensor.cpu_monitor_kernel      sensor.cpu_temperature        sensor.cpu_load_5m
sensor.cpu_monitor_model       sensor.cpu_primary_sensor     sensor.cpu_load_15m
sensor.cpu_monitor_vendor      sensor.cpu_frequency          sensor.cpu_uptime
sensor.cpu_monitor_logical_cores                             sensor.cpu_top_process
sensor.cpu_monitor_physical_cores                            sensor.cpu_process_count
```

`sensor.cpu_usage` lleva `attributes.per_core_usage` (lista de N floats) y
`attributes.temperatures` (lista completa de sensores). `sensor.cpu_top_process`
lleva `attributes.processes` con la lista completa.

### 3) Dashboard (opcional)

En `lovelace/cpu_dashboard.yaml` hay una vista lista para pegar (Settings →
Dashboards → Edit → tres puntos → Raw configuration editor → añadir bajo
`views:`).

## Por qué REST y no SSE

`cpu-monitord` también expone Server-Sent Events. HA tiene integración
`rest` nativa (probada, declarativa, multi-sensor compartiendo una request)
pero su soporte para SSE requeriría un custom component. A 15 s de poll, el
overhead es mínimo y no se pierde nada útil.

## Troubleshooting

- **Sensores `unavailable`**: el túnel SSH se cayó o el daemon no responde.
  Desde raspihome: `curl http://127.0.0.1:9124/healthz` (raíz, no bajo /v1).
  Si timeout: `systemctl --user status cpu-monitor-ha-tunnel.service` y
  `journalctl --user -u cpu-monitor-ha-tunnel.service -n 50`.
- **`administratively prohibited` en los logs del túnel**: la línea en
  `authorized_keys` de wallabot omite `port-forwarding`. `restrict` solo
  no basta; debe ser `restrict,port-forwarding,permitopen="..."`.
- **Coexistencia con `gpu_monitor`**: ambos paquetes pueden vivir en
  `/config/packages/` simultáneamente — usan puertos distintos (9123 / 9124),
  túneles distintos y claves SSH dedicadas distintas.
