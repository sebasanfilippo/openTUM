#!/usr/bin/env bash
# Prueba de openTUM en una estación Ubuntu 24.04 con GNOME (registro del Anexo 4).
#
# Ejecutar en una terminal DENTRO de la sesión gráfica de la VM (no por ssh), con el
# paquete .deb y verificar-instalacion.sh en la misma carpeta que este script:
#     bash prueba-vm.sh
# Deja todo en ~/opentum-resultados-<fecha>/. No copia tokens ni claves: guarda
# versiones, estados, porcentajes y la salida de texto de `agy`
# revisar antes de enviarla!
set -uo pipefail
dir=$(cd "$(dirname "$0")" && pwd)
out="$HOME/opentum-resultados-$(date +%Y%m%d-%H%M)"
mkdir -p "$out"
exec > >(tee "$out/registro.txt") 2>&1

seccion() { printf '\n===== %s =====\n' "$*"; }

seccion "1. Sistema"
date -R
grep PRETTY_NAME /etc/os-release
echo "Sesión: ${XDG_SESSION_TYPE:-desconocida} · escritorio: ${XDG_CURRENT_DESKTOP:-desconocido}"
gnome-shell --version 2>/dev/null || echo "gnome-shell no encontrado"
if command -v gnome-extensions >/dev/null; then
  gnome-extensions list --enabled 2>/dev/null | grep -i appindicator || echo "Extensión AppIndicator NO habilitada"
fi
dpkg-query -W -f='${Package} ${Version}\n' gnome-keyring libwebkit2gtk-4.1-0 libayatana-appindicator3-1 gnome-shell-extension-appindicator 2>/dev/null

seccion "2. Instalación del paquete"
deb=$(ls "$dir"/openTUM_*_amd64.deb 2>/dev/null | head -1)
[ -n "$deb" ] || { echo "No encuentro el .deb junto al script"; exit 1; }
pkill -x opentum 2>/dev/null; sleep 1
sudo apt-get install -y "$deb" libsecret-tools
opentum --version
dpkg-query -W -f='${Package} ${Version}\n' open-tum

seccion "3. Verificación de la instalación"
bash "$dir/verificar-instalacion.sh"
echo "Código de salida: $?"

seccion "4. Versiones de las CLI y sesiones disponibles (sin leer su contenido)"
for c in "claude --version" "codex --version" "agy --version"; do
  bin=${c%% *}
  if command -v "$bin" >/dev/null; then echo "$c -> $($c 2>&1 | head -1)"; else echo "$bin: no instalado"; fi
done
[ -f "${CLAUDE_CONFIG_DIR:-$HOME/.claude}/.credentials.json" ] && echo "Claude Code: hay sesión" || echo "Claude Code: sin archivo de sesión"
{ [ -f "${CODEX_HOME:-$HOME/.codex}/auth.json" ] || [ -f "$HOME/.config/codex/auth.json" ]; } && echo "Codex: hay sesión" || echo "Codex: sin archivo de sesión"
secret-tool search service gemini username antigravity >/dev/null 2>&1 && echo "Antigravity: hay sesión en el llavero" || echo "Antigravity: sin sesión en el llavero"

seccion "5. Salida de texto de agy (para ajustar el intérprete)"
if command -v agy >/dev/null; then
  timeout 30 agy -p /usage   > "$out/agy-usage.txt"   2>&1; echo "agy -p /usage: código $?"
  timeout 30 agy -p /credits > "$out/agy-credits.txt" 2>&1; echo "agy -p /credits: código $?"
else
  echo "agy no instalado"
fi

seccion "6. Ciclo con cuentas reales (configuración temporal)"
base=$(mktemp -d /tmp/opentum-prueba.XXXXXX)
export XDG_CONFIG_HOME="$base/config" XDG_CACHE_HOME="$base/cache" XDG_STATE_HOME="$base/state"
mkdir -p "$XDG_CONFIG_HOME/opentum"
cat > "$XDG_CONFIG_HOME/opentum/config.json" <<'EOF'
{"version":2,"intervaloSeg":120,"umbrales":[80,95],"proveedores":[
 {"id":"claude_code","habilitado":true,"mecanismo":"ARCHIVO_LOCAL"},
 {"id":"codex","habilitado":true,"mecanismo":"CLI"},
 {"id":"antigravity","habilitado":true,"mecanismo":"CLI"}]}
EOF
opentum --abrir panel > "$out/app.log" 2>&1 &
pid=$!
for _ in $(seq 1 60); do [ -f "$XDG_CACHE_HOME/opentum/lecturas.json" ] && break; sleep 1; done
sleep 3
if [ -f "$XDG_CACHE_HOME/opentum/lecturas.json" ]; then
  cp "$XDG_CACHE_HOME/opentum/lecturas.json" "$out/lecturas.json"
  python3 - "$out/lecturas.json" <<'PY'
import json, sys
for prov, l in json.load(open(sys.argv[1])).items():
    print(f"{prov}: {l['estado']}  ({l.get('detalleEstado','')})  obtenida {l['obtenidaEn']}")
    for m in l["metricas"]:
        r = (m.get("ventana") or {}).get("reinicioEn", "")
        print(f"    {m['nombre']:<24} {m['valor']:>8} {m['unidad']:<10} {m['sentido']:<10} origen {m['origen']:<13} reinicio {r}")
PY
else
  echo "No se generó lecturas.json"
fi

read -r -p "Presioná Enter para cerrar openTUM… " _
kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
grep -iE "warn|error" "$out/app.log" | grep -v dbind > "$out/app-advertencias.txt"
rm -rf "$base"

seccion "Fin"
echo "Resultados en: $out"
echo "registro.txt, lecturas.json, agy-usage.txt, agy-credits.txt "
