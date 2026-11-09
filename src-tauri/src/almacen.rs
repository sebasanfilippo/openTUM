//! Almacén de lecturas: conserva la última lectura válida por proveedor en
//! `lecturas.json` (HU-11 y HU-12).

use crate::archivos;
use crate::esquemas::{self, Esquema};
use crate::modelo::{EstadoFuente, FallaFuente, IdProveedor, Lectura, Metrica};
use chrono::{DateTime, Duration, Utc};
use std::collections::BTreeMap;
use std::path::Path;

/// Resultado exitoso de un conector.
#[derive(Debug, Clone)]
pub struct Consulta {
    pub metricas: Vec<Metrica>,
    /// Momento al que corresponde el dato cuando la fuente lo informa (por ejemplo,
    /// la última escritura de la línea de estado). Si es `None`, se usa la hora actual.
    pub obtenida_en: Option<DateTime<Utc>>,
    pub detalle: Option<String>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Almacen {
    pub lecturas: BTreeMap<IdProveedor, Lectura>,
}

impl Almacen {
    pub fn cargar(ruta: &Path) -> (Self, Option<String>) {
        let Some(bytes) = archivos::leer(ruta) else {
            return (Self::default(), None);
        };
        let r = serde_json::from_slice::<serde_json::Value>(&bytes)
            .map_err(|e| e.to_string())
            .and_then(|doc| {
                esquemas::validar(Esquema::Lecturas, &doc)?;
                serde_json::from_value::<BTreeMap<IdProveedor, Lectura>>(doc).map_err(|e| e.to_string())
            });
        match r {
            Ok(lecturas) => (Almacen { lecturas }, None),
            Err(e) => (Self::default(), Some(format!("lecturas.json descartado: {e}"))),
        }
    }

    pub fn guardar(&self, ruta: &Path) -> Result<(), String> {
        let doc = serde_json::to_value(&self.lecturas).map_err(|e| e.to_string())?;
        esquemas::validar(Esquema::Lecturas, &doc)?;
        archivos::escribir_atomico(ruta, &serde_json::to_vec_pretty(&doc).unwrap()).map_err(|e| e.to_string())
    }

    /// Incorpora el resultado de una consulta. Ante una falla se conserva la última
    /// lectura válida con su marca temporal y solo cambian el estado y el detalle.
    pub fn aplicar(&mut self, id: IdProveedor, resultado: Result<Consulta, FallaFuente>, ahora: DateTime<Utc>) {
        match resultado {
            Ok(c) => {
                self.lecturas.insert(id, Lectura {
                    obtenida_en: c.obtenida_en.unwrap_or(ahora),
                    estado: EstadoFuente::Actualizado,
                    detalle_estado: c.detalle,
                    metricas: c.metricas,
                });
            }
            Err(f) => {
                let previa = self.lecturas.remove(&id);
                let lectura = match previa {
                    Some(mut l) if !l.metricas.is_empty() => {
                        l.estado = f.estado;
                        l.detalle_estado = Some(f.detalle);
                        l
                    }
                    _ => Lectura { obtenida_en: ahora, estado: f.estado, detalle_estado: Some(f.detalle), metricas: vec![] },
                };
                self.lecturas.insert(id, lectura);
            }
        }
    }

    pub fn quitar_no_habilitados(&mut self, habilitados: &[IdProveedor]) {
        self.lecturas.retain(|id, _| habilitados.contains(id));
    }
}

/// Estado para mostrar: una lectura actualizada con más de dos intervalos de
/// antigüedad se presenta como desactualizada (HU-10).
pub fn estado_visible(l: &Lectura, intervalo_seg: u32, ahora: DateTime<Utc>) -> EstadoFuente {
    if l.estado == EstadoFuente::Actualizado && ahora - l.obtenida_en > Duration::seconds(2 * intervalo_seg as i64) {
        EstadoFuente::Desactualizado
    } else {
        l.estado
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modelo::OrigenDato;

    fn consulta(v: f64) -> Consulta {
        Consulta { metricas: vec![Metrica::cuota_porcentaje("sesion_5h", v, "PT5H", None, OrigenDato::Api)], obtenida_en: None, detalle: None }
    }

    #[test]
    fn una_falla_conserva_el_ultimo_valor_valido() {
        let mut a = Almacen::default();
        let t0 = Utc::now();
        a.aplicar(IdProveedor::Codex, Ok(consulta(40.0)), t0);
        a.aplicar(IdProveedor::Codex, Err(FallaFuente::new(EstadoFuente::LimiteConsulta, "429")), t0 + Duration::minutes(5));
        let l = &a.lecturas[&IdProveedor::Codex];
        assert_eq!(l.estado, EstadoFuente::LimiteConsulta);
        assert_eq!(l.obtenida_en, t0);
        assert_eq!(l.metricas[0].valor, 40.0);
    }

    #[test]
    fn una_falla_sin_lectura_previa_no_inventa_valores() {
        let mut a = Almacen::default();
        a.aplicar(IdProveedor::Xai, Err(FallaFuente::new(EstadoFuente::SinConexion, "sin red")), Utc::now());
        assert!(a.lecturas[&IdProveedor::Xai].metricas.is_empty());
    }

    #[test]
    fn persiste_y_valida_contra_el_esquema() {
        let dir = tempfile::tempdir().unwrap();
        let ruta = dir.path().join("lecturas.json");
        let mut a = Almacen::default();
        a.aplicar(IdProveedor::ClaudeCode, Ok(consulta(23.5)), Utc::now());
        a.guardar(&ruta).unwrap();
        let (b, aviso) = Almacen::cargar(&ruta);
        assert!(aviso.is_none());
        assert_eq!(a, b);
    }

    #[test]
    fn dato_antiguo_se_muestra_desactualizado() {
        let mut a = Almacen::default();
        let t0 = Utc::now() - Duration::minutes(30);
        a.aplicar(IdProveedor::Codex, Ok(consulta(10.0)), t0);
        assert_eq!(estado_visible(&a.lecturas[&IdProveedor::Codex], 300, Utc::now()), EstadoFuente::Desactualizado);
    }
}
