import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const write = process.argv.includes("--write");
const maintenance = spawnSync("python", ["-X", "utf8", "scripts/cargo-cache.py"], {
  cwd: root,
  stdio: "inherit",
});
if (maintenance.error || maintenance.status !== 0)
  console.warn("Cargo cache maintenance deferred; continuing IPC check.");
const result = spawnSync(
  "cargo",
  [
    "run",
    "--quiet",
    "--locked",
    "--manifest-path",
    "src-tauri/Cargo.toml",
    "--example",
    "export_ipc",
    "--features",
    "ipc-types",
    "--",
    write ? "--write" : "--check",
  ],
  { cwd: root, stdio: "inherit" },
);
if (result.error) console.error(result.error.message);
process.exit(result.status ?? 1);
