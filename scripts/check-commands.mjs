import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const catalogPath = resolve(root, "src/commands.ts");
const program = ts.createProgram([catalogPath], {
  strict: true,
  target: ts.ScriptTarget.ES2022,
  module: ts.ModuleKind.ESNext,
  moduleResolution: ts.ModuleResolutionKind.Bundler,
  skipLibCheck: true,
  noEmit: true,
});
const checker = program.getTypeChecker();
const source = program.getSourceFile(catalogPath);
const catalog = source.statements.find(
  (node) => ts.isInterfaceDeclaration(node) && node.name.text === "CommandMap",
);
assert.ok(catalog, "Missing CommandMap");
const commands = checker.getTypeAtLocation(catalog).getProperties();
const nativeRoot = resolve(root, "src-tauri/src/desktop/commands");
const registry = readFileSync(resolve(nativeRoot, "mod.rs"), "utf8")
  .split("tauri::generate_handler![")[1]
  ?.split("]")[0];
assert.ok(registry, "Missing native command registry");
const registered = [...registry.matchAll(/(\w+)::(\w+)/g)];
assert.equal(
  new Set(registered.map((match) => match[2])).size,
  registered.length,
);
assert.deepEqual(
  commands.map((command) => command.name).sort(),
  registered.map((match) => match[2]).sort(),
  "TypeScript catalog must cover exactly the registered native commands",
);

for (const [, module, name] of registered) {
  const native = readFileSync(resolve(nativeRoot, `${module}.rs`), "utf8");
  const signature = native.match(
    new RegExp(
      `pub\\(crate\\) (?:async )?fn ${name}\\(([\\s\\S]*?)\\)\\s*(?:->[^\\{]+)?\\{`,
    ),
  )?.[1];
  assert.notEqual(signature, undefined, `${name}: missing command definition`);
  // Commas inside State<'_, T> do not delimit a named function parameter.
  const parameters = [
    ...signature.matchAll(
      /(?:^|,)\s*(\w+):\s*([\s\S]*?)(?=,\s*\w+\s*:|,\s*$|$)/g,
    ),
  ].filter(([, , type]) => !/^(AppState|State<|tauri::AppHandle)/.test(type));
  const declaration = commands.find((command) => command.name === name);
  const command = checker.getTypeOfSymbolAtLocation(declaration, catalog);
  const args = checker.getTypeOfSymbolAtLocation(
    command.getProperty("args"),
    catalog,
  );
  if (parameters.some(([, , type]) => type.includes("tauri::ipc::Request"))) {
    assert.ok(
      checker.typeToString(args).startsWith("Uint8Array"),
      `${name}: raw binary body`,
    );
    continue;
  }
  const fields = args.getProperties();
  const camelCase = (value) =>
    value.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase());
  assert.deepEqual(
    fields.map((field) => field.name).sort(),
    parameters.map(([, parameter]) => camelCase(parameter)).sort(),
    `${name}: top-level IPC arguments differ`,
  );
  for (const [, parameter, type] of parameters) {
    const field = fields.find((value) => value.name === camelCase(parameter));
    assert.equal(
      Boolean(field.flags & ts.SymbolFlags.Optional),
      type.startsWith("Option<"),
      `${name}.${field.name}: optional argument differs`,
    );
  }
}

console.log(
  `Command catalog: ${commands.length} native registrations and top-level arguments aligned (including raw audio).`,
);
