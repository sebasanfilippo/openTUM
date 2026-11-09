// Evita una consola adicional en Windows; en Linux no tiene efecto.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--statusline") {
        std::process::exit(opentum_lib::conectores::claude::ejecutar_statusline());
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("openTUM {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    opentum_lib::run();
}
