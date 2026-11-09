#!/usr/bin/env bash
# Verificación de la instalación distribuida (criterio de cierre de TT-01).
#
# Sobre un Ubuntu 24.04 con el paquete Debian instalado, en una sesión gráfica con
# el llavero desbloqueado:
#   1. registra una clave de prueba en Secret Service con los atributos de openTUM;
#   2. ejecuta la aplicación instalada durante un ciclo con xAI habilitado, usando
#      directorios XDG temporales para no tocar la configuración del usuario;
#   3. comprueba los modos de los directorios (0700) y archivos (0600) creados;
#   4. busca el valor de la clave en los archivos de configuración, caché y estado,
#      en el registro de la aplicación y en el registro de la sesión (journalctl);
#   5. elimina la clave de prueba.
# Uso: scripts/verificar-instalacion.sh   (termina con código 0 si todo se cumple)
set -uo pipefail

fallas=0
ok()    { printf '  [OK]    %s\n' "$*"; }
falla() { printf '  [FALLA] %s\n' "$*"; fallas=$((fallas + 1)); }

command -v opentum >/dev/null || { echo "openTUM no está instalado (sudo apt install ./openTUM_*.deb)"; exit 2; }
command -v secret-tool >/dev/null || { echo "Falta secret-tool (sudo apt install libsecret-tools)"; exit 2; }
if pgrep -x opentum >/dev/null; then echo "Cierre openTUM antes de verificar (menú del indicador > Salir)."; exit 2; fi

echo "openTUM: $(opentum --version)  ·  paquete: $(dpkg-query -W -f='${Package} ${Version}' open-tum 2>/dev/null || echo 'no registrado en dpkg')"
base=$(mktemp -d /tmp/opentum-verif.XXXXXX)
export XDG_CONFIG_HOME="$base/config" XDG_CACHE_HOME="$base/cache" XDG_STATE_HOME="$base/state"
team="verificacion-$$"
clave="opentum-prueba-$(head -c 18 /dev/urandom | base64 | tr -dc 'A-Za-z0-9')"
inicio=$(date '+%Y-%m-%d %H:%M:%S')

echo "1. Registro de una clave de prueba en el llavero"
if printf '%s' "$clave" | secret-tool store --label="openTUM: clave de prueba" application opentum proveedor xai teamId "$team"; then
  ok "clave guardada en Secret Service"
else
  falla "no se pudo guardar la clave (¿llavero bloqueado?)"; exit 1
fi

mkdir -p "$XDG_CONFIG_HOME/opentum"
cat > "$XDG_CONFIG_HOME/opentum/config.json" <<EOF
{"version":2,"intervaloSeg":120,"umbrales":[80,95],"proveedores":[
 {"id":"xai","habilitado":true,"mecanismo":"API","credencialEn":"secret_service","teamId":"$team"}]}
EOF
chmod 600 "$XDG_CONFIG_HOME/opentum/config.json"

echo "2. Ejecución de un ciclo de la aplicación instalada"
opentum >"$base/app.log" 2>&1 &
pid=$!
for _ in $(seq 1 30); do [ -f "$XDG_CACHE_HOME/opentum/lecturas.json" ] && break; sleep 1; done
sleep 2; kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
if [ -f "$XDG_CACHE_HOME/opentum/lecturas.json" ]; then
  estado=$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['xai']['estado'])" "$XDG_CACHE_HOME/opentum/lecturas.json" 2>/dev/null)
  ok "ciclo completo; estado de la fuente xai: ${estado:-desconocido} (se espera un estado de error porque la clave de prueba no es real)"
else
  falla "la aplicación no escribió lecturas.json"
fi

echo "3. Permisos de directorios y archivos"
for d in "$XDG_CONFIG_HOME/opentum" "$XDG_CACHE_HOME/opentum" "$XDG_STATE_HOME/opentum"; do
  [ -d "$d" ] || continue
  m=$(stat -c %a "$d"); [ "$m" = "700" ] && ok "$m  ${d#$base/}" || falla "$m  ${d#$base/} (se esperaba 700)"
done
for f in "$XDG_CONFIG_HOME/opentum/config.json" "$XDG_CACHE_HOME/opentum/lecturas.json" "$XDG_STATE_HOME/opentum/alertas.json"; do
  [ -f "$f" ] || continue
  m=$(stat -c %a "$f"); [ "$m" = "600" ] && ok "$m  ${f#$base/}" || falla "$m  ${f#$base/} (se esperaba 600)"
done

echo "4. Búsqueda de la clave fuera del llavero"
if grep -rqF "$clave" "$base"; then falla "la clave aparece en: $(grep -rlF "$clave" "$base" | tr '\n' ' ')"; else ok "no aparece en config, caché, estado ni en el registro de la aplicación"; fi
if command -v journalctl >/dev/null && journalctl --user --since "$inicio" -q >/dev/null 2>&1; then
  if journalctl --user --since "$inicio" -q 2>/dev/null | grep -qF "$clave"; then falla "la clave aparece en journalctl --user"; else ok "no aparece en journalctl --user"; fi
else
  echo "  [--]    journalctl --user no disponible en este entorno"
fi

echo "5. Limpieza"
secret-tool clear application opentum teamId "$team" && ok "clave de prueba eliminada del llavero"
rm -rf "$base"

echo
if [ "$fallas" -eq 0 ]; then echo "RESULTADO: verificación superada"; else echo "RESULTADO: $fallas falla(s)"; fi
exit "$fallas"
