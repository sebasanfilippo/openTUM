# openTUM

openTUM (Open Token Usage Monitor) muestra en el área de indicadores de Ubuntu 24.04 el consumo de
Claude Code, OpenAI Codex, xAI (Grok) y Google Antigravity: el uso de cada ventana de cuota en su unidad
propia, el momento del reinicio, el origen del dato y su antigüedad. Avisa una sola vez por umbral y por
ventana, y guarda la única clave que administra (la de gestión de xAI) en el llavero de la sesión.

Es el prototipo del trabajo final de graduación «Monitor de Consumo de Servicios de Inteligencia
Artificial para Linux» (Licenciatura en Informática, Universidad Siglo 21).

## Instalación

Descargue el paquete de la sección *Releases* y ejecute:

```sh
sudo apt install ./openTUM_0.1.0_amd64.deb
```

o use el archivo AppImage (`chmod +x openTUM_0.1.0_amd64.AppImage && ./openTUM_0.1.0_amd64.AppImage`).
La guía completa está en [docs/instalacion.md](docs/instalacion.md).

## Fuentes de datos por proveedor

| Proveedor | Mecanismo principal | Alternativa |
|---|---|---|
| Claude Code | Línea de estado documentada (`opentum --statusline`) | Endpoint de uso OAuth con la sesión de `claude` |
| Codex | Panel `/status` leído en una pseudoterminal (contingencia R02) | Endpoint de uso de ChatGPT con la sesión de `codex` |
| Antigravity | `agy -p /usage` y `agy -p /credits` | Resumen de cuotas de Cloud Code con la sesión guardada por Antigravity |
| xAI | Management API con clave de gestión y Team ID | — |

Las sesiones de las CLI se leen sin copiarlas ni renovarlas.

## Modo de demostración

`OPENTUM_SIMULADO=1 opentum --abrir panel` reemplaza los conectores por lecturas simuladas y
anonimizadas (`src-tauri/tests/fixtures`). Sirve para revisar la interfaz y las alertas sin cuentas
reales; no demuestra el acceso a las fuentes.

## Desarrollo

Ver [docs/entorno.md](docs/entorno.md). Resumen:

```sh
scripts/setup-dev.sh          # dependencias de Ubuntu 24.04, Rust y tauri-cli
cd src-tauri && cargo test    # pruebas unitarias
cargo tauri build             # paquete .deb y AppImage
scripts/verificar-instalacion.sh   # verificación de la instalación distribuida
```

## Licencia

GPL-3.0-or-later. Ver [LICENSE](LICENSE).
