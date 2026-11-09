//! Validación de los archivos JSON contra los esquemas del Anexo 3 (JSON Schema 2020-12).

use serde_json::Value;
use std::sync::OnceLock;

pub const CONFIG: &str = include_str!("../esquemas/config.schema.json");
pub const LECTURAS: &str = include_str!("../esquemas/lecturas.schema.json");
pub const ALERTAS: &str = include_str!("../esquemas/alertas.schema.json");

#[derive(Clone, Copy, Debug)]
pub enum Esquema {
    Config,
    Lecturas,
    Alertas,
}

fn validador(e: Esquema) -> &'static jsonschema::Validator {
    static C: OnceLock<jsonschema::Validator> = OnceLock::new();
    static L: OnceLock<jsonschema::Validator> = OnceLock::new();
    static A: OnceLock<jsonschema::Validator> = OnceLock::new();
    let (cell, src) = match e {
        Esquema::Config => (&C, CONFIG),
        Esquema::Lecturas => (&L, LECTURAS),
        Esquema::Alertas => (&A, ALERTAS),
    };
    cell.get_or_init(|| {
        let schema: Value = serde_json::from_str(src).expect("esquema embebido inválido");
        jsonschema::options()
            .with_draft(jsonschema::Draft::Draft202012)
            .should_validate_formats(true)
            .build(&schema)
            .expect("esquema embebido inválido")
    })
}

/// Devuelve el primer error de validación, o `Ok` si el documento cumple el esquema.
pub fn validar(e: Esquema, doc: &Value) -> Result<(), String> {
    match validador(e).iter_errors(doc).next() {
        None => Ok(()),
        Some(err) => Err(format!("{} en {}", err, err.instance_path)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rechaza_los_casos_del_anexo_3() {
        let base = json!({"version":2,"intervaloSeg":300,"umbrales":[80,95],
            "proveedores":[{"id":"xai","habilitado":true,"mecanismo":"API","credencialEn":"secret_service","teamId":"t"}]});
        assert!(validar(Esquema::Config, &base).is_ok());
        let mut c = base.clone(); c["intervaloSeg"] = json!(60);
        assert!(validar(Esquema::Config, &c).is_err());
        let mut c = base.clone(); c["proveedores"][0].as_object_mut().unwrap().remove("teamId");
        assert!(validar(Esquema::Config, &c).is_err());
        let mut c = base.clone(); c["horarioSilencio"] = json!({"desde":"24:30","hasta":"07:00"});
        assert!(validar(Esquema::Config, &c).is_err());

        let lect = json!({"codex":{"obtenidaEn":"2026-10-08T14:05:00-03:00","estado":"ACTUALIZADO","metricas":[
            {"nombre":"sesion_5h","valor":23.5,"sentido":"CONSUMIDO","unidad":"PORCENTAJE","limite":100,
             "tipoLimite":"CUOTA","origen":"API","esEstimado":false,"ventana":{"duracion":"PT5H"}}]}});
        assert!(validar(Esquema::Lecturas, &lect).is_ok());
        let mut l = lect.clone(); l["codex"]["estado"] = json!("OK");
        assert!(validar(Esquema::Lecturas, &l).is_err());
        let mut l = lect.clone(); l["codex"]["metricas"][0]["valor"] = json!(120);
        assert!(validar(Esquema::Lecturas, &l).is_err());
        let mut l = lect.clone(); l["codex"]["obtenidaEn"] = json!("2026-10-08T14:05:00");
        assert!(validar(Esquema::Lecturas, &l).is_err());
        let mut l = lect.clone(); l["codex"]["metricas"][0].as_object_mut().unwrap().remove("sentido");
        assert!(validar(Esquema::Lecturas, &l).is_err());
        let mut l = lect.clone(); l["codex"]["metricas"][0]["ventana"]["duracion"] = json!("5 horas");
        assert!(validar(Esquema::Lecturas, &l).is_err());

        let al = json!({"alertas":[{"clave":"k","proveedor":"codex","metrica":"m","tipo":"UMBRAL_CUOTA",
            "registradaEn":"2026-10-08T14:05:00Z"}]});
        assert!(validar(Esquema::Alertas, &al).is_err());
        let mut al = al.clone(); al["alertas"][0]["notificada"] = json!(true);
        assert!(validar(Esquema::Alertas, &al).is_ok());
    }
}
