//! Claude Code (HU-05).
//!
//! Mecanismo principal (diseño): la línea de estado documentada de Claude Code
//! entrega por la entrada estándar un JSON con `rate_limits.five_hour` y
//! `rate_limits.seven_day`. El comando `opentum --statusline` guarda solo ese
//! bloque en la caché de la aplicación y el conector lo lee.
//!
//! Alternativa (OpenUsage): si no hay datos recientes de la línea de
//! estado, se consulta el endpoint de uso OAuth con el token que Claude Code
//! guarda en `~/.claude/.credentials.json`. El token se lee, no se copia ni se
//! renueva.

use super::{fecha, http_json, num, Conector, Contexto};
use crate::almacen::Consulta;
use crate::archivos;
use crate::modelo::{EstadoFuente, FallaFuente, IdProveedor, Metrica, OrigenDato, Sentido, TipoLimite, Unidad, Ventana};
use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use std::io::Read;
use std::path::PathBuf;

pub struct Claude;

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";

/// Métricas a partir del objeto `rate_limits` de la línea de estado.
pub fn desde_statusline(rate_limits: &Value) -> Vec<Metrica> {
    let mut v = Vec::new();
    for (clave, nombre, dur) in [("five_hour", "sesion_5h", "PT5H"), ("seven_day", "semanal", "P7D")] {
        if let Some(w) = rate_limits.get(clave) {
            if let Some(p) = num(w.get("used_percentage")) {
                v.push(Metrica::cuota_porcentaje(nombre, p, dur, fecha(w.get("resets_at")), OrigenDato::ArchivoLocal));
            }
        }
    }
    v
}

/// Métricas a partir de la respuesta del endpoint de uso OAuth.
pub fn desde_api(body: &Value) -> Vec<Metrica> {
    let mut v = Vec::new();
    for (clave, nombre, dur) in [("five_hour", "sesion_5h", "PT5H"), ("seven_day", "semanal", "P7D"), ("seven_day_sonnet", "semanal_sonnet", "P7D")] {
        if let Some(w) = body.get(clave).filter(|w| w.is_object()) {
            if let Some(p) = num(w.get("utilization")) {
                v.push(Metrica::cuota_porcentaje(nombre, p, dur, fecha(w.get("resets_at")), OrigenDato::Api));
            }
        }
    }
    if let Some(x) = body.get("extra_usage").filter(|x| x.get("is_enabled").and_then(Value::as_bool) == Some(true)) {
        let decimales = num(x.get("decimal_places")).unwrap_or(2.0);
        let div = 10f64.powf(decimales);
        if let Some(usado) = num(x.get("used_credits")) {
            v.push(Metrica {
                nombre: "creditos_adicionales".into(),
                valor: usado / div,
                sentido: Sentido::Consumido,
                unidad: Unidad::Usd,
                limite: num(x.get("monthly_limit")).filter(|l| *l > 0.0).map(|l| l / div),
                tipo_limite: TipoLimite::TopeGasto,
                origen: OrigenDato::Api,
                es_estimado: false,
                ventana: Some(Ventana { duracion: "P1M".into(), reinicio_en: None }),
            });
        }
    }
    v
}

fn credenciales() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from)
        .unwrap_or_else(|| archivos::home().join(".claude"))
        .join(".credentials.json")
}

fn leer_statusline(ruta: &std::path::Path) -> Option<(DateTime<Utc>, Vec<Metrica>)> {
    let doc: Value = serde_json::from_slice(&archivos::leer(ruta)?).ok()?;
    let t = fecha(doc.get("capturadoEn"))?;
    let m = desde_statusline(doc.get("rate_limits")?);
    (!m.is_empty()).then_some((t, m))
}

impl Conector for Claude {
    fn id(&self) -> IdProveedor { IdProveedor::ClaudeCode }

    fn consultar(&self, ctx: &Contexto) -> Result<Consulta, FallaFuente> {
        let local = leer_statusline(&ctx.rutas.claude_statusline);
        let frescura = Duration::seconds(2 * ctx.config.intervalo_seg as i64);
        if let Some((t, m)) = &local {
            if ctx.ahora - *t <= frescura {
                return Ok(Consulta { metricas: m.clone(), obtenida_en: Some(*t), detalle: Some("Fuente: línea de estado de Claude Code.".into()) });
            }
        }
        // Alternativa: endpoint OAuth con la sesión de la CLI.
        let api = (|| {
            let doc: Value = serde_json::from_slice(&archivos::leer(&credenciales()).ok_or_else(|| FallaFuente::new(EstadoFuente::NoDisponible,
                "Configure la línea de estado (opentum --statusline) o inicie sesión en Claude Code con `claude`."))?)
                .map_err(|_| FallaFuente::new(EstadoFuente::FormatoNoReconocido, "No se reconoce el archivo de sesión de Claude Code."))?;
            let oauth = doc.get("claudeAiOauth").ok_or_else(|| FallaFuente::new(EstadoFuente::NoDisponible, "La sesión de Claude Code no es de un plan Pro o Max."))?;
            let token = oauth.get("accessToken").and_then(Value::as_str).ok_or_else(|| FallaFuente::new(EstadoFuente::FormatoNoReconocido, "La sesión de Claude Code no tiene token."))?;
            if fecha(oauth.get("expiresAt")).is_some_and(|e| e <= ctx.ahora) {
                return Err(FallaFuente::new(EstadoFuente::CredencialInvalida, "La sesión de Claude Code venció. Ejecute `claude` para renovarla."));
            }
            let body = http_json(ctx.http.get(USAGE_URL).bearer_auth(token).header("anthropic-beta", "oauth-2025-04-20")
                .header("Accept", "application/json").send(), "La sesión de Claude Code no es válida. Ejecute `claude` e inicie sesión.")?;
            let m = desde_api(&body);
            if m.is_empty() {
                return Err(FallaFuente::new(EstadoFuente::FormatoNoReconocido, "La respuesta de uso de Claude no tiene ventanas reconocibles."));
            }
            Ok(Consulta { metricas: m, obtenida_en: None, detalle: Some("Fuente: endpoint de uso con la sesión de Claude Code.".into()) })
        })();
        match (api, local) {
            (Ok(c), _) => Ok(c),
            // Sin alternativa disponible: se conserva el último dato de la línea de estado con su fecha (HU-05.2).
            (Err(_), Some((t, m))) => Ok(Consulta { metricas: m, obtenida_en: Some(t),
                detalle: Some("Sin sesión activa de Claude Code desde la última lectura de la línea de estado.".into()) }),
            (Err(e), None) => Err(e),
        }
    }
}

/// Subcomando `opentum --statusline`: lo invoca Claude Code como línea de estado.
/// Guarda solo `rate_limits` con la hora de captura e imprime una línea breve.
pub fn ejecutar_statusline() -> i32 {
    let mut entrada = String::new();
    if std::io::stdin().take(1 << 20).read_to_string(&mut entrada).is_err() {
        return 0;
    }
    let Ok(doc) = serde_json::from_str::<Value>(&entrada) else {
        println!("openTUM");
        return 0;
    };
    let Some(rl) = doc.get("rate_limits").filter(|r| r.is_object()) else {
        println!("openTUM: sin datos de cuota");
        return 0;
    };
    let rutas = archivos::Rutas::xdg();
    let salida = json!({ "capturadoEn": Utc::now().to_rfc3339(), "rate_limits": rl });
    let _ = archivos::escribir_atomico(&rutas.claude_statusline, salida.to_string().as_bytes());
    let pct = |k: &str| num(rl.get(k).and_then(|w| w.get("used_percentage"))).map(|p| format!("{p:.0} %")).unwrap_or_else(|| "—".into());
    println!("5 h {} · 7 d {}", pct("five_hour"), pct("seven_day"));
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statusline_del_ejemplo_del_documento() {
        let rl = json!({"five_hour":{"used_percentage":23.5,"resets_at":1792008000},"seven_day":{"used_percentage":41.2,"resets_at":1792443600}});
        let m = desde_statusline(&rl);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].nombre, "sesion_5h");
        assert_eq!(m[0].valor, 23.5);
        assert_eq!(m[0].origen, OrigenDato::ArchivoLocal);
        assert_eq!(m[0].ventana.as_ref().unwrap().duracion, "PT5H");
        assert_eq!(m[0].ventana.as_ref().unwrap().reinicio_en.unwrap().timestamp(), 1792008000);
    }

    #[test]
    fn respuesta_api_con_creditos_adicionales() {
        let body: Value = serde_json::from_str(include_str!("../../tests/fixtures/claude_usage.json")).unwrap();
        let m = desde_api(&body);
        assert!(m.iter().any(|x| x.nombre == "sesion_5h" && x.valor == 37.0));
        let extra = m.iter().find(|x| x.nombre == "creditos_adicionales").unwrap();
        assert_eq!(extra.valor, 12.5);
        assert_eq!(extra.limite, Some(50.0));
    }
}
