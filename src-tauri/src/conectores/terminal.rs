//! Ejecución de una CLI interactiva en una pseudoterminal y lectura de su pantalla.
//! Se usa para el panel `/status` de Codex, que solo existe en la interfaz
//! interactiva (riesgo R02).

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const FILAS: u16 = 45;
const COLUMNAS: u16 = 130;

/// Busca un ejecutable en PATH y en las ubicaciones habituales de instalación por
/// usuario, que una aplicación iniciada desde el menú puede no tener en su PATH.
pub fn ubicar(nombre: &str) -> Option<PathBuf> {
    let home = crate::archivos::home();
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    for extra in [".local/bin", ".npm-global/bin", "bin", ".cargo/bin"] {
        dirs.push(home.join(extra));
    }
    dirs.into_iter().map(|d| d.join(nombre)).find(|p| p.is_file())
}

/// Abre `programa` en una pseudoterminal, espera a que la pantalla contenga
/// `listo`, escribe `comando` y devuelve la pantalla cuando aparece `fin` o vence
/// el plazo. El proceso se termina siempre al salir.
pub fn pantalla_tras_comando(programa: &PathBuf, cwd: &PathBuf, listo: &str, comando: &str, fin: &str, plazo: Duration) -> Option<String> {
    let pty = native_pty_system().openpty(PtySize { rows: FILAS, cols: COLUMNAS, pixel_width: 0, pixel_height: 0 }).ok()?;
    let mut cmd = CommandBuilder::new(programa);
    cmd.cwd(cwd);
    cmd.env("TERM", "xterm-256color");
    let mut hijo = pty.slave.spawn_command(cmd).ok()?;
    drop(pty.slave);
    let mut lector = pty.master.try_clone_reader().ok()?;
    let mut escritor = pty.master.take_writer().ok()?;
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while let Ok(n) = lector.read(&mut buf) {
            if n == 0 || tx.send(buf[..n].to_vec()).is_err() { break; }
        }
    });
    let mut parser = vt100::Parser::new(FILAS, COLUMNAS, 0);
    let inicio = Instant::now();
    let mut enviado = false;
    let mut resultado = None;
    while inicio.elapsed() < plazo {
        if let Ok(datos) = rx.recv_timeout(Duration::from_millis(200)) {
            parser.process(&datos);
        }
        let texto = parser.screen().contents();
        if !enviado && texto.contains(listo) {
            std::thread::sleep(Duration::from_millis(500));
            let _ = escritor.write_all(comando.as_bytes());
            let _ = escritor.flush();
            std::thread::sleep(Duration::from_millis(1500));
            let _ = escritor.write_all(b"\r");
            let _ = escritor.flush();
            enviado = true;
        } else if enviado && texto.contains(fin) {
            // Un instante más para que termine de dibujar la línea.
            let limite = Instant::now() + Duration::from_millis(800);
            while Instant::now() < limite {
                if let Ok(d) = rx.recv_timeout(Duration::from_millis(100)) { parser.process(&d); }
            }
            resultado = Some(parser.screen().contents());
            break;
        }
    }
    let _ = hijo.kill();
    let _ = hijo.wait();
    resultado
}
