#!/usr/bin/env bash
# Construye el paquete .deb y el AppImage de openTUM a partir del código fuente.
#
# Uso:
#   scripts/construir-paquetes.sh [opciones]
#
# Opciones:
#   --dependencias   instala con apt las bibliotecas de compilación y, si faltan,
#                    rustup y tauri-cli (pide sudo)
#   --sin-pruebas    omite cargo test antes de empaquetar
#   --solo-deb       construye solo el .deb
#   --solo-appimage  construye solo el AppImage
#   --salida DIR     carpeta donde se copian los paquetes (por omisión: dist/)
#   -h, --ayuda      muestra esta ayuda
#
# Probado en Ubuntu 24.04 y 26.04 (x86_64). La versión de Rust la fija
# rust-toolchain.toml; para usar otra: RUSTUP_TOOLCHAIN=stable scripts/construir-paquetes.sh
set -euo pipefail

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SALIDA="$RAIZ/dist"
PRUEBAS=1
DEPENDENCIAS=0
PAQUETES="deb,appimage"

ayuda() { sed -n '2,18p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; }
paso() { printf '\n\033[1;34m==> %s\033[0m\n' "$*"; }
error() { printf '\033[1;31mError:\033[0m %s\n' "$*" >&2; exit 1; }

while [ $# -gt 0 ]; do
  case "$1" in
    --dependencias) DEPENDENCIAS=1 ;;
    --sin-pruebas) PRUEBAS=0 ;;
    --solo-deb) PAQUETES="deb" ;;
    --solo-appimage) PAQUETES="appimage" ;;
    --salida) shift; [ $# -gt 0 ] || error "falta la carpeta después de --salida"; SALIDA="$(mkdir -p "$1" && cd "$1" && pwd)" ;;
    -h|--ayuda) ayuda; exit 0 ;;
    *) error "opción desconocida: $1 (use --ayuda)" ;;
  esac
  shift
done

[ "$(uname -s)" = "Linux" ] || error "los paquetes se construyen en Linux"
[ "$(uname -m)" = "x86_64" ] || echo "Aviso: arquitectura $(uname -m); los paquetes no serán amd64."

# 1. Dependencias del sistema (opcional)
if [ "$DEPENDENCIAS" = 1 ]; then
  paso "Instalando dependencias de compilación"
  sudo apt-get update
  sudo apt-get install -y build-essential curl wget file pkg-config libssl-dev \
    libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev \
    libsecret-1-dev patchelf squashfs-tools desktop-file-utils
  if ! command -v rustup >/dev/null 2>&1 && ! command -v cargo >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain none
  fi
fi

# shellcheck disable=SC1091
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

# 2. Verificación de herramientas
paso "Verificando herramientas"
command -v cargo >/dev/null 2>&1 || error "no se encontró cargo; ejecute con --dependencias o instale rustup"
for pc in webkit2gtk-4.1 ayatana-appindicator3-0.1 libsecret-1 openssl; do
  pkg-config --exists "$pc" 2>/dev/null || error "falta la biblioteca de desarrollo '$pc'; ejecute con --dependencias"
done
if ! cargo tauri --version >/dev/null 2>&1; then
  paso "Instalando tauri-cli 2"
  cargo install tauri-cli --version "^2" --locked
fi
echo "  $(cd "$RAIZ" && rustc --version)"
echo "  $(cargo tauri --version)"

# 3. Pruebas automáticas
if [ "$PRUEBAS" = 1 ]; then
  paso "Ejecutando pruebas (cargo test)"
  (cd "$RAIZ/src-tauri" && cargo test --release)
fi

# 4. Compilación y empaquetado
# linuxdeploy (usado para el AppImage) es a su vez un AppImage: sin FUSE 2
# (Ubuntu 24.04 no lo trae instalado, ni los contenedores) falla con
# "No such file or directory". APPIMAGE_EXTRACT_AND_RUN evita depender de FUSE.
export APPIMAGE_EXTRACT_AND_RUN=1
# Evita que linuxdeploy intente reducir bibliotecas ya reducidas (falla en distros nuevas).
export NO_STRIP=1

paso "Compilando y empaquetando: $PAQUETES"
(cd "$RAIZ/src-tauri" && cargo tauri build --bundles "$PAQUETES")

# 5. Copia de resultados y sumas de verificación
BUNDLE="$RAIZ/src-tauri/target/release/bundle"
mkdir -p "$SALIDA"
paso "Copiando paquetes a $SALIDA"
copiados=()
for f in "$BUNDLE"/deb/*.deb "$BUNDLE"/appimage/*.AppImage; do
  [ -f "$f" ] || continue
  cp -f "$f" "$SALIDA/"
  copiados+=("$(basename "$f")")
done
[ ${#copiados[@]} -gt 0 ] || error "no se generó ningún paquete en $BUNDLE"
(cd "$SALIDA" && sha256sum "${copiados[@]}" > SHA256SUMS)

# 6. Comprobaciones básicas de los paquetes
paso "Comprobando paquetes"
for f in "${copiados[@]}"; do
  case "$f" in
    *.deb)
      dpkg-deb --field "$SALIDA/$f" Package Version Architecture | sed 's/^/  /'
      dpkg-deb --contents "$SALIDA/$f" | grep -q 'usr/bin/opentum$' \
        && echo "  $f: contiene usr/bin/opentum" || error "$f no contiene el ejecutable" ;;
    *.AppImage)
      chmod +x "$SALIDA/$f"
      v="$("$SALIDA/$f" --version 2>/dev/null || true)"
      [ -n "$v" ] && echo "  $f: $v" || error "$f no respondió a --version" ;;
  esac
done

paso "Listo"
ls -lh "$SALIDA" | sed '1d;s/^/  /'
cat <<EOF

Instalación:
  sudo apt install $SALIDA/<paquete>.deb
  o bien: $SALIDA/<archivo>.AppImage
EOF
