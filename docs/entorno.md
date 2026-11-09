# Entorno de desarrollo y reconstrucción en otro equipo

Este documento permite dejar operativo un equipo nuevo a partir del repositorio y de los respaldos
(política de respaldo del trabajo final, capítulo Seguridad).

## Sistema y paquetes

- Ubuntu 24.04 LTS con GNOME (Wayland o X11).
- Paquetes de compilación (los instala `scripts/setup-dev.sh`):
  `build-essential curl pkg-config libssl-dev file libwebkit2gtk-4.1-dev libayatana-appindicator3-dev
  librsvg2-dev libxdo-dev libsecret-1-dev gnome-keyring libsecret-tools`.
- Extensión del indicador: Ubuntu 24.04 la trae instalada (Ubuntu AppIndicators,
  paquete `gnome-shell-extension-appindicator`). Si está deshabilitada, se activa desde la aplicación
  Extensiones de GNOME.

## Versiones fijadas

| Componente | Dónde se fija | Versión |
|---|---|---|
| Rust | `rust-toolchain.toml` | 1.97.0 |
| Dependencias de Rust | `src-tauri/Cargo.lock` | según el archivo |
| Tauri CLI | `scripts/setup-dev.sh` | 2.x (probado con 2.12.4) |

La interfaz web no usa Node.js ni un empaquetador: los archivos de `ui/` se incluyen tal cual.

## Versiones de las CLI con las que se probaron los conectores

Se registra la salida de `--version` de cada herramienta en la fecha de la prueba con cuentas reales. La prueba del
08/11/2026 se hizo en Ubuntu 26.04.1 LTS con GNOME Shell 50.1 (Wayland).

`agy` usa un almacenamiento de credenciales en archivo cuando detecta una sesión SSH; las consultas de Antigravity
deben hacerse desde la sesión gráfica.

| Herramienta | Comando | Versión probada | Fecha |
|---|---|---|---|
| Claude Code | `claude --version` | 2.1.319 | 08/11/2026 |
| Codex | `codex --version` | codex-cli 0.167.0 | 08/11/2026 |
| Antigravity | `agy --version` | 1.5.3 | 08/11/2026 |
| xAI Management API | — | v1 | pendiente (sin clave real) |

## Restitución de la configuración del usuario

1. Instalar el paquete (`sudo apt install ./openTUM_*.deb`).
2. Copiar desde el respaldo `~/.config/opentum/config.json` y `~/.local/state/opentum/alertas.json`.
   `~/.cache/opentum/lecturas.json` no se restaura: la aplicación lo reconstruye en la siguiente consulta.
3. Iniciar sesión otra vez en cada CLI (`claude`, `codex`, `agy`).
4. Generar una clave de gestión nueva en la consola de xAI, revocar la anterior y registrarla en
   Configuración. La clave no forma parte de los respaldos.
5. Ejecutar `scripts/verificar-instalacion.sh`.
