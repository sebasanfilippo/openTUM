//! `config.json`: estructura, valores por omisión y reglas entre campos (HU-01, HU-15, HU-17).

use crate::archivos;
use crate::esquemas::{self, Esquema};
use crate::modelo::IdProveedor;
use chrono::NaiveTime;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const INTERVALO_MIN: u32 = 120;
pub const INTERVALO_MAX: u32 = 3600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Mecanismo {
    Cli,
    ArchivoLocal,
    Api,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Proveedor {
    pub id: IdProveedor,
    pub habilitado: bool,
    pub mecanismo: Mecanismo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credencial_en: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HorarioSilencio {
    pub desde: String,
    pub hasta: String,
}

impl HorarioSilencio {
    /// Indica si la hora local cae dentro de la franja; si `hasta` es anterior a
    /// `desde`, la franja cruza la medianoche.
    pub fn contiene(&self, ahora: NaiveTime) -> bool {
        let (Ok(d), Ok(h)) = (NaiveTime::parse_from_str(&self.desde, "%H:%M"), NaiveTime::parse_from_str(&self.hasta, "%H:%M")) else {
            return false;
        };
        if d < h { ahora >= d && ahora < h } else { ahora >= d || ahora < h }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Configuracion {
    pub version: u32,
    pub intervalo_seg: u32,
    pub umbrales: Vec<u32>,
    #[serde(rename = "umbralSaldoUSD", skip_serializing_if = "Option::is_none")]
    pub umbral_saldo_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub horario_silencio: Option<HorarioSilencio>,
    pub proveedores: Vec<Proveedor>,
}

impl Default for Configuracion {
    fn default() -> Self {
        let p = |id, mecanismo| Proveedor { id, habilitado: false, mecanismo, credencial_en: None, team_id: None };
        Configuracion {
            version: 2,
            intervalo_seg: 300,
            umbrales: vec![80, 95],
            umbral_saldo_usd: None,
            horario_silencio: None,
            proveedores: vec![
                p(IdProveedor::ClaudeCode, Mecanismo::ArchivoLocal),
                p(IdProveedor::Codex, Mecanismo::Cli),
                p(IdProveedor::Antigravity, Mecanismo::Cli),
            ],
        }
    }
}

impl Configuracion {
    pub fn proveedor(&self, id: IdProveedor) -> Option<&Proveedor> {
        self.proveedores.iter().find(|p| p.id == id)
    }

    pub fn habilitados(&self) -> Vec<IdProveedor> {
        self.proveedores.iter().filter(|p| p.habilitado).map(|p| p.id).collect()
    }

    /// Valida contra el esquema y aplica las tres reglas que el esquema no expresa.
    pub fn validar(&self) -> Result<(), String> {
        let doc = serde_json::to_value(self).map_err(|e| e.to_string())?;
        esquemas::validar(Esquema::Config, &doc)?;
        reglas(self)
    }

    pub fn cargar(ruta: &Path) -> (Self, Option<String>) {
        let Some(bytes) = archivos::leer(ruta) else {
            return (Self::default(), None);
        };
        let resultado = serde_json::from_slice::<serde_json::Value>(&bytes)
            .map_err(|e| format!("JSON inválido: {e}"))
            .and_then(|doc| {
                esquemas::validar(Esquema::Config, &doc)?;
                let c: Configuracion = serde_json::from_value(doc).map_err(|e| e.to_string())?;
                reglas(&c)?;
                Ok(c)
            });
        match resultado {
            Ok(c) => (c, None),
            Err(e) => (Self::default(), Some(format!("config.json descartado: {e}"))),
        }
    }

    pub fn guardar(&self, ruta: &Path) -> Result<(), String> {
        self.validar()?;
        let datos = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        archivos::escribir_atomico(ruta, &datos).map_err(|e| e.to_string())
    }
}

fn reglas(c: &Configuracion) -> Result<(), String> {
    let mut ids: Vec<_> = c.proveedores.iter().map(|p| p.id).collect();
    ids.sort();
    if ids.windows(2).any(|w| w[0] == w[1]) {
        return Err("cada proveedor debe aparecer una sola vez".into());
    }
    if c.umbrales.windows(2).any(|w| w[0] >= w[1]) {
        return Err("los umbrales deben ordenarse de menor a mayor".into());
    }
    if let Some(h) = &c.horario_silencio {
        if h.desde == h.hasta {
            return Err("el horario de silencio no puede empezar y terminar a la misma hora".into());
        }
    }
    if !(INTERVALO_MIN..=INTERVALO_MAX).contains(&c.intervalo_seg) {
        return Err(format!("el intervalo debe estar entre {} y {} minutos", INTERVALO_MIN / 60, INTERVALO_MAX / 60));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_configuracion_por_omision_es_valida() {
        assert!(Configuracion::default().validar().is_ok());
    }

    #[test]
    fn reglas_entre_campos() {
        let mut c = Configuracion::default();
        c.umbrales = vec![95, 80];
        assert!(c.validar().is_err());
        let mut c = Configuracion::default();
        c.proveedores.push(c.proveedores[0].clone());
        assert!(c.validar().is_err());
        let mut c = Configuracion::default();
        c.horario_silencio = Some(HorarioSilencio { desde: "22:00".into(), hasta: "22:00".into() });
        assert!(c.validar().is_err());
        let mut c = Configuracion::default();
        c.intervalo_seg = 60;
        assert!(c.validar().is_err());
    }

    #[test]
    fn horario_que_cruza_la_medianoche() {
        let h = HorarioSilencio { desde: "22:00".into(), hasta: "07:00".into() };
        let t = |s| NaiveTime::parse_from_str(s, "%H:%M").unwrap();
        assert!(h.contiene(t("23:30")));
        assert!(h.contiene(t("06:59")));
        assert!(!h.contiene(t("07:00")));
        assert!(!h.contiene(t("12:00")));
    }

    #[test]
    fn archivo_invalido_se_descarta() {
        let dir = tempfile::tempdir().unwrap();
        let ruta = dir.path().join("config.json");
        std::fs::write(&ruta, br#"{"version":2,"intervaloSeg":60,"umbrales":[80],"proveedores":[]}"#).unwrap();
        let (c, aviso) = Configuracion::cargar(&ruta);
        assert_eq!(c, Configuracion::default());
        assert!(aviso.is_some());
    }
}
