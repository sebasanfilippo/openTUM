//! Custodia de credenciales en el almacén de secretos de la sesión mediante
//! Secret Service (HU-02 y HU-18). Ninguna credencial se escribe en archivos.

use secret_service::blocking::SecretService;
use secret_service::EncryptionType;
use std::collections::HashMap;

const APP_ATTR: (&str, &str) = ("application", "opentum");

#[derive(Debug, Clone, PartialEq)]
pub enum ErrorAlmacen {
    /// No hay un servicio de secretos en el bus de sesión.
    Ausente(String),
    /// La colección está bloqueada y el usuario no la desbloqueó.
    Bloqueado,
    Otro(String),
}

impl std::fmt::Display for ErrorAlmacen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ausente(e) => write!(f, "No se encontró el almacén de secretos de la sesión ({e}). La clave no se guardó."),
            Self::Bloqueado => write!(f, "El almacén de secretos está bloqueado. Desbloquéelo e intente de nuevo; la clave no se guardó."),
            Self::Otro(e) => write!(f, "Error del almacén de secretos: {e}"),
        }
    }
}

fn conectar() -> Result<SecretService<'static>, ErrorAlmacen> {
    SecretService::connect(EncryptionType::Dh).map_err(|e| ErrorAlmacen::Ausente(e.to_string()))
}

/// Formato de una clave de gestión de xAI: texto imprimible sin espacios, de 20 a 200 caracteres.
pub fn validar_formato_clave(clave: &str) -> Result<(), String> {
    let c = clave.trim();
    if c.len() < 20 || c.len() > 200 {
        return Err("La clave debe tener entre 20 y 200 caracteres.".into());
    }
    if c.chars().any(|ch| ch.is_whitespace() || ch.is_control()) {
        return Err("La clave no puede contener espacios ni caracteres de control.".into());
    }
    Ok(())
}

pub fn guardar_xai(team_id: &str, clave: &str) -> Result<(), ErrorAlmacen> {
    let ss = conectar()?;
    let col = ss.get_default_collection().map_err(|e| ErrorAlmacen::Ausente(e.to_string()))?;
    col.ensure_unlocked().map_err(|_| ErrorAlmacen::Bloqueado)?;
    if col.is_locked().unwrap_or(true) {
        return Err(ErrorAlmacen::Bloqueado);
    }
    let attrs = HashMap::from([APP_ATTR, ("proveedor", "xai"), ("teamId", team_id)]);
    col.create_item("openTUM: clave de gestión de xAI", attrs, clave.trim().as_bytes(), true, "text/plain")
        .map(|_| ())
        .map_err(|e| ErrorAlmacen::Otro(e.to_string()))
}

pub fn obtener_xai(team_id: &str) -> Result<Option<String>, ErrorAlmacen> {
    leer(HashMap::from([APP_ATTR, ("proveedor", "xai"), ("teamId", team_id)]))
}

/// Lectura de un elemento ajeno (por ejemplo, el que guarda Antigravity mediante
/// go-keyring) sin modificarlo ni copiarlo.
pub fn leer_ajeno(service: &str, username: &str) -> Result<Option<String>, ErrorAlmacen> {
    leer(HashMap::from([("service", service), ("username", username)]))
}

fn leer(attrs: HashMap<&str, &str>) -> Result<Option<String>, ErrorAlmacen> {
    let ss = conectar()?;
    let res = ss.search_items(attrs).map_err(|e| ErrorAlmacen::Otro(e.to_string()))?;
    let item = match (res.unlocked.into_iter().next(), res.locked.into_iter().next()) {
        (Some(i), _) => i,
        (None, Some(i)) => {
            i.unlock().map_err(|_| ErrorAlmacen::Bloqueado)?;
            i
        }
        (None, None) => return Ok(None),
    };
    let secreto = item.get_secret().map_err(|e| ErrorAlmacen::Otro(e.to_string()))?;
    Ok(Some(String::from_utf8_lossy(&secreto).into_owned()))
}

/// Elimina los elementos con los atributos de la aplicación (HU-18). No toca los
/// elementos de otras herramientas.
pub fn eliminar_todas() -> Result<usize, ErrorAlmacen> {
    let ss = conectar()?;
    let res = ss.search_items(HashMap::from([APP_ATTR])).map_err(|e| ErrorAlmacen::Otro(e.to_string()))?;
    let mut n = 0;
    for item in res.unlocked.into_iter().chain(res.locked) {
        item.delete().map_err(|e| ErrorAlmacen::Otro(e.to_string()))?;
        n += 1;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formato_de_clave() {
        assert!(validar_formato_clave("corta").is_err());
        assert!(validar_formato_clave("xai-mgmt-0123456789abcdef con espacio").is_err());
        assert!(validar_formato_clave("xai-mgmt-0123456789abcdefXYZ").is_ok());
    }
}
