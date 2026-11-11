// Windows release builds: no console window (debug keeps it for logs).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    halo::run();
}
