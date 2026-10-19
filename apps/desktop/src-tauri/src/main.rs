#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(err) = bitbot_desktop::run() {
        eprintln!("bitbot-desktop: {err:#}");
        std::process::exit(1);
    }
}
