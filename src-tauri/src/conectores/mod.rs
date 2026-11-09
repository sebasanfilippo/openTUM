//! Conectores por proveedor. Todos implementan la misma interfaz, de modo que
//! agregar un proveedor no modifica el resto del sistema.

pub mod antigravity;
pub mod claude;
pub mod codex;
pub mod simulado;
pub mod terminal;
pub mod xai;

use crate::almacen::Consulta;
use crate::archivos::Rutas;
use crate::config::Configuracion;
use crate::modelo::{EstadoFuente, FallaFuente, IdProveedor};
use chrono::{DateTime, TimeZone, Utc};
use std::time::Duration;

pub struct Contexto<'a> {
    pub config: &'a Configuracion,
    pub rutas: &'a Rutas,
    pub http: &'a reqwest::blocking::Client,
    pub ahora: DateTime<Utc>,
}

pub trait Conector: Send + Sync {
    fn id(&self) -> IdProveedor;
    fn consultar(&self, ctx: &Contexto) -> Result<Consulta, FallaFuente>;
}

pub fn cliente_http() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(concat!("openTUM/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("cliente HTTP")
}

pub fn para(id: IdProveedor) -> Box<dyn Conector> {
    if simulado::activo() {
        return Box::new(simulado::Simulado(id));
    }
    match id {
        IdProveedor::ClaudeCode => Box::new(claude::Claude),
        IdProveedor::Codex => Box::new(codex::Codex),
        IdProveedor::Xai => Box::new(xai::Xai),
        IdProveedor::Antigravity => Box::new(antigravity::Antigravity),
    }
}

/// Traduce el resultado HTTP a los estados de la fuente (HU-11). Nunca se devuelve
/// un consumo cero ante una falla.
pub fn http_json(r: reqwest::Result<reqwest::blocking::Response>, accion_credencial: &str) -> Result<serde_json::Value, FallaFuente> {
    let resp = r.map_err(|e| {
        if e.is_timeout() || e.is_connect() {
            FallaFuente::new(EstadoFuente::SinConexion, "Sin conexión con el proveedor. Se conserva el último valor válido.")
        } else {
            FallaFuente::new(EstadoFuente::SinConexion, "No se pudo completar la consulta.")
        }
    })?;
    let codigo = resp.status().as_u16();
    match codigo {
        200..=299 => resp.json::<serde_json::Value>().map_err(|_| FallaFuente::new(EstadoFuente::FormatoNoReconocido, "La respuesta del proveedor no tiene el formato esperado.")),
        401 => Err(FallaFuente::new(EstadoFuente::CredencialInvalida, accion_credencial)),
        403 => Err(FallaFuente::new(EstadoFuente::PermisoInsuficiente, "La credencial no tiene permiso para consultar este dato.")),
        429 => Err(FallaFuente::new(EstadoFuente::LimiteConsulta, "El proveedor limitó las consultas (HTTP 429). Se reintentará con un intervalo mayor.")),
        _ => Err(FallaFuente::new(EstadoFuente::NoDisponible, format!("El proveedor respondió HTTP {codigo}."))),
    }
}

pub fn num(v: Option<&serde_json::Value>) -> Option<f64> {
    match v? {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// Fecha en segundos desde la época, en milisegundos o en texto RFC 3339.
pub fn fecha(v: Option<&serde_json::Value>) -> Option<DateTime<Utc>> {
    match v? {
        serde_json::Value::String(s) => DateTime::parse_from_rfc3339(s.trim()).ok().map(|d| d.to_utc()),
        n => {
            let x = num(Some(n))?;
            let s = if x > 1e12 { x / 1000.0 } else { x };
            Utc.timestamp_opt(s as i64, 0).single()
        }
    }
}
