//! Notificaciones de escritorio por la interfaz org.freedesktop.Notifications.
//!
//! GNOME Shell asocia las notificaciones de una aplicación con la conexión D-Bus
//! que las envió y las retira cuando esa conexión se cierra. Por eso se usa una
//! única conexión que vive mientras corre la aplicación, en lugar de abrir una
//! conexión nueva por aviso: con conexiones de un solo uso, GNOME descartaba el
//! aviso en el mismo instante en que lo recibía.

use std::collections::HashMap;
use std::sync::Mutex;
use zbus::blocking::Connection;
use zbus::zvariant::Value;

/// Nombre de la aplicación y de su archivo .desktop (openTUM.desktop).
const APP: &str = "openTUM";

static CONEXION: Mutex<Option<Connection>> = Mutex::new(None);

/// Muestra un aviso y devuelve el identificador que asignó el servidor.
pub fn mostrar(titulo: &str, cuerpo: &str, urgente: bool) -> zbus::Result<u32> {
    let mut guardada = CONEXION.lock().unwrap_or_else(|e| e.into_inner());
    if guardada.is_none() {
        *guardada = Some(Connection::session()?);
    }
    let conexion = guardada.as_ref().expect("conexión inicializada");
    let mut hints: HashMap<&str, Value> = HashMap::new();
    hints.insert("urgency", Value::U8(if urgente { 2 } else { 1 }));
    hints.insert("desktop-entry", Value::from(APP));
    let acciones: Vec<&str> = Vec::new();
    let resultado = conexion
        .call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &(APP, 0u32, "dialog-information", titulo, cuerpo, acciones, hints, -1i32),
        )
        .and_then(|r| r.body().deserialize::<u32>());
    if resultado.is_err() {
        // Si la sesión de D-Bus se reinició, el próximo aviso abre una conexión nueva.
        *guardada = None;
    }
    resultado
}
