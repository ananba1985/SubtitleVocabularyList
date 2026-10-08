import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
  copyFileSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const fixture = mkdtempSync(join(tmpdir(), "svl-version-"));
try {
  mkdirSync(join(fixture, "scripts"));
  mkdirSync(join(fixture, "src-tauri"));
  const script = join(fixture, "scripts/version.mjs");
  copyFileSync(
    join(dirname(fileURLToPath(import.meta.url)), "version.mjs"),
    script,
  );
  const files = [
    "package.json",
    "src-tauri/tauri.conf.json",
    "src-tauri/Cargo.toml",
    "src-tauri/Cargo.lock",
    "CHANGELOG.md",
  ];
  writeFileSync(
    join(fixture, files[0]),
    '{"name":"subtitle-vocabulary-list",\n"version": "0.1.0",\n"dependencies":{"example":"9.9.9"}}\n',
  );
  writeFileSync(
    join(fixture, files[1]),
    '{\n"version": "0.1.0",\n"identifier":"example.test"}\n',
  );
  writeFileSync(
    join(fixture, files[2]),
    '[package]\nname = "subtitle-vocabulary-list"\nversion = "0.1.0"\n[dependencies]\nexample = { version = "9.9.9" }\n',
  );
  writeFileSync(
    join(fixture, files[3]),
    '[[package]]\nname = "example"\nversion = "9.9.9"\n\n[[package]]\nname = "subtitle-vocabulary-list"\nversion = "0.1.0"\ndependencies = ["example"]\n',
  );
  writeFileSync(
    join(fixture, files[4]),
    "# Changelog\n\n## 0.1.0\n\n- Baseline\n",
  );
  const read = (path) => readFileSync(join(fixture, path), "utf8");
  const run = (...args) =>
    execFileSync(process.execPath, [script, ...args], {
      cwd: tmpdir(),
      encoding: "utf8",
    });
  const snapshot = () => files.map(read);
  const expectRejected = (...args) => {
    const before = snapshot();
    assert.notEqual(
      spawnSync(process.execPath, [script, ...args], { encoding: "utf8" })
        .status,
      0,
    );
    assert.deepEqual(
      snapshot(),
      before,
      "Rejected operations must not modify files",
    );
  };
  const expectVersion = (version) => {
    run("check");
    assert.equal(JSON.parse(read(files[0])).version, version);
    assert.equal(JSON.parse(read(files[1])).version, version);
    assert.ok(read(files[2]).includes(`version = "${version}"`));
    assert.ok(
      read(files[3]).includes(
        `name = "subtitle-vocabulary-list"\nversion = "${version}"`,
      ),
    );
    assert.ok(read(files[2]).includes('example = { version = "9.9.9" }'));
    assert.ok(read(files[3]).includes('name = "example"\nversion = "9.9.9"'));
  };
  expectVersion("0.1.0");
  expectRejected("major", "--message", "must be manual");
  expectRejected("patch");
  run("patch", "--message", "修复示例错误");
  expectVersion("0.1.1");
  assert.ok(read("CHANGELOG.md").includes("修复示例错误"));
  run("patch", "--message", "第二次修复");
  expectVersion("0.1.2");
  run("minor", "--message", "新增大功能");
  expectVersion("0.2.0");
  expectRejected("set", "0.1.9", "--message", "downgrade");
  expectRejected("set", "01.0.0", "--message", "invalid");
  run("set", "1.0.0", "--message", "负责人手动设置主版本");
  expectVersion("1.0.0");
  run("patch", "--message", "主版本保持");
  expectVersion("1.0.1");
  writeFileSync(
    join(fixture, files[1]),
    read(files[1]).replace('"1.0.1"', '"1.0.2"'),
  );
  expectRejected("check");
  const changelog = read("CHANGELOG.md");
  run("sync");
  expectVersion("1.0.1");
  assert.equal(read("CHANGELOG.md"), changelog);
  assert.ok(!read("CHANGELOG.md").includes("undefined"));
  console.log(
    "Version workflow verified: patch, minor reset, manual major, consistency, dependency preservation, changelog and rejected-operation integrity.",
  );
} finally {
  assert.equal(dirname(resolve(fixture)), resolve(tmpdir()));
  assert.ok(basename(fixture).startsWith("svl-version-"));
  rmSync(fixture, { recursive: true, force: true });
}
