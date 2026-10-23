import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { copyFile, mkdir, readFile, stat, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const source = resolve(root, ".tools/release-dictionaries");
const destination = resolve(root, ".local/android-assets/svl");
const hash = async (path) => {
  const digest = createHash("sha256");
  for await (const chunk of createReadStream(path)) digest.update(chunk);
  return digest.digest("hex");
};

try {
  const manifest = JSON.parse(await readFile(resolve(source, "manifest.json"), "utf8"));
  const files = [];
  for (const entry of manifest.files) {
    const path = resolve(source, entry.path);
    if (!(path.startsWith(source + "/") || path.startsWith(source + "\\"))) throw new Error("Invalid dictionary resource path.");
    if ((await stat(path)).size !== entry.size || await hash(path) !== entry.sha256) {
      throw new Error(`Dictionary resource does not match its manifest: ${entry.path}`);
    }
    const target = resolve(destination, "dictionaries", entry.path);
    await mkdir(dirname(target), { recursive: true });
    let existing = false;
    try { existing = (await stat(target)).size === entry.size && await hash(target) === entry.sha256; } catch {}
    if (!existing) await copyFile(path, target);
    files.push({ path: `dictionaries/${entry.path}`, sha256: entry.sha256, size: entry.size });
  }
  const originalManifest = resolve(source, "manifest.json");
  await copyFile(originalManifest, resolve(destination, "dictionaries/source-manifest.json"));
  files.push({ path: "dictionaries/source-manifest.json", sha256: await hash(originalManifest), size: (await stat(originalManifest)).size });
  await writeFile(resolve(destination, "manifest.json"), JSON.stringify({ schemaVersion: 1, files }, null, 2) + "\n");
  console.log(`Android offline assets ready: ${files.length} verified dictionary and notice files.`);
} catch (error) {
  console.error(`Android resources: ${error.message}\nPrepare the shared dictionaries with python -X utf8 scripts/prepare-base-dictionary.py first.`);
  process.exitCode = 1;
}
