#!/usr/bin/env node
/**
 * Cross-platform Tauri launcher.
 * Ensures Cargo is on PATH and applies Linux WebKit workarounds, then runs the Tauri CLI.
 * Avoids depending on bash/WSL so `yarn tauri dev` works on Windows PowerShell.
 */
import { spawn, spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir, platform } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
process.chdir(root);

const home = homedir();
const cargoBin = join(home, ".cargo", "bin");
const pathSep = platform() === "win32" ? ";" : ":";
const pathParts = (process.env.PATH || "").split(pathSep).filter(Boolean);
if (!pathParts.some((p) => p.toLowerCase() === cargoBin.toLowerCase())) {
  process.env.PATH = `${cargoBin}${pathSep}${process.env.PATH || ""}`;
}

function commandExists(cmd) {
  if (platform() === "win32") {
    // Check common Windows locations without shell (avoids DEP0190).
    const exts = (process.env.PATHEXT || ".EXE;.CMD;.BAT").split(";").filter(Boolean);
    const names = [cmd, ...exts.map((e) => cmd + e)];
    for (const dir of (process.env.PATH || "").split(";")) {
      for (const name of names) {
        if (dir && existsSync(join(dir, name))) return true;
      }
    }
    return false;
  }
  const result = spawnSync("which", [cmd], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  });
  return result.status === 0;
}

if (!commandExists("cargo")) {
  console.error("error: cargo not found.");
  console.error(
    'Install Rust: https://rustup.rs/  (or: curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y)'
  );
  console.error(
    'Then open a new terminal (or ensure ~/.cargo/bin is on PATH) and retry.'
  );
  process.exit(127);
}

if (platform() === "linux" && !commandExists("pkg-config")) {
  console.warn("warning: pkg-config not found — Tauri may fail linking GTK/WebKit.");
  console.warn("On Ubuntu/Debian install:");
  console.warn(
    "  sudo apt update && sudo apt install -y pkg-config build-essential libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev libxdo-dev"
  );
}

// Linux WebKitGTK: avoid broken GPU/dmabuf paths that produce MESA/EGL spam
// and occasionally leave the webview at 1× scale on large monitors.
if (platform() === "linux") {
  process.env.WEBKIT_DISABLE_DMABUF_RENDERER ??= "1";
  process.env.XCURSOR_SIZE ??= "16";
  if (!existsSync("/dev/dri/renderD128") && !existsSync("/dev/dri/card0")) {
    process.env.LIBGL_ALWAYS_SOFTWARE ??= "1";
  }
}

const args = process.argv.slice(2);
if (args.length === 0) {
  args.push("dev");
}

// Prefer local CLI JS entry (cross-platform; avoids .cmd + shell on Windows).
const localTauriJs = join(root, "node_modules", "@tauri-apps", "cli", "tauri.js");
const useLocal = existsSync(localTauriJs);
const command = useLocal ? process.execPath : "npx";
const commandArgs = useLocal
  ? [localTauriJs, ...args]
  : ["tauri", ...args];

const child = spawn(command, commandArgs, {
  stdio: "inherit",
  env: process.env,
  shell: !useLocal && platform() === "win32",
});

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 1);
});

child.on("error", (err) => {
  console.error("error: failed to start Tauri CLI:", err.message);
  process.exit(1);
});
