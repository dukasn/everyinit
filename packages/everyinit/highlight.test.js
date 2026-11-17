import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  createCodeHighlighter,
  resolveCodeLanguage,
  supportedCodeLanguages,
  tokenizeCode,
} from "./highlight.js";

const scopes = (tokens) => new Set(tokens.flatMap((token) => token.scopes));
const highlighter = await createCodeHighlighter();

test("package grammars stay byte-for-byte aligned with the VS Code grammars", async () => {
  for (const language of ["flow", "esh"]) {
    const [packageGrammar, extensionGrammar] = await Promise.all([
      readFile(new URL(`./grammars/${language}.tmLanguage.json`, import.meta.url), "utf8"),
      readFile(new URL(`../../editors/vscode/syntaxes/${language}.tmLanguage.json`, import.meta.url), "utf8"),
    ]);
    assert.deepEqual(JSON.parse(packageGrammar), JSON.parse(extensionGrammar));
  }
});

test("Flow tokens match contextual scopes from the VS Code grammar", () => {
  const source = `#!evin/v1\nworkspace:\n  name: "demo\\n"\n  path: ./dist\n  output_dir: "\${BUILD_DIR}"\n  enabled: true # shared config\n  retries: 2\nvalues:\n  artifact_name: example\ntasks:\n  package:\n    stage: Release\n    description: "Build an archive"\n    steps:\n      - uses: evin/archive@v1\n        with:\n          path: "dist/\${artifact_name}.tar.gz"\n          include: [LICENSE]\n      - run: |\n          echo "archive: \${artifact_name}"\n`;
  const tokens = highlighter.tokenizeCode(source, "flow");
  const found = scopes(tokens);

  assert.equal(tokens.map((token) => token.text).join(""), source);
  assert.ok(found.has("comment.line.shebang.flow"));
  assert.ok(found.has("comment.block.section.flow"));
  assert.ok(found.has("markup.bold.flow"));
  assert.ok(found.has("keyword.control.property.flow"));
  assert.ok(found.has("string.unquoted.path.flow"));
  assert.ok(found.has("constant.language.boolean.flow"));
  assert.ok(found.has("constant.numeric.flow"));
  assert.ok(found.has("entity.name.tag.enum.flow"));
  assert.ok(found.has("entity.name.function.action.flow"));
  assert.ok(found.has("string.unquoted.block.flow"));
  assert.ok(found.has("string.quoted.double.flow"));
  assert.ok(found.has("constant.character.escape.flow"));
  assert.ok(found.has("support.variable.flow"));
  assert.ok(found.has("comment.line.number-sign.flow"));
});

test("ESH tokens match variables, operators, and control scopes from VS Code", () => {
  const source = `#!/usr/bin/env esh\n# deploy\nif test "$MODE" == "prod"; then\n  printf "%s\\n" "$MESSAGE"\n  echo "\${MESSAGE}"\n  echo 'done'\nfi\n`;
  const tokens = highlighter.tokenizeCode(source, "esh");
  const found = scopes(tokens);

  assert.equal(tokens.map((token) => token.text).join(""), source);
  assert.ok(found.has("comment.line.shebang.esh"));
  assert.ok(found.has("comment.line.number-sign.esh"));
  assert.ok(found.has("keyword.control.esh"));
  assert.ok(found.has("support.function.builtin.esh"));
  assert.ok(found.has("variable.other.normal.esh"));
  assert.ok(found.has("variable.other.bracket.esh"));
  assert.ok(found.has("string.quoted.double.esh"));
  assert.ok(found.has("string.quoted.single.esh"));
  assert.ok(found.has("constant.character.escape.esh"));
  assert.ok(found.has("keyword.operator.esh"));
});

test("language aliases resolve and unknown languages remain plain text", async () => {
  assert.deepEqual(supportedCodeLanguages, ["flow", "esh", "json"]);
  assert.equal(resolveCodeLanguage(".flow"), "flow");
  assert.equal(resolveCodeLanguage("EVIN-ESH"), "esh");
  assert.equal(resolveCodeLanguage("JSON"), "json");
  assert.equal(resolveCodeLanguage("bash"), null);
  assert.deepEqual(await tokenizeCode("echo hi", "bash"), [
    { text: "echo hi", scope: null, scopes: [] },
  ]);
});
