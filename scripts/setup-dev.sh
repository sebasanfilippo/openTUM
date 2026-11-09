#!/usr/bin/env bash
# Prepara un Ubuntu 24.04 para compilar openTUM (ver docs/entorno.md).
set -euo pipefail
sudo apt-get update
sudo apt-get install -y build-essential curl pkg-config libssl-dev file \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libsecret-1-dev \
  gnome-keyring libsecret-tools
if ! command -v rustup >/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain none
  . "$HOME/.cargo/env"
fi
# rust-toolchain.toml fija la versión de Rust; rustup la instala al compilar.
cargo install tauri-cli --version "^2" --locked
echo "Listo. Compile con: cd src-tauri && cargo tauri build"
