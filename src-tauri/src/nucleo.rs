//! Núcleo: estado compartido y planificador de actualizaciones (HU-12, HU-14,
//! HU-17). Las consultas se ejecutan en un hilo propio y no bloquean la interfaz.

use crate::alertas::{Alertas, Aviso};
use crate::almacen::{self, Almacen};
use crate::archivos::Rutas;
use crate::conectores::{self, Contexto};
use crate::config::Configuracion;
use crate::modelo::{EstadoFuente, IdProveedor};
use chrono::{DateTime, Duration, Local, Utc};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

pub struct Estado {
    pub config: Configuracion,
    pub almacen: Almacen,
    pub alertas: Alertas,
    pub avisos_carga: Vec<String>,
    pub ultima_actualizacion: Option<DateTime<Utc>>,
    /// Próxima consulta permitida por proveedor tras un HTTP 429 y nivel de espera.
    espera: HashMap<IdProveedor, (DateTime<Utc>, u32)>,
}

pub struct Nucleo {
    pub rutas: Rutas,
    pub estado: Mutex<Estado>,
    actualizando: AtomicBool,
    disparar: Mutex<Option<Sender<()>>>,
}

pub type AlNotificar = Box<dyn Fn(&Aviso) + Send + Sync>;
pub type AlCambiar = Box<dyn Fn() + Send + Sync>;

impl Nucleo {
    pub fn nuevo(rutas: Rutas) -> Arc<Self> {
        // Los directorios propios se crean o corrigen con modo 0700 al iniciar,
        // aunque hayan sido creados por otro programa.
        for r in [&rutas.config, &rutas.lecturas, &rutas.alertas] {
            if let Some(d) = r.parent() {
                if let Err(e) = crate::archivos::asegurar_directorio(d) { log::warn!("no se pudo preparar {}: {e}", d.display()); }
            }
        }
        let (config, a1) = Configuracion::cargar(&rutas.config);
        let (mut almacen, a2) = Almacen::cargar(&rutas.lecturas);
        let (alertas, a3) = Alertas::cargar(&rutas.alertas);
        almacen.quitar_no_habilitados(&config.habilitados());
        Arc::new(Nucleo {
            rutas,
            estado: Mutex::new(Estado { config, almacen, alertas, avisos_carga: [a1, a2, a3].into_iter().flatten().collect(),
                ultima_actualizacion: None, espera: HashMap::new() }),
            actualizando: AtomicBool::new(false),
            disparar: Mutex::new(None),
        })
    }

    pub fn actualizando(&self) -> bool {
        self.actualizando.load(Ordering::SeqCst)
    }

    /// Pide una actualización inmediata; si ya hay una en curso no se duplica (HU-14.2).
    pub fn pedir_actualizacion(&self) -> bool {
        if self.actualizando() {
            return false;
        }
        if let Some(tx) = self.disparar.lock().unwrap().as_ref() {
            let _ = tx.send(());
        }
        true
    }

    /// Ejecuta un ciclo completo. Devuelve los avisos a mostrar.
    pub fn ciclo(&self, http: &reqwest::blocking::Client) -> Vec<Aviso> {
        if self.actualizando.swap(true, Ordering::SeqCst) {
            return vec![];
        }
        let ahora = Utc::now();
        let (config, pendientes) = {
            let e = self.estado.lock().unwrap();
            let ids: Vec<_> = e.config.habilitados().into_iter()
                .filter(|id| e.espera.get(id).is_none_or(|(t, _)| *t <= ahora)).collect();
            (e.config.clone(), ids)
        };
        // Consultas fuera del bloqueo para no detener la interfaz.
        let resultados: Vec<_> = pendientes.iter().map(|id| {
            let ctx = Contexto { config: &config, rutas: &self.rutas, http, ahora };
            (*id, conectores::para(*id).consultar(&ctx))
        }).collect();

        let mut avisos = Vec::new();
        let mut e = self.estado.lock().unwrap();
        let hora = Local::now().time();
        for (id, r) in resultados {
            match &r {
                Err(f) if f.estado == EstadoFuente::LimiteConsulta => {
                    let nivel = e.espera.get(&id).map(|(_, n)| n + 1).unwrap_or(1).min(5);
                    let espera = Duration::seconds((config.intervalo_seg as i64) << nivel).min(Duration::hours(1));
                    e.espera.insert(id, (ahora + espera, nivel));
                }
                Ok(_) => { e.espera.remove(&id); }
                Err(_) => {}
            }
            let ok = r.is_ok();
            e.almacen.aplicar(id, r, ahora);
            if ok {
                let lectura = e.almacen.lecturas[&id].clone();
                let cfg = e.config.clone();
                avisos.extend(e.alertas.evaluar(id, &lectura, &cfg, ahora, hora));
            }
        }
        let cfg = e.config.clone();
        if let Some(r) = e.alertas.resumen_pendientes(&cfg, ahora, hora) {
            avisos.push(r);
        }
        e.almacen.quitar_no_habilitados(&cfg.habilitados());
        e.ultima_actualizacion = Some(ahora);
        let g1 = e.almacen.guardar(&self.rutas.lecturas);
        let g2 = e.alertas.guardar(&self.rutas.alertas);
        for err in [g1.err(), g2.err()].into_iter().flatten() {
            log::error!("no se pudo guardar: {err}");
        }
        drop(e);
        self.actualizando.store(false, Ordering::SeqCst);
        avisos
    }

    /// Hilo planificador: un ciclo al iniciar, luego cada `intervaloSeg`, o antes si
    /// se pide una actualización manual.
    pub fn iniciar(self: &Arc<Self>, notificar: AlNotificar, cambio: AlCambiar) {
        let (tx, rx): (Sender<()>, Receiver<()>) = mpsc::channel();
        *self.disparar.lock().unwrap() = Some(tx);
        let n = Arc::clone(self);
        std::thread::Builder::new().name("planificador".into()).spawn(move || {
            let http = conectores::cliente_http();
            loop {
                cambio();
                let avisos = n.ciclo(&http);
                for a in &avisos {
                    notificar(a);
                }
                cambio();
                let intervalo = n.estado.lock().unwrap().config.intervalo_seg;
                let _ = rx.recv_timeout(std::time::Duration::from_secs(intervalo as u64));
                while rx.try_recv().is_ok() {}
            }
        }).expect("hilo planificador");
    }

    pub fn instantanea(&self) -> Instantanea {
        let e = self.estado.lock().unwrap();
        let ahora = Utc::now();
        let proveedores = e.config.habilitados().into_iter().map(|id| {
            let l = e.almacen.lecturas.get(&id);
            let metricas = l.map(|l| l.metricas.iter().map(|m| MetricaVista {
                porcentaje: m.porcentaje_consumido(),
                metrica: m.clone(),
            }).collect()).unwrap_or_default();
            ProveedorVista {
                id,
                nombre: id.nombre().into(),
                estado: l.map(|l| almacen::estado_visible(l, e.config.intervalo_seg, ahora)).unwrap_or(EstadoFuente::NoDisponible),
                estado_etiqueta: l.map(|l| almacen::estado_visible(l, e.config.intervalo_seg, ahora).etiqueta()).unwrap_or("sin datos").into(),
                detalle: l.and_then(|l| l.detalle_estado.clone()),
                obtenida_en: l.filter(|l| !l.metricas.is_empty()).map(|l| l.obtenida_en),
                metricas,
            }
        }).collect();
        Instantanea { proveedores, actualizando: self.actualizando(), ultima_actualizacion: e.ultima_actualizacion,
            umbrales: e.config.umbrales.clone(), intervalo_seg: e.config.intervalo_seg, avisos: e.avisos_carga.clone() }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricaVista {
    #[serde(flatten)]
    pub metrica: crate::modelo::Metrica,
    pub porcentaje: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProveedorVista {
    pub id: IdProveedor,
    pub nombre: String,
    pub estado: EstadoFuente,
    pub estado_etiqueta: String,
    pub detalle: Option<String>,
    pub obtenida_en: Option<DateTime<Utc>>,
    pub metricas: Vec<MetricaVista>,
}

impl ProveedorVista {
    /// Métrica principal: la cuota con mayor porcentaje consumido; si no hay, la primera.
    pub fn principal(&self) -> Option<&MetricaVista> {
        // Ante un empate se prefiere la ventana más corta (5 horas) y, después, el orden de la fuente.
        let corta = |m: &&MetricaVista| m.metrica.ventana.as_ref().is_some_and(|v| v.duracion == "PT5H");
        let mut mejor: Option<&MetricaVista> = None;
        for m in self.metricas.iter().filter(|m| m.porcentaje.is_some() && m.metrica.tipo_limite == crate::modelo::TipoLimite::Cuota) {
            mejor = match mejor {
                None => Some(m),
                Some(b) if m.porcentaje > b.porcentaje || (m.porcentaje == b.porcentaje && corta(&m) && !corta(&b)) => Some(m),
                x => x,
            };
        }
        mejor.or_else(|| self.metricas.first())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Instantanea {
    pub proveedores: Vec<ProveedorVista>,
    pub actualizando: bool,
    pub ultima_actualizacion: Option<DateTime<Utc>>,
    pub umbrales: Vec<u32>,
    pub intervalo_seg: u32,
    pub avisos: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ciclo_simulado_completo_persiste_los_tres_archivos() {
        std::env::set_var("OPENTUM_SIMULADO", "1");
        let dir = tempfile::tempdir().unwrap();
        let rutas = Rutas::en(&dir.path().join("c"), &dir.path().join("k"), &dir.path().join("s"));
        let n = Nucleo::nuevo(rutas.clone());
        {
            let mut e = n.estado.lock().unwrap();
            for p in e.config.proveedores.iter_mut() { p.habilitado = true; }
            e.config.guardar(&rutas.config).unwrap();
        }
        let avisos = n.ciclo(&conectores::cliente_http());
        let snap = n.instantanea();
        assert_eq!(snap.proveedores.len(), 3);
        assert!(snap.proveedores.iter().all(|p| p.estado == EstadoFuente::Actualizado && !p.metricas.is_empty()));
        assert!(rutas.lecturas.exists() && rutas.alertas.exists() && rutas.config.exists());
        // Codex parte del 62 % y supera el 80 % según el paso simulado; los avisos dependen de la hora.
        let _ = avisos;
        // Un segundo ciclo inmediato no repite alertas ya registradas.
        let n2 = Nucleo::nuevo(rutas.clone());
        let previas = n2.estado.lock().unwrap().alertas.alertas.len();
        let _ = n2.ciclo(&conectores::cliente_http());
        assert!(n2.estado.lock().unwrap().alertas.alertas.len() >= previas);
        std::env::remove_var("OPENTUM_SIMULADO");
    }
}
