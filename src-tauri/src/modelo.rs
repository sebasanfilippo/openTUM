//! Modelo común de métricas (HU-04). Los nombres siguen el diccionario de datos
//! del trabajo final y el esquema `lecturas.schema.json`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum IdProveedor {
    ClaudeCode,
    Codex,
    Xai,
    Antigravity,
}

impl IdProveedor {
    pub const TODOS: [IdProveedor; 4] = [Self::ClaudeCode, Self::Codex, Self::Xai, Self::Antigravity];

    pub fn clave(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude_code",
            Self::Codex => "codex",
            Self::Xai => "xai",
            Self::Antigravity => "antigravity",
        }
    }

    pub fn nombre(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
            Self::Xai => "xAI (Grok)",
            Self::Antigravity => "Antigravity",
        }
    }

    pub fn desde_clave(s: &str) -> Option<Self> {
        Self::TODOS.into_iter().find(|p| p.clave() == s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EstadoFuente {
    Actualizado,
    Desactualizado,
    CredencialInvalida,
    PermisoInsuficiente,
    LimiteConsulta,
    SinConexion,
    FormatoNoReconocido,
    NoDisponible,
}

impl EstadoFuente {
    pub fn etiqueta(self) -> &'static str {
        match self {
            Self::Actualizado => "actualizado",
            Self::Desactualizado => "desactualizado",
            Self::CredencialInvalida => "credencial inválida",
            Self::PermisoInsuficiente => "permiso insuficiente",
            Self::LimiteConsulta => "límite de consulta",
            Self::SinConexion => "sin conexión",
            Self::FormatoNoReconocido => "formato no reconocido",
            Self::NoDisponible => "no disponible",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Sentido {
    Consumido,
    Disponible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Unidad {
    Porcentaje,
    Tokens,
    Solicitudes,
    Creditos,
    Usd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TipoLimite {
    Cuota,
    LimiteTasa,
    TopeGasto,
    Saldo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrigenDato {
    Cli,
    ArchivoLocal,
    Api,
    Cabecera,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Ventana {
    /// Duración ISO 8601, por ejemplo PT5H o P7D.
    pub duracion: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reinicio_en: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Metrica {
    pub nombre: String,
    pub valor: f64,
    pub sentido: Sentido,
    pub unidad: Unidad,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limite: Option<f64>,
    pub tipo_limite: TipoLimite,
    pub origen: OrigenDato,
    pub es_estimado: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ventana: Option<Ventana>,
}

impl Metrica {
    /// Cuota en porcentaje consumido informada por la fuente.
    pub fn cuota_porcentaje(nombre: &str, usado: f64, duracion: &str, reinicio: Option<DateTime<Utc>>, origen: OrigenDato) -> Self {
        Metrica {
            nombre: nombre.into(),
            valor: usado.clamp(0.0, 100.0),
            sentido: Sentido::Consumido,
            unidad: Unidad::Porcentaje,
            limite: Some(100.0),
            tipo_limite: TipoLimite::Cuota,
            origen,
            es_estimado: false,
            ventana: Some(Ventana { duracion: duracion.into(), reinicio_en: reinicio }),
        }
    }

    /// Porcentaje consumido sin convertir unidades (regla de la Estructura de Datos).
    /// Devuelve `None` cuando la métrica no tiene un límite en su misma unidad.
    pub fn porcentaje_consumido(&self) -> Option<f64> {
        let p = match (self.unidad, self.sentido) {
            (Unidad::Porcentaje, Sentido::Consumido) => self.valor,
            (Unidad::Porcentaje, Sentido::Disponible) => 100.0 - self.valor,
            (_, sentido) => {
                let limite = self.limite.filter(|l| *l > 0.0)?;
                let consumido = match sentido {
                    Sentido::Consumido => self.valor,
                    Sentido::Disponible => limite - self.valor,
                };
                consumido / limite * 100.0
            }
        };
        Some(p.clamp(0.0, 100.0))
    }

    pub fn es_cuota(&self) -> bool {
        self.tipo_limite == TipoLimite::Cuota && self.porcentaje_consumido().is_some()
    }
}

/// Última lectura de un proveedor, tal como se guarda en `lecturas.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Lectura {
    pub obtenida_en: DateTime<Utc>,
    pub estado: EstadoFuente,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detalle_estado: Option<String>,
    pub metricas: Vec<Metrica>,
}

/// Resultado de una consulta fallida: estado de la fuente y acción sugerida.
#[derive(Debug, Clone, PartialEq)]
pub struct FallaFuente {
    pub estado: EstadoFuente,
    pub detalle: String,
}

impl FallaFuente {
    pub fn new(estado: EstadoFuente, detalle: impl Into<String>) -> Self {
        let mut detalle: String = detalle.into();
        if detalle.chars().count() > 200 {
            detalle = detalle.chars().take(197).collect::<String>() + "...";
        }
        Self { estado, detalle }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(valor: f64, sentido: Sentido, unidad: Unidad, limite: Option<f64>) -> Metrica {
        Metrica { nombre: "x".into(), valor, sentido, unidad, limite, tipo_limite: TipoLimite::Cuota,
                  origen: OrigenDato::Api, es_estimado: false, ventana: None }
    }

    #[test]
    fn porcentaje_consumido_respeta_el_sentido() {
        assert_eq!(m(23.5, Sentido::Consumido, Unidad::Porcentaje, None).porcentaje_consumido(), Some(23.5));
        assert_eq!(m(80.0, Sentido::Disponible, Unidad::Porcentaje, None).porcentaje_consumido(), Some(20.0));
    }

    #[test]
    fn otras_unidades_requieren_limite_en_la_misma_unidad() {
        assert_eq!(m(500.0, Sentido::Consumido, Unidad::Tokens, None).porcentaje_consumido(), None);
        assert_eq!(m(250.0, Sentido::Consumido, Unidad::Creditos, Some(1000.0)).porcentaje_consumido(), Some(25.0));
        assert_eq!(m(750.0, Sentido::Disponible, Unidad::Creditos, Some(1000.0)).porcentaje_consumido(), Some(25.0));
    }

    #[test]
    fn serializa_con_los_nombres_del_esquema() {
        let met = Metrica::cuota_porcentaje("sesion_5h", 23.5, "PT5H", None, OrigenDato::ArchivoLocal);
        let v = serde_json::to_value(&met).unwrap();
        assert_eq!(v["tipoLimite"], "CUOTA");
        assert_eq!(v["origen"], "ARCHIVO_LOCAL");
        assert_eq!(v["esEstimado"], false);
        assert_eq!(v["ventana"]["duracion"], "PT5H");
        assert_eq!(serde_json::to_value(IdProveedor::ClaudeCode).unwrap(), "claude_code");
    }
}
