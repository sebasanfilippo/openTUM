# Instructivo de instalación y puesta en marcha de openTUM

Versión 0.1.0. Probado con cuentas reales en Ubuntu 26.04 LTS con GNOME 50 (Wayland). La versión de
referencia del proyecto es Ubuntu 24.04 LTS con GNOME.

## 1. Requisitos

- Ubuntu 24.04 LTS o posterior con GNOME.
- Extensión Ubuntu AppIndicators habilitada (viene activa en Ubuntu).
- Llavero de GNOME (gnome-keyring), desbloqueado al iniciar sesión.
- Para cada proveedor que se quiera monitorear:
  - Claude Code: la herramienta `claude` con sesión iniciada en una cuenta Pro o Max (una clave de
    API no tiene cuota de plan).
  - Codex: la herramienta `codex` con sesión iniciada con una cuenta de ChatGPT (no con clave de API).
  - Antigravity: la herramienta `agy` con sesión iniciada.
  - xAI: una clave de gestión (Management API) y el Team ID.

Antes de instalar, compruebe en una terminal que `claude` y `codex` abren sin pedir inicio de sesión.

## 2. Instalación

La carpeta de la demo incluye dos formatos. Use uno solo.

Paquete Debian (recomendado):

```sh
sudo apt install ./openTUM_0.1.0_amd64.deb
```

AppImage (sin instalación):

```sh
chmod +x openTUM_0.1.0_amd64.AppImage
./openTUM_0.1.0_amd64.AppImage
```

Se ejecuta una sola instancia por sesión: si openTUM ya está abierto, volver a abrirlo solo muestra
su ventana. Para pasar de un formato al otro, cierre antes la instancia abierta con «Salir» en el
menú del ícono.

## 3. Primer inicio

Abra openTUM desde el menú de aplicaciones (o ejecute el AppImage). Aparece un ícono en la barra
superior y, como no hay proveedores habilitados, se abre la ventana de Configuración.

1. Marque los proveedores que usa y pulse «Guardar».
2. xAI: complete el Team ID y la clave de gestión y pulse «Registrar clave». La clave se verifica
   con la Management API y se guarda en el llavero; no se escribe en archivos.
3. Ajuste, si lo desea, los umbrales (80 % y 95 % por omisión), el saldo mínimo, el horario de
   silencio y el intervalo de actualización (2 a 60 minutos, 5 por omisión) y pulse «Guardar».

## 4. Fuentes de datos de Claude Code y Codex

### Claude Code

openTUM usa dos fuentes:

- Principal: la línea de estado de Claude Code, que se actualiza mientras hay una sesión de
  `claude` abierta. Requiere configurarla (ver abajo).
- Alternativa: si no hay datos recientes de la línea de estado, openTUM consulta el uso con la
  sesión que `claude` guarda en `~/.claude/.credentials.json`. Esa sesión vence; cuando ocurre, el
  estado del proveedor lo indica y basta con abrir `claude` para renovarla.

Para configurar la línea de estado, edite `~/.claude/settings.json` y agregue la clave `statusLine`
dentro del objeto existente (si el archivo no existe, créelo con este contenido):

```json
{
  "statusLine": {
    "type": "command",
    "command": "opentum --statusline"
  }
}
```

Si ya hay otras claves, agregue `"statusLine": {...}` junto a ellas, separada por una coma.

Con el AppImage no existe el comando `opentum`: escriba en `command` la ruta completa del archivo
seguida de `--statusline`, por ejemplo:

```text
/home/usuario/Descargas/openTUM_0.1.0_amd64.AppImage --statusline
```

Luego abra `claude` en cualquier carpeta y envíe un mensaje: los datos de cuota llegan con la
primera respuesta. La línea de estado de Claude Code muestra «5 h … · 7 d …».

### Codex

No requiere configuración. openTUM ejecuta `codex` en segundo plano, abre el panel `/status` y lee
el porcentaje restante de las ventanas de 5 horas y semanal. Si no encuentra el programa (por
ejemplo, instalado en una carpeta que no está en el PATH), usa la sesión guardada en
`~/.codex/auth.json` para consultar el mismo uso.

## 5. Validar los valores con una cuenta real

1. En el menú del ícono, pulse «Actualizar ahora».
2. Abra «Abrir panel…» y luego «Ver detalle» del proveedor. El detalle muestra la hora del dato,
   la fuente usada (por ejemplo, «Fuente: línea de estado de Claude Code» o «Fuente: panel /status
   de Codex») y, por métrica, la unidad, si el valor es consumido o disponible, el reinicio y el
   origen.
3. Compare con la herramienta del proveedor, en una terminal:
   - Claude Code: dentro de `claude`, el comando `/usage`.
   - Codex: dentro de `codex`, el comando `/status`.
4. Deben coincidir el porcentaje (con una diferencia de un punto por redondeo o por el tiempo entre
   ambas lecturas) y la hora de reinicio. Tenga en cuenta el sentido: Claude Code informa el
   porcentaje consumido y Codex el disponible; openTUM lo indica como «disponible» cuando
   corresponde.

## 6. Uso

- El menú del ícono muestra una línea por proveedor con su consumo principal, el reinicio y un punto
  de color (verde, ámbar desde el primer umbral, rojo desde el segundo, gris sin datos). El ícono toma
  el color del proveedor más comprometido.
- «Abrir panel…» muestra todas las métricas; la línea de un proveedor abre su detalle.
- «Actualizar ahora» consulta todas las fuentes sin esperar el intervalo.
- Ante una falla, el estado de la fuente se muestra junto al último valor válido y su hora; nunca como
  consumo cero.

## 7. Modo de demostración

Sin cuentas reales, con el paquete Debian instalado:

```sh
OPENTUM_SIMULADO=1 opentum --abrir panel
```

Con el AppImage:

```sh
OPENTUM_SIMULADO=1 ./openTUM_0.1.0_amd64.AppImage --abrir panel
```

Usa lecturas simuladas y anonimizadas para revisar la interfaz y las alertas. Cierre antes la
instancia normal.

## 8. Verificación de la instalación

Requiere el paquete Debian instalado y openTUM cerrado. El script está en la carpeta de la demo y en
el repositorio (`scripts/verificar-instalacion.sh`):

```sh
sudo apt install libsecret-tools
bash verificar-instalacion.sh
```

Comprueba los permisos de los archivos y que una clave de prueba no aparezca fuera del llavero.
Termina con «[OK]» en cada punto si todo se cumple.

## 9. Desinstalación y eliminación de datos

Configuración > «Eliminar datos de openTUM…» borra las claves guardadas por openTUM y sus archivos,
sin modificar las sesiones de las CLI. Luego: `sudo apt remove open-tum` (o borre el AppImage).
Si configuró la línea de estado, quite la clave `statusLine` de `~/.claude/settings.json`.
