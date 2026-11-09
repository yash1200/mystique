// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `--mcp-stdio-bridge`: an external agent started this binary as its
    // MCP server. Bridge stdio to the local MCP server and exit when stdin
    // closes; no window, no Tauri.
    if let Some(code) = mystique_acp::bridge::run_if_requested() {
        std::process::exit(code);
    }
    mystique_lib::run()
}
