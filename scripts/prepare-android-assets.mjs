import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { copyFile, mkdir, readFile, rename, stat, writeFile } from "node:fs/promises";
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
  async function ensureResource(path, url, expected, fallback) {
    const target = resolve(root,path);
    try { await stat(target); return; } catch {}
    await mkdir(dirname(target),{recursive:true});
    if (fallback) { try { await copyFile(resolve(root,fallback),target); return; } catch {} }
    const response = await fetch(url);
    if (!response.ok) throw new Error(`Unable to prepare fixed OCR resource: ${path}`);
    const bytes = Buffer.from(await response.arrayBuffer());
    if (createHash("sha256").update(bytes).digest("hex") !== expected) throw new Error(`OCR download checksum mismatch: ${path}`);
    await writeFile(target+".download",bytes); await rename(target+".download",target);
  }
  await ensureResource(".tools/release-cache/eng-best-4.1.0.traineddata","https://raw.githubusercontent.com/tesseract-ocr/tessdata_best/4.1.0/eng.traineddata","8280aed0782fe27257a68ea10fe7ef324ca0f8d85bd2fd145d1c2b560bcb66ba");
  await ensureResource(".tools/release-cache/tessdata-best-4.1.0-LICENSE.txt","https://raw.githubusercontent.com/tesseract-ocr/tessdata_best/4.1.0/LICENSE","a6cba85bc92e0cff7a450b1d873c0eaa2e9fc96bf472df0247a26bec77bf3ff9",".tools/release-resources/tesseract/licenses/English-data-LICENSE.txt");
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
  const ocr = [
    [".tools/release-cache/eng-best-4.1.0.traineddata", "ocr/tessdata/eng.traineddata", "8280aed0782fe27257a68ea10fe7ef324ca0f8d85bd2fd145d1c2b560bcb66ba"],
    [".tools/release-cache/sources/tesseract-5.5.3/LICENSE", "ocr/licenses/Tesseract-LICENSE.txt"],
    [".tools/release-cache/sources/leptonica-1.85.0/leptonica-license.txt", "ocr/licenses/Leptonica-LICENSE.txt"],
    [".tools/release-cache/tessdata-best-4.1.0-LICENSE.txt", "ocr/licenses/English-data-LICENSE.txt", "a6cba85bc92e0cff7a450b1d873c0eaa2e9fc96bf472df0247a26bec77bf3ff9"],
  ];
  if (!process.env.NDK_HOME) throw new Error("Run this through the Android development entry to locate the pinned NDK.");
  ocr.push([resolve(process.env.NDK_HOME, "NOTICE"), "ocr/licenses/Android-NDK-NOTICE.txt"]);
  ocr.push([resolve(process.env.NDK_HOME, "NOTICE.toolchain"), "ocr/licenses/Android-NDK-toolchain-NOTICE.txt"]);
  for (const [input, path, expected] of ocr) {
    const sourcePath = resolve(root, input);
    const digest = await hash(sourcePath);
    if (expected && digest !== expected) throw new Error(`OCR resource checksum mismatch: ${path}`);
    const target = resolve(destination,path);
    await mkdir(dirname(target),{recursive:true});
    let exists = false;
    try { exists = await hash(target) === digest; } catch {}
    if (!exists) await copyFile(sourcePath,target);
    files.push({path,sha256:digest,size:(await stat(sourcePath)).size});
  }
  await writeFile(resolve(destination, "manifest.json"), JSON.stringify({ schemaVersion: 1, files }, null, 2) + "\n");
  console.log(`Android offline assets ready: ${files.length} verified dictionary, OCR and notice files.`);
} catch (error) {
  console.error(`Android resources: ${error.message}\nPrepare the shared dictionaries with python -X utf8 scripts/prepare-base-dictionary.py first.`);
  process.exitCode = 1;
}
