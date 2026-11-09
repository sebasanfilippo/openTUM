//! Gestor de alertas (HU-07, HU-13, HU-15 y HU-16). El registro de cada alerta se
//! persiste en `alertas.json`, de modo que no se repite tras reiniciar la aplicación.

use crate::archivos;
use crate::config::Configuracion;
use crate::esquemas::{self, Esquema};
use crate::modelo::{IdProveedor, Lectura, Metrica, Sentido, TipoLimite, Unidad};
use chrono::{DateTime, Duration, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TipoAlerta {
    UmbralCuota,
    SaldoBajo,
    ConsumoCreditos,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegistroAlerta {
    pub clave: String,
    pub proveedor: IdProveedor,
    pub metrica: String,
    pub tipo: TipoAlerta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub umbral: Option<f64>,
    pub registrada_en: DateTime<Utc>,
    pub notificada: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reinicio_ventana: Option<DateTime<Utc>>,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Alertas {
    pub alertas: Vec<RegistroAlerta>,
}

/// Notificación a entregar al servidor de notificaciones del escritorio.
#[derive(Debug, Clone, PartialEq)]
pub struct Aviso {
    pub titulo: String,
    pub cuerpo: String,
    pub urgente: bool,
}

fn duracion_iso(s: &str) -> Option<Duration> {
    // Subconjunto suficiente para las ventanas del modelo: PTnH, PnD, PTnM.
    let s = s.strip_prefix('P')?;
    if let Some(t) = s.strip_prefix('T') {
        if let Some(h) = t.strip_suffix('H') { return h.parse().ok().map(Duration::hours); }
        if let Some(m) = t.strip_suffix('M') { return m.parse().ok().map(Duration::minutes); }
        return None;
    }
    s.strip_suffix('D')?.parse().ok().map(Duration::days)
}

fn inicio_ventana(m: &Metrica) -> String {
    match &m.ventana {
        Some(v) => match (v.reinicio_en, duracion_iso(&v.duracion)) {
            (Some(r), Some(d)) => (r - d).format("%Y-%m-%dT%H:%MZ").to_string(),
            (Some(r), None) => r.format("%Y-%m-%dT%H:%MZ").to_string(),
            _ => "sin-reinicio".into(),
        },
        None => "sin-ventana".into(),
    }
}

fn fmt_hora(t: Option<DateTime<Utc>>) -> String {
    t.map(|t| format!(" Reinicio: {}.", t.with_timezone(&chrono::Local).format("%d/%m %H:%M")))
        .unwrap_or_default()
}

impl Alertas {
    pub fn cargar(ruta: &Path) -> (Self, Option<String>) {
        let Some(bytes) = archivos::leer(ruta) else { return (Self::default(), None) };
        let r = serde_json::from_slice::<serde_json::Value>(&bytes).map_err(|e| e.to_string()).and_then(|doc| {
            esquemas::validar(Esquema::Alertas, &doc)?;
            serde_json::from_value::<Alertas>(doc).map_err(|e| e.to_string())
        });
        match r {
            Ok(a) => (a, None),
            Err(e) => (Self::default(), Some(format!("alertas.json descartado: {e}"))),
        }
    }

    pub fn guardar(&self, ruta: &Path) -> Result<(), String> {
        let doc = serde_json::to_value(self).map_err(|e| e.to_string())?;
        esquemas::validar(Esquema::Alertas, &doc)?;
        archivos::escribir_atomico(ruta, &serde_json::to_vec_pretty(&doc).unwrap()).map_err(|e| e.to_string())
    }

    fn existe(&self, clave: &str) -> bool {
        self.alertas.iter().any(|a| a.clave == clave)
    }

    /// Descarta los registros cuya ventana ya se reinició; la alerta puede volver a
    /// emitirse en la ventana nueva y una pendiente de esa ventana se pierde (HU-15.4).
    pub fn purgar(&mut self, ahora: DateTime<Utc>) {
        self.alertas.retain(|a| a.reinicio_ventana.is_none_or(|r| r > ahora));
    }

    /// Evalúa una lectura actualizada y devuelve los avisos a entregar ahora.
    pub fn evaluar(&mut self, id: IdProveedor, lectura: &Lectura, cfg: &Configuracion, ahora: DateTime<Utc>, hora_local: NaiveTime) -> Vec<Aviso> {
        self.purgar(ahora);
        let silencio = cfg.horario_silencio.as_ref().is_some_and(|h| h.contiene(hora_local));
        let mut avisos = Vec::new();
        for m in &lectura.metricas {
            // Umbrales de cuota (HU-13): se registran todos los cruzados y se notifica el mayor.
            if let (true, Some(p)) = (m.tipo_limite == TipoLimite::Cuota, m.porcentaje_consumido()) {
                let inicio = inicio_ventana(m);
                let reinicio = m.ventana.as_ref().and_then(|v| v.reinicio_en);
                let mut nuevo_mayor = None;
                for &u in &cfg.umbrales {
                    if p + 1e-9 < u as f64 { continue; }
                    let clave = format!("{}|{}|{}|{}", id.clave(), m.nombre, u, inicio);
                    if self.existe(&clave) { continue; }
                    self.alertas.push(RegistroAlerta { clave, proveedor: id, metrica: m.nombre.clone(), tipo: TipoAlerta::UmbralCuota,
                        umbral: Some(u as f64), registrada_en: ahora, notificada: !silencio, reinicio_ventana: reinicio });
                    nuevo_mayor = Some(u);
                }
                if let (Some(u), false) = (nuevo_mayor, silencio) {
                    avisos.push(Aviso {
                        titulo: format!("{}: la cuota {} superó el {} %", id.nombre(), etiqueta(&m.nombre), u),
                        cuerpo: format!("Uso actual: {:.0} %.{}", p, fmt_hora(reinicio)),
                        urgente: u >= *cfg.umbrales.last().unwrap_or(&95),
                    });
                }
            }
            // Saldo prepagado en dólares (HU-07): una vez hasta que vuelva a superar el umbral.
            if m.tipo_limite == TipoLimite::Saldo && m.unidad == Unidad::Usd && m.sentido == Sentido::Disponible {
                if let Some(umbral) = cfg.umbral_saldo_usd {
                    let clave = format!("{}|{}|saldo|{}", id.clave(), m.nombre, umbral);
                    if m.valor <= umbral {
                        if !self.existe(&clave) {
                            self.alertas.push(RegistroAlerta { clave, proveedor: id, metrica: m.nombre.clone(), tipo: TipoAlerta::SaldoBajo,
                                umbral: Some(umbral), registrada_en: ahora, notificada: !silencio, reinicio_ventana: None });
                            if !silencio {
                                avisos.push(Aviso { titulo: format!("{}: saldo bajo", id.nombre()),
                                    cuerpo: format!("Saldo disponible: USD {:.2} (umbral USD {:.2}).", m.valor, umbral), urgente: true });
                            }
                        }
                    } else {
                        self.alertas.retain(|a| a.clave != clave);
                    }
                }
            }
            // Créditos adicionales (HU-16): aviso distinto de las alertas de cuota.
            if m.nombre == "creditos_adicionales" && m.sentido == Sentido::Consumido && m.valor > 0.0 {
                let periodo = m.ventana.as_ref().and_then(|v| v.reinicio_en).map(|r| r.format("%Y-%m").to_string())
                    .unwrap_or_else(|| ahora.format("%Y-%m").to_string());
                let clave = format!("{}|{}|creditos|{}", id.clave(), m.nombre, periodo);
                if !self.existe(&clave) {
                    self.alertas.push(RegistroAlerta { clave, proveedor: id, metrica: m.nombre.clone(), tipo: TipoAlerta::ConsumoCreditos,
                        umbral: None, registrada_en: ahora, notificada: !silencio, reinicio_ventana: m.ventana.as_ref().and_then(|v| v.reinicio_en) });
                    if !silencio {
                        let unidad = if m.unidad == Unidad::Usd { "USD" } else { "créditos" };
                        avisos.push(Aviso { titulo: format!("{}: se están usando créditos adicionales", id.nombre()),
                            cuerpo: format!("Gasto en créditos del período: {:.2} {}.", m.valor, unidad), urgente: true });
                    }
                }
            }
        }
        avisos
    }

    /// Al terminar el horario de silencio entrega un único resumen con las alertas
    /// pendientes que siguen vigentes, y las marca como notificadas (HU-15.3).
    pub fn resumen_pendientes(&mut self, cfg: &Configuracion, ahora: DateTime<Utc>, hora_local: NaiveTime) -> Option<Aviso> {
        if cfg.horario_silencio.as_ref().is_some_and(|h| h.contiene(hora_local)) {
            return None;
        }
        self.purgar(ahora);
        let pendientes: Vec<String> = self.alertas.iter().filter(|a| !a.notificada).map(|a| match a.tipo {
            TipoAlerta::UmbralCuota => format!("{}: {} ≥ {:.0} %", a.proveedor.nombre(), etiqueta(&a.metrica), a.umbral.unwrap_or(0.0)),
            TipoAlerta::SaldoBajo => format!("{}: saldo ≤ USD {:.2}", a.proveedor.nombre(), a.umbral.unwrap_or(0.0)),
            TipoAlerta::ConsumoCreditos => format!("{}: uso de créditos adicionales", a.proveedor.nombre()),
        }).collect();
        if pendientes.is_empty() {
            return None;
        }
        self.alertas.iter_mut().for_each(|a| a.notificada = true);
        Some(Aviso { titulo: format!("Resumen de {} alerta(s) del horario de silencio", pendientes.len()), cuerpo: pendientes.join("\n"), urgente: false })
    }

    pub fn borrar_proveedor(&mut self, id: IdProveedor) {
        self.alertas.retain(|a| a.proveedor != id);
    }
}

/// Nombre legible de una métrica para el menú y las notificaciones.
pub fn etiqueta(nombre: &str) -> String {
    let ventana = |base: &str| if base.ends_with("_5h") { "5 horas" } else if base.ends_with("_semanal") { "semanal" } else { "" };
    match nombre {
        "sesion_5h" => "5 horas".into(),
        "semanal" => "semanal".into(),
        "semanal_sonnet" => "semanal (Sonnet)".into(),
        "creditos_adicionales" => "créditos adicionales".into(),
        "saldo_creditos" => "saldo de créditos".into(),
        "saldo_prepagado" => "saldo prepagado".into(),
        "gasto_periodo" => "gasto del período".into(),
        n => {
            let grupo = n.trim_end_matches("_5h").trim_end_matches("_semanal");
            let g = match grupo { "gemini" => "Gemini".to_string(), "claude_gpt" => "Claude y GPT".to_string(),
                "gemini_4_argon" => "Gemini 4 Argon".to_string(), x => x.replace('_', " ") };
            match ventana(n) { "" => g, v => format!("{g} · {v}") }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::HorarioSilencio;
    use crate::modelo::{EstadoFuente, OrigenDato};

    fn lectura(p: f64, reinicio: DateTime<Utc>) -> Lectura {
        Lectura { obtenida_en: Utc::now(), estado: EstadoFuente::Actualizado, detalle_estado: None,
            metricas: vec![Metrica::cuota_porcentaje("sesion_5h", p, "PT5H", Some(reinicio), OrigenDato::Api)] }
    }
    fn h(s: &str) -> NaiveTime { NaiveTime::parse_from_str(s, "%H:%M").unwrap() }

    #[test]
    fn una_sola_notificacion_por_umbral_y_ventana() {
        let cfg = Configuracion::default();
        let mut a = Alertas::default();
        let ahora = Utc::now();
        let r = ahora + Duration::hours(2);
        assert_eq!(a.evaluar(IdProveedor::Codex, &lectura(82.0, r), &cfg, ahora, h("12:00")).len(), 1);
        assert!(a.evaluar(IdProveedor::Codex, &lectura(84.0, r), &cfg, ahora, h("12:05")).is_empty());
        let av = a.evaluar(IdProveedor::Codex, &lectura(96.0, r), &cfg, ahora, h("12:10"));
        assert_eq!(av.len(), 1);
        assert!(av[0].titulo.contains("95"));
    }

    #[test]
    fn el_registro_persiste_y_evita_repetir_tras_reiniciar() {
        let dir = tempfile::tempdir().unwrap();
        let ruta = dir.path().join("alertas.json");
        let cfg = Configuracion::default();
        let ahora = Utc::now();
        let r = ahora + Duration::hours(2);
        let mut a = Alertas::default();
        a.evaluar(IdProveedor::Codex, &lectura(85.0, r), &cfg, ahora, h("12:00"));
        a.guardar(&ruta).unwrap();
        let (mut b, aviso) = Alertas::cargar(&ruta);
        assert!(aviso.is_none());
        assert!(b.evaluar(IdProveedor::Codex, &lectura(85.0, r), &cfg, ahora, h("12:05")).is_empty());
    }

    #[test]
    fn el_reinicio_de_la_ventana_habilita_nuevas_alertas() {
        let cfg = Configuracion::default();
        let mut a = Alertas::default();
        let t0 = Utc::now();
        a.evaluar(IdProveedor::Codex, &lectura(85.0, t0 + Duration::minutes(10)), &cfg, t0, h("12:00"));
        let t1 = t0 + Duration::minutes(20);
        assert_eq!(a.evaluar(IdProveedor::Codex, &lectura(85.0, t1 + Duration::hours(5)), &cfg, t1, h("12:20")).len(), 1);
    }

    #[test]
    fn silencio_registra_pendiente_y_entrega_resumen() {
        let mut cfg = Configuracion::default();
        cfg.horario_silencio = Some(HorarioSilencio { desde: "22:00".into(), hasta: "07:00".into() });
        let mut a = Alertas::default();
        let ahora = Utc::now();
        let r = ahora + Duration::hours(4);
        assert!(a.evaluar(IdProveedor::Codex, &lectura(90.0, r), &cfg, ahora, h("23:00")).is_empty());
        assert!(a.resumen_pendientes(&cfg, ahora, h("23:30")).is_none());
        let res = a.resumen_pendientes(&cfg, ahora, h("07:01")).expect("resumen");
        assert!(res.cuerpo.contains("80"));
        assert!(a.resumen_pendientes(&cfg, ahora, h("07:06")).is_none());
    }

    #[test]
    fn pendiente_de_ventana_reiniciada_se_descarta() {
        let mut cfg = Configuracion::default();
        cfg.horario_silencio = Some(HorarioSilencio { desde: "22:00".into(), hasta: "07:00".into() });
        let mut a = Alertas::default();
        let t0 = Utc::now();
        a.evaluar(IdProveedor::Codex, &lectura(90.0, t0 + Duration::hours(1)), &cfg, t0, h("23:00"));
        assert!(a.resumen_pendientes(&cfg, t0 + Duration::hours(8), h("07:01")).is_none());
    }

    #[test]
    fn saldo_bajo_una_vez_hasta_recuperarse() {
        let mut cfg = Configuracion::default();
        cfg.umbral_saldo_usd = Some(5.0);
        let saldo = |v| Lectura { obtenida_en: Utc::now(), estado: EstadoFuente::Actualizado, detalle_estado: None, metricas: vec![Metrica {
            nombre: "saldo_prepagado".into(), valor: v, sentido: Sentido::Disponible, unidad: Unidad::Usd, limite: None,
            tipo_limite: TipoLimite::Saldo, origen: OrigenDato::Api, es_estimado: false, ventana: None }] };
        let mut a = Alertas::default();
        let t = Utc::now();
        assert_eq!(a.evaluar(IdProveedor::Xai, &saldo(5.0), &cfg, t, h("10:00")).len(), 1);
        assert!(a.evaluar(IdProveedor::Xai, &saldo(4.0), &cfg, t, h("10:05")).is_empty());
        assert!(a.evaluar(IdProveedor::Xai, &saldo(20.0), &cfg, t, h("10:10")).is_empty());
        assert_eq!(a.evaluar(IdProveedor::Xai, &saldo(3.0), &cfg, t, h("10:15")).len(), 1);
    }
}
