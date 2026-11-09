//! Lecturas simuladas y anonimizadas (TT-02). Con `OPENTUM_SIMULADO=1` cada
//! conector se reemplaza por uno que interpreta las salidas de ejemplo de
//! `tests/fixtures` con los mismos intérpretes de los conectores reales. Sirve
//! para revisar la interfaz y las alertas sin cuentas reales; no demuestra el
//! acceso a las fuentes.

use super::{antigravity, claude, codex, xai, Conector, Contexto};
use crate::almacen::Consulta;
use crate::modelo::{FallaFuente, IdProveedor, Metrica};
use chrono::Duration;
use serde_json::Value;

pub struct Simulado(pub IdProveedor);

pub fn activo() -> bool {
    std::env::var("OPENTUM_SIMULADO").is_ok_and(|v| v == "1")
}

fn json(s: &str) -> Value {
    serde_json::from_str(s).expect("fixture")
}

/// Desplaza los reinicios para que queden en el futuro respecto de `ahora` y
/// varía levemente el uso entre ciclos para ver el efecto de las alertas.
fn ajustar(mut m: Vec<Metrica>, ctx: &Contexto, paso: f64) -> Vec<Metrica> {
    for (i, x) in m.iter_mut().enumerate() {
        if let Some(v) = x.ventana.as_mut() {
            if v.duracion == "P1M" { v.reinicio_en = None; continue; }
            let horas = if v.duracion == "PT5H" { 2 + i as i64 } else { 70 + 5 * i as i64 };
            v.reinicio_en = Some(ctx.ahora + Duration::hours(horas));
        }
        if x.porcentaje_consumido().is_some() && x.unidad == crate::modelo::Unidad::Porcentaje {
            x.valor = (x.valor + paso).min(100.0);
        }
    }
    m
}

impl Conector for Simulado {
    fn id(&self) -> IdProveedor { self.0 }

    fn consultar(&self, ctx: &Contexto) -> Result<Consulta, FallaFuente> {
        let paso = ((ctx.ahora.timestamp() / 300) % 12) as f64 * 4.0;
        let m = match self.0 {
            IdProveedor::ClaudeCode => {
                let e = json(include_str!("../../tests/fixtures/claude_statusline_entrada.json"));
                let mut m = claude::desde_statusline(&e["rate_limits"]);
                m.extend(claude::desde_api(&json(include_str!("../../tests/fixtures/claude_usage.json"))).into_iter().filter(|x| x.nombre == "creditos_adicionales"));
                m
            }
            IdProveedor::Codex => codex::mapear(&json(include_str!("../../tests/fixtures/codex_usage.json")), ctx.ahora),
            IdProveedor::Antigravity => antigravity::mapear_resumen(&json(include_str!("../../tests/fixtures/antigravity_summary.json"))),
            IdProveedor::Xai => xai::mapear(&json(include_str!("../../tests/fixtures/xai_balance.json")),
                Some(&json(include_str!("../../tests/fixtures/xai_preview.json")))),
        };
        Ok(Consulta { metricas: ajustar(m, ctx, paso), obtenida_en: None, detalle: Some("Lectura simulada (OPENTUM_SIMULADO=1).".into()) })
    }
}
