//! Antigravity (HU-08).
//!
//! Mecanismo principal (diseño): `agy -p /usage` y `agy -p /credits`, leyendo la
//! salida estándar en texto. Como ese uso no está documentado, si la CLI no está
//! instalada o la salida no tiene cuotas reconocibles se usa la alternativa de
//! OpenUsage: el resumen de cuotas de Cloud Code con el token que
//! Antigravity guarda en el almacén de secretos (servicio `gemini`, usuario
//! `antigravity`). El token se lee, no se copia ni se renueva.

use super::{fecha, http_json, Conector, Contexto};
use crate::almacen::Consulta;
use crate::credenciales;
use crate::modelo::{EstadoFuente, FallaFuente, IdProveedor, Metrica, OrigenDato, Sentido, TipoLimite, Unidad, Ventana};
use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use regex::Regex;
use serde_json::{json, Value};
use std::process::{Command, Stdio};
use std::time::Instant;

pub struct Antigravity;

const BASES: [&str; 2] = ["https://cloudcode-pa.googleapis.com", "https://daily-cloudcode-pa.googleapis.com"];
const RESUMEN: &str = "/v1internal:retrieveUserQuotaSummary";

fn ejecutar(args: &[&str], limite: std::time::Duration) -> Option<String> {
    let bin = super::terminal::ubicar("agy")?;
    let mut hijo = Command::new(bin).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let inicio = Instant::now();
    loop {
        match hijo.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if inicio.elapsed() < limite => std::thread::sleep(std::time::Duration::from_millis(100)),
            _ => {
                let _ = hijo.kill();
                let _ = hijo.wait();
                return None;
            }
        }
    }
    let salida = hijo.wait_with_output().ok()?;
    Some(String::from_utf8_lossy(&salida.stdout).into_owned())
}

fn sin_ansi(s: &str) -> String {
    Regex::new(r"\x1b\[[0-9;?]*[ -/]*[@-~]").unwrap().replace_all(s, "").into_owned()
}

/// Intérprete tolerante de la salida de texto de `/usage`: cada línea con un
/// porcentaje se toma como una cuota; «remaining/restante/left» indica sentido
/// DISPONIBLE y «resets in 3h 12m» o «resets in 2d» fija el reinicio.
pub fn interpretar_usage(texto: &str, ahora: DateTime<Utc>) -> Vec<Metrica> {
    let tabular = interpretar_tabla(texto);
    if !tabular.is_empty() {
        return tabular;
    }
    let pct = Regex::new(r"(\d{1,3}(?:[.,]\d+)?)\s*%").unwrap();
    let reinicio = Regex::new(r"(?i)resets?\s+in\s+(?:(\d+)\s*d)?\s*(?:(\d+)\s*h)?\s*(?:(\d+)\s*m)?").unwrap();
    let mut v: Vec<Metrica> = Vec::new();
    for linea in sin_ansi(texto).lines() {
        let Some(c) = pct.captures(linea) else { continue };
        let Ok(p) = c[1].replace(',', ".").parse::<f64>() else { continue };
        if p > 100.0 { continue; }
        let etiqueta = linea[..c.get(0).unwrap().start()].trim_matches(|ch: char| !ch.is_alphanumeric()).to_lowercase();
        let base: String = etiqueta.chars().map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' }).collect::<String>()
            .split('_').filter(|s| !s.is_empty()).collect::<Vec<_>>().join("_");
        if base.is_empty() { continue; }
        let mut nombre: String = base.chars().take(40).collect();
        if v.iter().any(|m| m.nombre == nombre) { nombre = format!("{}_{}", &nombre[..nombre.len().min(37)], v.len()); }
        let l = linea.to_lowercase();
        let disponible = l.contains("remaining") || l.contains("left") || l.contains("restante");
        let semanal = l.contains("week") || l.contains("seman");
        let r = reinicio.captures(linea).map(|c| {
            let g = |i: usize| c.get(i).and_then(|m| m.as_str().parse::<i64>().ok()).unwrap_or(0);
            ahora + Duration::days(g(1)) + Duration::hours(g(2)) + Duration::minutes(g(3))
        });
        v.push(Metrica {
            nombre,
            valor: p,
            sentido: if disponible { Sentido::Disponible } else { Sentido::Consumido },
            unidad: Unidad::Porcentaje,
            limite: Some(100.0),
            tipo_limite: TipoLimite::Cuota,
            origen: OrigenDato::Cli,
            es_estimado: false,
            ventana: Some(Ventana { duracion: if semanal { "P7D".into() } else { "PT5H".into() }, reinicio_en: r }),
        });
    }
    v
}

/// Nombre corto del grupo de modelos: «Gemini Models» → gemini, «Claude and GPT
/// models» → claude_gpt (el grupo de modelos de terceros que comparten cuota).
fn grupo(texto: &str) -> String {
    let t = texto.to_lowercase().replace(" models", "").replace(" model", "");
    let t = t.replace(" and ", "_");
    let s: String = t.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    s.split('_').filter(|x| !x.is_empty()).collect::<Vec<_>>().join("_")
}

/// Salida de `agy -p /usage` en columnas separadas por tabulaciones (agy 1.5.3):
/// grupo, límite, porcentaje restante y reinicio en RFC 3339.
pub fn interpretar_tabla(texto: &str) -> Vec<Metrica> {
    let mut v: Vec<Metrica> = Vec::new();
    for linea in sin_ansi(texto).lines() {
        let c: Vec<&str> = linea.split('\t').map(str::trim).collect();
        if c.len() < 3 { continue; }
        let Some(p) = c[2].strip_suffix('%').and_then(|x| x.trim().replace(',', ".").parse::<f64>().ok()) else { continue };
        let limite = c[1].to_lowercase();
        let semanal = limite.contains("week");
        let nombre: String = format!("{}_{}", grupo(c[0]), if semanal { "semanal" } else { "5h" }).chars().take(40).collect();
        if v.iter().any(|m| m.nombre == nombre) { continue; }
        let mut m = Metrica::cuota_porcentaje(&nombre, p, if semanal { "P7D" } else { "PT5H" },
            c.get(3).and_then(|r| DateTime::parse_from_rfc3339(r).ok()).map(|d| d.to_utc()), OrigenDato::Cli);
        if !limite.contains("used") { m.sentido = Sentido::Disponible; }
        v.push(m);
    }
    v
}

/// Saldo de créditos a partir de `/credits`: primer número seguido de «credit».
pub fn interpretar_creditos(texto: &str) -> Option<Metrica> {
    let limpio = sin_ansi(texto);
    let tabla = Regex::new(r"(?i)remaining\s+credits\s+(\d[\d.,]*)").unwrap();
    let re = Regex::new(r"(?i)(\d[\d.,]*)\s*(?:ai\s+)?credits?").unwrap();
    let c = tabla.captures(&limpio).or_else(|| re.captures(&limpio))?;
    let valor: f64 = c[1].replace(',', "").parse().ok()?;
    Some(Metrica { nombre: "saldo_creditos".into(), valor, sentido: Sentido::Disponible, unidad: Unidad::Creditos, limite: None,
        tipo_limite: TipoLimite::Saldo, origen: OrigenDato::Cli, es_estimado: false, ventana: None })
}

/// Resumen de cuotas de Cloud Code (alternativa): fracción restante por grupo.
pub fn mapear_resumen(body: &Value) -> Vec<Metrica> {
    let grupos = body.pointer("/response/groups").or_else(|| body.get("groups")).and_then(Value::as_array);
    let mut v = Vec::new();
    for (id, nombre, dur) in [("gemini-5h", "gemini_5h", "PT5H"), ("gemini-weekly", "gemini_semanal", "P7D"),
                              ("3p-5h", "claude_gpt_5h", "PT5H"), ("3p-weekly", "claude_gpt_semanal", "P7D")] {
        let b = grupos.into_iter().flatten().filter_map(|g| g.get("buckets").and_then(Value::as_array)).flatten()
            .find(|b| b.get("bucketId").and_then(Value::as_str) == Some(id));
        if let Some(b) = b {
            if let Some(f) = b.get("remainingFraction").and_then(Value::as_f64) {
                let mut m = Metrica::cuota_porcentaje(nombre, (1.0 - f.clamp(0.0, 1.0)) * 100.0, dur, fecha(b.get("resetTime")), OrigenDato::Api);
                m.valor = (m.valor * 10.0).round() / 10.0;
                v.push(m);
            }
        }
    }
    v
}

/// Extrae el token de acceso del elemento que guarda Antigravity (go-keyring).
pub fn token_de(raw: &str) -> Option<String> {
    let t = raw.trim();
    let texto = match t.strip_prefix("go-keyring-base64:") {
        Some(b) => String::from_utf8(base64::engine::general_purpose::STANDARD.decode(b.trim()).ok()?).ok()?,
        None => t.to_owned(),
    };
    match serde_json::from_str::<Value>(texto.trim()) {
        Ok(v) => {
            let src = v.get("token").filter(|x| x.is_object()).unwrap_or(&v);
            ["access_token", "accessToken", "token"].iter().find_map(|k| src.get(*k).and_then(Value::as_str)).map(str::to_owned)
                .or_else(|| v.as_str().map(str::to_owned))
        }
        Err(_) => Some(texto.trim().trim_start_matches("Bearer ").to_owned()).filter(|s| !s.is_empty() && !s.starts_with('{')),
    }
}

impl Conector for Antigravity {
    fn id(&self) -> IdProveedor { IdProveedor::Antigravity }

    fn consultar(&self, ctx: &Contexto) -> Result<Consulta, FallaFuente> {
        // 1. CLI (diseño).
        if let Some(salida) = ejecutar(&["-p", "/usage"], std::time::Duration::from_secs(20)) {
            let mut m = interpretar_usage(&salida, ctx.ahora);
            if !m.is_empty() {
                if let Some(c) = ejecutar(&["-p", "/credits"], std::time::Duration::from_secs(20)).as_deref().and_then(interpretar_creditos) {
                    m.push(c);
                }
                return Ok(Consulta { metricas: m, obtenida_en: None, detalle: Some("Fuente: agy -p /usage.".into()) });
            }
        }
        // 2. Alternativa: resumen de cuotas con la sesión guardada por Antigravity.
        let raw = credenciales::leer_ajeno("gemini", "antigravity")
            .map_err(|e| FallaFuente::new(EstadoFuente::NoDisponible, e.to_string()))?
            .ok_or_else(|| FallaFuente::new(EstadoFuente::NoDisponible, "La CLI de Antigravity no está disponible o no tiene sesión. Ejecute `agy` e inicie sesión."))?;
        let token = token_de(&raw).ok_or_else(|| FallaFuente::new(EstadoFuente::FormatoNoReconocido, "No se reconoce la sesión guardada por Antigravity."))?;
        let mut ultimo = None;
        for base in BASES {
            let r = http_json(ctx.http.post(format!("{base}{RESUMEN}")).bearer_auth(&token).header("User-Agent", "antigravity").json(&json!({})).send(),
                "La sesión de Antigravity venció. Abra Antigravity o ejecute `agy` para renovarla.");
            match r {
                Ok(body) => {
                    let m = mapear_resumen(&body);
                    if m.is_empty() {
                        return Err(FallaFuente::new(EstadoFuente::FormatoNoReconocido, "El resumen de cuotas de Antigravity no tiene grupos reconocibles."));
                    }
                    return Ok(Consulta { metricas: m, obtenida_en: None, detalle: Some("Fuente: resumen de cuotas con la sesión de Antigravity.".into()) });
                }
                Err(f) if f.estado == EstadoFuente::CredencialInvalida || f.estado == EstadoFuente::PermisoInsuficiente => return Err(f),
                Err(f) => ultimo = Some(f),
            }
        }
        Err(ultimo.unwrap_or_else(|| FallaFuente::new(EstadoFuente::NoDisponible, "Antigravity no respondió.")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpreta_salida_de_texto() {
        let t = "\x1b[1mModel quotas\x1b[0m\n  Gemini Pro      45% used   resets in 3h 12m\n  Claude (3P)     80% remaining  resets in 2d 4h\n";
        let m = interpretar_usage(t, Utc::now());
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].nombre, "gemini_pro");
        assert_eq!(m[0].porcentaje_consumido(), Some(45.0));
        assert_eq!(m[1].porcentaje_consumido(), Some(20.0));
        assert!(m[1].ventana.as_ref().unwrap().reinicio_en.is_some());
    }

    #[test]
    fn interpreta_la_tabla_de_agy() {
        let t = include_str!("../../tests/fixtures/agy_usage.txt");
        let m = interpretar_usage(t, Utc::now());
        assert_eq!(m.len(), 5);
        let g = m.iter().find(|x| x.nombre == "gemini_5h").unwrap();
        assert_eq!(g.porcentaje_consumido(), Some(0.0));
        assert_eq!(g.origen, OrigenDato::Cli);
        assert_eq!(g.ventana.as_ref().unwrap().reinicio_en.unwrap().to_rfc3339(), "2026-11-08T23:20:13+00:00");
        assert!(m.iter().any(|x| x.nombre == "claude_gpt_semanal"));
        assert!(m.iter().any(|x| x.nombre == "gemini_4_argon_semanal"));
        assert_eq!(interpretar_creditos("Remaining credits\t0\nUpgrade\thttps://x").unwrap().valor, 0.0);
    }

    #[test]
    fn mapea_resumen_de_cloud_code() {
        let body: Value = serde_json::from_str(include_str!("../../tests/fixtures/antigravity_summary.json")).unwrap();
        let m = mapear_resumen(&body);
        let g = m.iter().find(|x| x.nombre == "gemini_5h").unwrap();
        assert_eq!(g.valor, 20.0);
        assert!(m.iter().any(|x| x.nombre == "claude_gpt_semanal" && x.valor == 50.0));
    }

    #[test]
    fn token_go_keyring() {
        let raw = format!("go-keyring-base64:{}", base64::engine::general_purpose::STANDARD.encode(r#"{"token":{"access_token":"ya29.x","refresh_token":"r"}}"#));
        assert_eq!(token_de(&raw).as_deref(), Some("ya29.x"));
        assert_eq!(interpretar_creditos("You have 1,250 AI credits").unwrap().valor, 1250.0);
    }
}
