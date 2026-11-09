//! Ubicación XDG de los archivos y escritura atómica con permisos privados.

use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub const APP: &str = "opentum";

#[derive(Debug, Clone)]
pub struct Rutas {
    pub config: PathBuf,
    pub lecturas: PathBuf,
    pub alertas: PathBuf,
    /// Archivo que escribe el comando de línea de estado de Claude Code.
    pub claude_statusline: PathBuf,
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(fallback))
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"))
}

impl Rutas {
    pub fn xdg() -> Self {
        let config = xdg("XDG_CONFIG_HOME", ".config").join(APP);
        let cache = xdg("XDG_CACHE_HOME", ".cache").join(APP);
        let state = xdg("XDG_STATE_HOME", ".local/state").join(APP);
        Self::en(&config, &cache, &state)
    }

    pub fn en(config: &Path, cache: &Path, state: &Path) -> Self {
        Rutas {
            config: config.join("config.json"),
            lecturas: cache.join("lecturas.json"),
            alertas: state.join("alertas.json"),
            claude_statusline: cache.join("claude-statusline.json"),
        }
    }

    pub fn todas(&self) -> [&Path; 4] {
        [&self.config, &self.lecturas, &self.alertas, &self.claude_statusline]
    }
}

/// Crea el directorio con modo 0700 si no existe y corrige el modo si ya existe.
pub fn asegurar_directorio(dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
}

/// Escribe en un temporal del mismo directorio (modo 0600) y lo renombra sobre el
/// destino. Reduce el riesgo de dejar un archivo truncado ante una interrupción,
/// aunque no garantiza conservar la última escritura.
pub fn escribir_atomico(destino: &Path, datos: &[u8]) -> std::io::Result<()> {
    let dir = destino.parent().ok_or_else(|| std::io::Error::other("ruta sin directorio"))?;
    asegurar_directorio(dir)?;
    let tmp = dir.join(format!(
        ".{}.tmp-{}",
        destino.file_name().and_then(|n| n.to_str()).unwrap_or("archivo"),
        std::process::id()
    ));
    {
        let mut f = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
        f.write_all(datos)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, destino)?;
    fs::set_permissions(destino, fs::Permissions::from_mode(0o600))
}

pub fn leer(ruta: &Path) -> Option<Vec<u8>> {
    let meta = fs::metadata(ruta).ok()?;
    if meta.len() > 4 * 1024 * 1024 {
        return None;
    }
    fs::read(ruta).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escritura_atomica_con_modos_privados() {
        let base = tempfile::tempdir().unwrap();
        let destino = base.path().join("sub").join("config.json");
        escribir_atomico(&destino, b"{}").unwrap();
        let modo_archivo = fs::metadata(&destino).unwrap().permissions().mode() & 0o777;
        let modo_dir = fs::metadata(destino.parent().unwrap()).unwrap().permissions().mode() & 0o777;
        assert_eq!(modo_archivo, 0o600);
        assert_eq!(modo_dir, 0o700);
        assert_eq!(fs::read(&destino).unwrap(), b"{}");
    }
}
