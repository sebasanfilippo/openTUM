//! Codex (HU-06).
//!
//! Mecanismo principal (diseño): el panel de `/status`, que solo existe en la
//! interfaz interactiva. Según la contingencia del riesgo R02 se ejecuta `codex`
//! en una pseudoterminal, se envía `/status` y se leen las líneas «5h limit» y
//! «Weekly limit» de la pantalla. Alternativa (OpenUsage): el endpoint
//! de uso de ChatGPT con la sesión que la CLI guarda en `auth.json`, que se lee y
//! no se copia ni se modifica.

use super::{fecha, http_json, num, Conector, Contexto};
use crate::almacen::Consulta;
use crate::archivos;
use crate::modelo::{EstadoFuente, FallaFuente, IdProveedor, Metrica, OrigenDato, Sentido, TipoLimite, Unidad};
use chrono::{DateTime, Duration, Local, NaiveDate, NaiveTime, TimeZone, Utc};
use regex::Regex;
use serde_json::Value;
use std::path::PathBuf;

pub struct Codex;

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

fn rutas_auth() -> Vec<PathBuf> {
    if let Some(h) = std::env::var_os("CODEX_HOME") {
        return vec![PathBuf::from(h).join("auth.json")];
    }
    let home = archivos::home();
    vec![home.join(".codex").join("auth.json"), home.join(".config").join("codex").join("auth.json")]
}

/// Clasifica cada ventana por su duración (`limit_window_seconds`); si falta,
/// usa la posición habitual (primaria: 5 horas, secundaria: semanal).
pub fn mapear(body: &Value, ahora: DateTime<Utc>) -> Vec<Metrica> {
    let rl = body.get("rate_limit");
    let mut v = Vec::new();
    for (clave, nombre_def) in [("primary_window", "sesion_5h"), ("secondary_window", "semanal")] {
        let Some(w) = rl.and_then(|r| r.get(clave)).filter(|w| w.is_object()) else { continue };
        let Some(p) = num(w.get("used_percent")) else { continue };
        let seg = num(w.get("limit_window_seconds"));
        let (nombre, dur) = match seg {
            Some(s) if (s - 18_000.0).abs() < 1.0 => ("sesion_5h", "PT5H"),
            Some(s) if (s - 604_800.0).abs() < 1.0 => ("semanal", "P7D"),
            _ if nombre_def == "sesion_5h" => ("sesion_5h", "PT5H"),
            _ => ("semanal", "P7D"),
        };
        if v.iter().any(|m: &Metrica| m.nombre == nombre) { continue; }
        let reinicio = fecha(w.get("reset_at"))
            .or_else(|| num(w.get("reset_after_seconds")).map(|s| ahora + Duration::seconds(s as i64)));
        v.push(Metrica::cuota_porcentaje(nombre, p, dur, reinicio, OrigenDato::Api));
    }
    // Créditos de Codex: se muestran separados de la cuota (HU-06.3).
    if let Some(c) = body.get("credits") {
        let saldo = num(c.get("balance")).or_else(|| (c.get("has_credits").and_then(Value::as_bool) == Some(false)).then_some(0.0));
        if let Some(s) = saldo {
            v.push(Metrica { nombre: "saldo_creditos".into(), valor: s, sentido: Sentido::Disponible, unidad: Unidad::Creditos,
                limite: None, tipo_limite: TipoLimite::Saldo, origen: OrigenDato::Api, es_estimado: false, ventana: None });
        }
    }
    v
}

/// Reinicio informado por `/status`: «7:50 PM» (hoy o mañana) o «2:45 PM on 14 Nov».
fn reinicio_status(t: &str, ahora: DateTime<Local>) -> Option<DateTime<Utc>> {
    let t = t.trim();
    let (hora, fecha) = match t.split_once(" on ") { Some((h, f)) => (h.trim(), Some(f.trim())), None => (t, None) };
    let hh = NaiveTime::parse_from_str(&hora.to_uppercase(), "%I:%M %p").or_else(|_| NaiveTime::parse_from_str(hora, "%H:%M")).ok()?;
    let dia = match fecha {
        Some(f) => {
            let mut d = NaiveDate::parse_from_str(&format!("{f} {}", ahora.format("%Y")), "%d %b %Y").ok()?;
            if d < ahora.date_naive() - Duration::days(1) { d = d.with_year_siguiente()?; }
            d
        }
        None => if hh > ahora.time() { ahora.date_naive() } else { ahora.date_naive() + Duration::days(1) },
    };
    Local.from_local_datetime(&dia.and_time(hh)).single().map(|d| d.with_timezone(&Utc))
}

trait AnioSiguiente { fn with_year_siguiente(self) -> Option<NaiveDate>; }
impl AnioSiguiente for NaiveDate {
    fn with_year_siguiente(self) -> Option<NaiveDate> { use chrono::Datelike; self.with_year(self.year() + 1) }
}

/// Interpreta la pantalla de `/status`: porcentaje restante y reinicio de cada ventana.
pub fn interpretar_status(pantalla: &str, ahora: DateTime<Local>) -> Vec<Metrica> {
    let re = Regex::new(r"(?i)(5h|weekly)\s+limit:.*?(\d{1,3})%\s+left(?:\s*\(resets\s+([^)]+)\))?").unwrap();
    let mut v = Vec::new();
    for c in re.captures_iter(pantalla) {
        let semanal = c[1].eq_ignore_ascii_case("weekly");
        let (nombre, dur) = if semanal { ("semanal", "P7D") } else { ("sesion_5h", "PT5H") };
        if v.iter().any(|m: &Metrica| m.nombre == nombre) { continue; }
        let Ok(restante) = c[2].parse::<f64>() else { continue };
        let reinicio = c.get(3).and_then(|r| reinicio_status(r.as_str(), ahora));
        let mut m = Metrica::cuota_porcentaje(nombre, restante, dur, reinicio, OrigenDato::Cli);
        m.sentido = Sentido::Disponible;
        v.push(m);
    }
    v
}

fn por_status() -> Option<Vec<Metrica>> {
    let bin = super::terminal::ubicar("codex")?;
    let dir = archivos::home().join(".cache").join(archivos::APP).join("codex-status");
    archivos::asegurar_directorio(&dir).ok()?;
    let pantalla = super::terminal::pantalla_tras_comando(&bin, &dir, "/status", "/status", "Weekly limit", std::time::Duration::from_secs(40))?;
    let m = interpretar_status(&pantalla, Local::now());
    (!m.is_empty()).then_some(m)
}

impl Conector for Codex {
    fn id(&self) -> IdProveedor { IdProveedor::Codex }

    fn consultar(&self, ctx: &Contexto) -> Result<Consulta, FallaFuente> {
        if let Some(m) = por_status() {
            return Ok(Consulta { metricas: m, obtenida_en: None, detalle: Some("Fuente: panel /status de Codex.".into()) });
        }
        let bytes = rutas_auth().iter().find_map(|r| archivos::leer(r))
            .ok_or_else(|| FallaFuente::new(EstadoFuente::NoDisponible, "No hay una sesión de Codex. Ejecute `codex` e inicie sesión con su cuenta de ChatGPT."))?;
        let doc: Value = serde_json::from_slice(&bytes).map_err(|_| FallaFuente::new(EstadoFuente::FormatoNoReconocido, "No se reconoce auth.json de Codex."))?;
        let token = doc.pointer("/tokens/access_token").and_then(Value::as_str)
            .ok_or_else(|| FallaFuente::new(EstadoFuente::NoDisponible, "La sesión de Codex usa una clave de API; la cuota del plan requiere iniciar sesión con ChatGPT."))?;
        let mut req = ctx.http.get(USAGE_URL).bearer_auth(token).header("Accept", "application/json");
        if let Some(acc) = doc.pointer("/tokens/account_id").and_then(Value::as_str).filter(|s| !s.is_empty()) {
            req = req.header("ChatGPT-Account-Id", acc);
        }
        let body = http_json(req.send(), "La sesión de Codex venció. Ejecute `codex` para renovarla.")?;
        let m = mapear(&body, ctx.ahora);
        if m.is_empty() {
            return Err(FallaFuente::new(EstadoFuente::FormatoNoReconocido, "La respuesta de uso de Codex no tiene ventanas reconocibles."));
        }
        Ok(Consulta { metricas: m, obtenida_en: None, detalle: Some("Fuente: endpoint de uso con la sesión de Codex.".into()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpreta_el_panel_status() {
        let p = "  5h limit:            [████████████████████] 100% left (resets 7:50 PM)\n  Weekly limit:        [██████████░░░░░░░░░░] 47% left (resets 2:45 PM on 14 Nov)\n";
        let ahora = Local.with_ymd_and_hms(2026, 11, 8, 14, 50, 0).unwrap();
        let m = interpretar_status(p, ahora);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].porcentaje_consumido(), Some(0.0));
        assert_eq!(m[1].porcentaje_consumido(), Some(53.0));
        let r = m[0].ventana.as_ref().unwrap().reinicio_en.unwrap().with_timezone(&Local);
        assert_eq!(r.format("%d %H:%M").to_string(), "08 19:50");
        let r = m[1].ventana.as_ref().unwrap().reinicio_en.unwrap().with_timezone(&Local);
        assert_eq!(r.format("%d/%m %H:%M").to_string(), "14/11 14:45");
    }

    #[test]
    fn mapea_ventanas_y_creditos() {
        let body: Value = serde_json::from_str(include_str!("../../tests/fixtures/codex_usage.json")).unwrap();
        let m = mapear(&body, Utc::now());
        let s = m.iter().find(|x| x.nombre == "sesion_5h").unwrap();
        assert_eq!(s.valor, 62.0);
        assert_eq!(s.ventana.as_ref().unwrap().reinicio_en.unwrap().timestamp(), 1792008000);
        assert!(m.iter().any(|x| x.nombre == "semanal" && x.valor == 18.0));
        assert!(m.iter().any(|x| x.nombre == "saldo_creditos" && x.unidad == Unidad::Creditos));
    }
}
