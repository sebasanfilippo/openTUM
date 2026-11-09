//! xAI (HU-07): Management API con la clave de gestión (en Secret Service) y el
//! Team ID (en config.json). Los importes se informan en centavos de dólar como
//! texto; las compras de créditos prepagados figuran con signo negativo.

use super::{http_json, num, Conector, Contexto};
use crate::almacen::Consulta;
use crate::credenciales;
use crate::modelo::{EstadoFuente, FallaFuente, IdProveedor, Metrica, OrigenDato, Sentido, TipoLimite, Unidad, Ventana};
use serde_json::Value;

pub struct Xai;

pub const BASE: &str = "https://management-api.x.ai";

fn centavos(v: Option<&Value>) -> Option<f64> {
    num(v.and_then(|x| x.get("val")).or(v)).map(|c| c / 100.0)
}

/// Saldo disponible = créditos prepagados comprados − créditos usados en el ciclo.
/// Se marca como estimado porque se calcula con dos valores del proveedor.
pub fn mapear(balance: &Value, preview: Option<&Value>) -> Vec<Metrica> {
    let mut v = Vec::new();
    let comprado = centavos(balance.get("total")).map(|x| -x);
    let usado = preview.and_then(|p| centavos(p.pointer("/coreInvoice/prepaidCreditsUsed"))).map(f64::abs).unwrap_or(0.0);
    if let Some(c) = comprado {
        v.push(Metrica { nombre: "saldo_prepagado".into(), valor: ((c - usado) * 100.0).round() / 100.0, sentido: Sentido::Disponible,
            unidad: Unidad::Usd, limite: None, tipo_limite: TipoLimite::Saldo, origen: OrigenDato::Api, es_estimado: preview.is_some() && usado > 0.0, ventana: None });
    }
    if let Some(p) = preview {
        if let Some(gasto) = centavos(p.pointer("/coreInvoice/totalWithCorr")) {
            v.push(Metrica { nombre: "gasto_periodo".into(), valor: gasto, sentido: Sentido::Consumido, unidad: Unidad::Usd,
                limite: num(p.get("effectiveSpendingLimit")).map(|c| c / 100.0).filter(|l| *l > 0.0),
                tipo_limite: TipoLimite::TopeGasto, origen: OrigenDato::Api, es_estimado: false,
                ventana: Some(Ventana { duracion: "P1M".into(), reinicio_en: None }) });
        }
    }
    v
}

/// Consulta el saldo con una clave nueva antes de guardarla (HU-02).
pub fn verificar(http: &reqwest::blocking::Client, team_id: &str, clave: &str) -> Result<(), FallaFuente> {
    http_json(http.get(format!("{BASE}/v1/billing/teams/{team_id}/prepaid/balance")).bearer_auth(clave.trim()).send(),
        "La clave de gestión no es válida.").map(|_| ())
}

impl Conector for Xai {
    fn id(&self) -> IdProveedor { IdProveedor::Xai }

    fn consultar(&self, ctx: &Contexto) -> Result<Consulta, FallaFuente> {
        let team = ctx.config.proveedor(IdProveedor::Xai).and_then(|p| p.team_id.clone())
            .ok_or_else(|| FallaFuente::new(EstadoFuente::NoDisponible, "Falta el Team ID de xAI en la configuración."))?;
        let clave = credenciales::obtener_xai(&team).map_err(|e| FallaFuente::new(EstadoFuente::NoDisponible, e.to_string()))?
            .ok_or_else(|| FallaFuente::new(EstadoFuente::CredencialInvalida, "No hay una clave de gestión de xAI registrada. Regístrela en Configuración."))?;
        let accion = "La clave de gestión de xAI no es válida o fue revocada. Registre una nueva en Configuración.";
        let balance = http_json(ctx.http.get(format!("{BASE}/v1/billing/teams/{team}/prepaid/balance")).bearer_auth(&clave).send(), accion)?;
        let preview = http_json(ctx.http.get(format!("{BASE}/v1/billing/teams/{team}/postpaid/invoice/preview")).bearer_auth(&clave).send(), accion).ok();
        let m = mapear(&balance, preview.as_ref());
        if m.is_empty() {
            return Err(FallaFuente::new(EstadoFuente::FormatoNoReconocido, "La respuesta de facturación de xAI no tiene el formato esperado."));
        }
        Ok(Consulta { metricas: m, obtenida_en: None, detalle: Some("Fuente: Management API de xAI.".into()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saldo_y_gasto_en_dolares() {
        let b: Value = serde_json::from_str(include_str!("../../tests/fixtures/xai_balance.json")).unwrap();
        let p: Value = serde_json::from_str(include_str!("../../tests/fixtures/xai_preview.json")).unwrap();
        let m = mapear(&b, Some(&p));
        let s = m.iter().find(|x| x.nombre == "saldo_prepagado").unwrap();
        assert_eq!(s.valor, 7.5); // 10,00 comprados − 2,50 usados
        assert!(s.es_estimado);
        let g = m.iter().find(|x| x.nombre == "gasto_periodo").unwrap();
        assert_eq!(g.valor, 2.5);
        assert_eq!(g.limite, Some(200.0));
    }
}
