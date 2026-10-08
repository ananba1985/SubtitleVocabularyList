import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2).filter((value) => value !== "--");
const action = args.shift() ?? "check";
const read = (path) => readFileSync(resolve(root, path), "utf8");
const files = {
  "package.json": read("package.json"),
  "src-tauri/tauri.conf.json": read("src-tauri/tauri.conf.json"),
  "src-tauri/Cargo.toml": read("src-tauri/Cargo.toml"),
  "src-tauri/Cargo.lock": read("src-tauri/Cargo.lock"),
};
const packageInfo = JSON.parse(files["package.json"]);
const cargoPackage = /(^\[package\]\r?\n[\s\S]*?^version\s*=\s*")([^"]+)(")/m;
const cargoLock =
  /(^\[\[package\]\]\r?\nname = "subtitle-vocabulary-list"\r?\nversion = ")([^"]+)(")/m;

function parse(value) {
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(value)) {
    throw new Error(`版本必须为三段数字：${value}`);
  }
  const parts = value.split(".").map(Number);
  if (parts.some((part) => !Number.isSafeInteger(part) || part > 65535)) {
    throw new Error("Windows 版本的每一段必须在 0–65535 之间。");
  }
  return parts;
}

function versions() {
  const cargo = files["src-tauri/Cargo.toml"].match(cargoPackage)?.[2];
  const lock = files["src-tauri/Cargo.lock"].match(cargoLock)?.[2];
  if (!cargo || !lock) throw new Error("未找到本项目的 Cargo 版本字段。");
  return {
    "package.json": packageInfo.version,
    "src-tauri/tauri.conf.json": JSON.parse(files["src-tauri/tauri.conf.json"])
      .version,
    "src-tauri/Cargo.toml": cargo,
    "src-tauri/Cargo.lock": lock,
  };
}

function check() {
  for (const [path, version] of Object.entries(versions())) {
    parse(version);
    if (version !== packageInfo.version) {
      throw new Error(
        `${path} 的版本 ${version} 与 package.json 的 ${packageInfo.version} 不一致；执行 pnpm version:sync。`,
      );
    }
  }
}

function jsonVersion(text, version) {
  const pattern = /(^\s*"version"\s*:\s*")[^"]+("\s*[,}])/m;
  if (!pattern.test(text)) throw new Error("未找到 JSON 顶层版本字段。");
  return text.replace(
    pattern,
    (_match, prefix, suffix) => prefix + version + suffix,
  );
}

try {
  const current = parse(packageInfo.version);
  let next = packageInfo.version;
  if (action === "check") {
    check();
    console.log(`Version OK: ${next}`);
  } else {
    if (!["patch", "minor", "set", "sync"].includes(action)) {
      throw new Error(
        "用法：version.mjs check|sync|patch|minor|set [版本] [--message 变更说明]",
      );
    }
    versions();
    if (action !== "sync") check();
    if (action === "patch")
      next = [current[0], current[1], current[2] + 1].join(".");
    if (action === "minor") next = [current[0], current[1] + 1, 0].join(".");
    if (action === "set") {
      next = args.shift();
      const parts = parse(next ?? "");
      const delta = parts.findIndex((part, index) => part !== current[index]);
      if (delta < 0 || parts[delta] < current[delta])
        throw new Error("手动设置的版本必须大于当前版本。");
    }
    parse(next);
    const messageIndex = args.indexOf("--message");
    const message = messageIndex >= 0 ? args[messageIndex + 1]?.trim() : "";
    if (action !== "sync" && (!message || /[\r\n]/.test(message))) {
      throw new Error("递增版本时请提供单行 --message 变更说明。");
    }
    const updates = {
      "package.json": jsonVersion(files["package.json"], next),
      "src-tauri/tauri.conf.json": jsonVersion(
        files["src-tauri/tauri.conf.json"],
        next,
      ),
      "src-tauri/Cargo.toml": files["src-tauri/Cargo.toml"].replace(
        cargoPackage,
        (_m, prefix, _old, suffix) => prefix + next + suffix,
      ),
      "src-tauri/Cargo.lock": files["src-tauri/Cargo.lock"].replace(
        cargoLock,
        (_m, prefix, _old, suffix) => prefix + next + suffix,
      ),
    };
    if (action !== "sync") {
      const changelog = read("CHANGELOG.md");
      const localDate = new Date();
      const date = `${localDate.getFullYear()}-${String(localDate.getMonth() + 1).padStart(2, "0")}-${String(localDate.getDate()).padStart(2, "0")}`;
      const index = changelog.indexOf("\n## ");
      if (index < 0) throw new Error("CHANGELOG.md 缺少版本记录区。");
      updates["CHANGELOG.md"] =
        `${changelog.slice(0, index).trimEnd()}\n\n## ${next}（开发中）— ${date}\n\n- ${message}\n${changelog.slice(index)}`;
    }
    for (const [path, text] of Object.entries(updates)) {
      if (text !== (files[path] ?? read(path)))
        writeFileSync(resolve(root, path), text, "utf8");
    }
    console.log(
      action === "sync"
        ? `Version synchronized: ${next}`
        : `Version updated: ${packageInfo.version} -> ${next}`,
    );
    console.log("未创建 Git 标签、提交或安装包。");
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
