//! openTUM: monitor de consumo de servicios de IA para el área de indicadores de
//! Ubuntu. Copyright (C) 2026 Sebastián Andrés Sanfilippo. Licencia GPL-3.0-or-later.

pub mod alertas;
pub mod almacen;
pub mod archivos;
pub mod conectores;
pub mod config;
pub mod credenciales;
pub mod esquemas;
pub mod modelo;
pub mod notificaciones;
pub mod nucleo;

use crate::config::Configuracion;
use crate::modelo::{EstadoFuente, IdProveedor};
use crate::nucleo::{Instantanea, Nucleo};
use std::sync::Arc;
use tauri::image::Image;
use tauri::menu::{IconMenuItemBuilder, Menu, MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

const TRAY_ID: &str = "opentum";

fn png(bytes: &'static [u8]) -> Image<'static> {
    Image::from_bytes(bytes).expect("ícono")
}

#[derive(Clone, Copy, PartialEq, PartialOrd)]
enum Nivel { Gris, Verde, Ambar, Rojo }

fn nivel(p: Option<f64>, umbrales: &[u32]) -> Nivel {
    let (u1, u2) = (*umbrales.first().unwrap_or(&80) as f64, *umbrales.last().unwrap_or(&95) as f64);
    match p {
        None => Nivel::Gris,
        Some(p) if p >= u2 => Nivel::Rojo,
        Some(p) if p >= u1 => Nivel::Ambar,
        Some(_) => Nivel::Verde,
    }
}

fn icono_tray(n: Nivel) -> Image<'static> {
    match n {
        Nivel::Gris => png(include_bytes!("../icons/tray-gris.png")),
        Nivel::Verde => png(include_bytes!("../icons/tray-verde.png")),
        Nivel::Ambar => png(include_bytes!("../icons/tray-ambar.png")),
        Nivel::Rojo => png(include_bytes!("../icons/tray-rojo.png")),
    }
}

fn icono_punto(n: Nivel) -> Image<'static> {
    match n {
        Nivel::Gris => png(include_bytes!("../icons/punto-gris.png")),
        Nivel::Verde => png(include_bytes!("../icons/punto-verde.png")),
        Nivel::Ambar => png(include_bytes!("../icons/punto-ambar.png")),
        Nivel::Rojo => png(include_bytes!("../icons/punto-rojo.png")),
    }
}

fn hora_local(t: chrono::DateTime<chrono::Utc>) -> String {
    let l = t.with_timezone(&chrono::Local);
    if l.date_naive() == chrono::Local::now().date_naive() { l.format("%H:%M").to_string() } else { l.format("%d/%m %H:%M").to_string() }
}

/// Línea de texto del menú nativo para un proveedor (HU-09).
pub fn linea_menu(p: &nucleo::ProveedorVista) -> String {
    let principal = p.principal().map(|m| {
        let valor = match m.metrica.unidad {
            modelo::Unidad::Porcentaje if m.metrica.sentido == modelo::Sentido::Disponible => format!("{:.0} % disponible", m.metrica.valor),
            modelo::Unidad::Porcentaje => format!("{:.0} %", m.metrica.valor),
            modelo::Unidad::Usd => format!("USD {:.2}", m.metrica.valor),
            _ => format!("{:.0}", m.metrica.valor),
        };
        let nombre = alertas::etiqueta(&m.metrica.nombre);
        let reinicio = m.metrica.ventana.as_ref().and_then(|v| v.reinicio_en).map(|r| format!(" · reinicio {}", hora_local(r))).unwrap_or_default();
        format!("{nombre} {valor}{reinicio}")
    });
    match (p.estado, principal) {
        (EstadoFuente::Actualizado, Some(t)) => format!("{} — {}", p.nombre, t),
        (e, Some(t)) => format!("{} — {} ({})", p.nombre, t, e.etiqueta()),
        (_, None) => format!("{} — {}", p.nombre, p.estado_etiqueta),
    }
}

fn construir_menu(app: &AppHandle, s: &Instantanea) -> tauri::Result<Menu<tauri::Wry>> {
    let mut b = MenuBuilder::new(app);
    if s.proveedores.is_empty() {
        b = b.item(&MenuItemBuilder::with_id("config", "Sin proveedores habilitados: abrir configuración…").build(app)?);
    }
    for p in &s.proveedores {
        let n = if p.metricas.is_empty() { Nivel::Gris } else { nivel(p.principal().and_then(|m| m.porcentaje), &s.umbrales) };
        b = b.item(&IconMenuItemBuilder::with_id(format!("detalle:{}", p.id.clave()), linea_menu(p)).icon(icono_punto(n)).build(app)?);
    }
    let pie = match (s.actualizando, s.ultima_actualizacion) {
        (true, _) => "Actualizando…".to_string(),
        (false, Some(t)) => format!("Actualizado a las {}", hora_local(t)),
        (false, None) => "Sin actualizar".into(),
    };
    b.separator()
        .item(&MenuItemBuilder::with_id("info", pie).enabled(false).build(app)?)
        .item(&MenuItemBuilder::with_id("panel", "Abrir panel…").build(app)?)
        .item(&MenuItemBuilder::with_id("actualizar", "Actualizar ahora").enabled(!s.actualizando).build(app)?)
        .item(&MenuItemBuilder::with_id("config", "Configuración…").build(app)?)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&MenuItemBuilder::with_id("salir", "Salir").build(app)?)
        .build()
}

fn refrescar(app: &AppHandle) {
    let n = app.state::<Arc<Nucleo>>();
    let s = n.instantanea();
    let peor = s.proveedores.iter().filter(|p| !p.metricas.is_empty())
        .map(|p| nivel(p.principal().and_then(|m| m.porcentaje), &s.umbrales))
        .fold(Nivel::Gris, |a, b| if b > a { b } else { a });
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_icon(Some(icono_tray(peor)));
        if let Ok(menu) = construir_menu(app, &s) {
            let _ = tray.set_menu(Some(menu));
        }
        let _ = tray.set_tooltip(Some("openTUM"));
    }
    let _ = app.emit("estado", s);
}

fn abrir(app: &AppHandle, etiqueta: &str, url: &str, titulo: &str, ancho: f64, alto: f64) {
    if let Some(w) = app.get_webview_window(etiqueta) {
        let _ = w.eval(&format!("window.location.replace('{url}')"));
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let _ = WebviewWindowBuilder::new(app, etiqueta, WebviewUrl::App(url.into()))
        .title(titulo).inner_size(ancho, alto).min_inner_size(420.0, 320.0).build();
}

fn al_menu(app: &AppHandle, id: &str) {
    match id {
        "panel" => abrir(app, "panel", "index.html", "openTUM — Panel", 560.0, 640.0),
        "config" => abrir(app, "config", "config.html", "openTUM — Configuración", 600.0, 700.0),
        "actualizar" => { app.state::<Arc<Nucleo>>().pedir_actualizacion(); }
        "salir" => app.exit(0),
        x if x.starts_with("detalle:") => {
            let prov = &x["detalle:".len()..];
            abrir(app, "detalle", &format!("detalle.html?id={prov}"), "openTUM — Detalle", 600.0, 640.0)
        }
        _ => {}
    }
}

// ---------- Comandos para las ventanas web ----------

#[tauri::command]
fn estado(n: State<Arc<Nucleo>>) -> Instantanea {
    n.instantanea()
}

#[tauri::command]
fn actualizar(n: State<Arc<Nucleo>>) -> bool {
    n.pedir_actualizacion()
}

#[tauri::command]
fn abrir_ventana(app: AppHandle, destino: String) {
    al_menu(&app, &destino);
}

#[tauri::command]
fn config_obtener(n: State<Arc<Nucleo>>) -> Configuracion {
    n.estado.lock().unwrap().config.clone()
}

#[tauri::command]
fn config_guardar(app: AppHandle, n: State<Arc<Nucleo>>, config: Configuracion) -> Result<(), String> {
    config.validar()?;
    // xai solo puede habilitarse con Team ID y clave registrada.
    config.guardar(&n.rutas.config)?;
    {
        let mut e = n.estado.lock().unwrap();
        let antes = e.config.habilitados();
        e.config = config.clone();
        e.almacen.quitar_no_habilitados(&config.habilitados());
        for id in antes.into_iter().filter(|id| !config.habilitados().contains(id)) {
            e.alertas.borrar_proveedor(id);
        }
    }
    n.pedir_actualizacion();
    refrescar(&app);
    Ok(())
}

#[tauri::command]
fn xai_registrar(n: State<Arc<Nucleo>>, team_id: String, clave: String) -> Result<String, String> {
    let team = team_id.trim().to_string();
    if team.is_empty() || team.len() > 64 {
        return Err("El Team ID debe tener entre 1 y 64 caracteres.".into());
    }
    credenciales::validar_formato_clave(&clave)?;
    conectores::xai::verificar(&conectores::cliente_http(), &team, &clave).map_err(|f| f.detalle)?;
    credenciales::guardar_xai(&team, &clave).map_err(|e| e.to_string())?;
    let mut e = n.estado.lock().unwrap();
    let mut c = e.config.clone();
    c.proveedores.retain(|p| p.id != IdProveedor::Xai);
    c.proveedores.push(config::Proveedor { id: IdProveedor::Xai, habilitado: true, mecanismo: config::Mecanismo::Api,
        credencial_en: Some("secret_service".into()), team_id: Some(team) });
    c.guardar(&n.rutas.config)?;
    e.config = c;
    drop(e);
    n.pedir_actualizacion();
    Ok("La clave se guardó en el llavero de la sesión.".into())
}

/// Elimina solo las claves con los atributos de la aplicación y sus archivos (HU-18).
#[tauri::command]
fn eliminar_datos(app: AppHandle, n: State<Arc<Nucleo>>) -> Result<String, String> {
    let borradas = credenciales::eliminar_todas().map_err(|e| e.to_string())?;
    for r in n.rutas.todas() {
        let _ = std::fs::remove_file(r);
    }
    {
        let mut e = n.estado.lock().unwrap();
        e.config = Configuracion::default();
        e.almacen = Default::default();
        e.alertas = Default::default();
    }
    refrescar(&app);
    Ok(format!("Se eliminaron {borradas} clave(s) del llavero y los archivos de la aplicación. Las sesiones de las CLI no se modificaron."))
}

pub fn run() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).try_init();
    let nucleo = Nucleo::nuevo(archivos::Rutas::xdg());
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            let destino = args.iter().position(|a| a == "--abrir").and_then(|i| args.get(i + 1)).cloned().unwrap_or_else(|| "panel".into());
            al_menu(app, &destino);
        }))
        .manage(Arc::clone(&nucleo))
        .invoke_handler(tauri::generate_handler![estado, actualizar, abrir_ventana, config_obtener, config_guardar, xai_registrar, eliminar_datos])
        .setup(move |app| {
            let handle = app.handle().clone();
            let s = nucleo.instantanea();
            let _tray: TrayIcon = TrayIconBuilder::with_id(TRAY_ID)
                .icon(icono_tray(Nivel::Gris))
                .menu(&construir_menu(&handle, &s)?)
                .on_menu_event(|app, ev| al_menu(app, ev.id().as_ref()))
                .build(app)?;
            let h1 = handle.clone();
            let h2 = handle.clone();
            nucleo.iniciar(
                Box::new(move |aviso| {
                    if let Err(e) = notificaciones::mostrar(&aviso.titulo, &aviso.cuerpo, aviso.urgente) {
                        log::warn!("notificación no entregada: {e}");
                    }
                    let _ = h1.emit("aviso", (&aviso.titulo, &aviso.cuerpo));
                }),
                Box::new(move || {
                    let h = h2.clone();
                    let _ = h2.run_on_main_thread(move || refrescar(&h));
                }),
            );
            // `opentum --abrir panel|config|detalle:<id>` abre una ventana al iniciar.
            let args: Vec<String> = std::env::args().collect();
            if let Some(i) = args.iter().position(|a| a == "--abrir") {
                if let Some(destino) = args.get(i + 1) { al_menu(&handle, destino); }
            } else if s.proveedores.is_empty() {
                al_menu(&handle, "config");
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error al iniciar openTUM")
        .run(|_app, ev| {
            // Cerrar las ventanas no termina la aplicación: sigue en el área de indicadores.
            if let tauri::RunEvent::ExitRequested { code: None, api, .. } = ev {
                api.prevent_exit();
            }
        });
}
