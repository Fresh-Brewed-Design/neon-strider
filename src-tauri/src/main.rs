// Release builds have no console window (logs go to files; see logging.rs).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    neon_strider_lib::run()
}
