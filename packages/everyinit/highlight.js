import { createHighlighterCore } from "shiki/core";
import { createOnigurumaEngine } from "shiki/engine/oniguruma";
import flowGrammar from "./grammars/flow.tmLanguage.json" with { type: "json" };
import eshGrammar from "./grammars/esh.tmLanguage.json" with { type: "json" };

const languages = new Map([
  ["flow", "flow"],
  ["evin-flow", "flow"],
  ["esh", "esh"],
  ["evin-esh", "esh"],
]);

export const supportedCodeLanguages = Object.freeze(["flow", "esh"]);

const grammars = [
  { ...flowGrammar, name: "flow", aliases: ["evin-flow"] },
  { ...eshGrammar, name: "esh", aliases: ["evin-esh"] },
];

/** Return the canonical language ID for a supported Evin code language. */
export function resolveCodeLanguage(language) {
  if (typeof language !== "string") return null;
  return languages.get(language.trim().toLowerCase().replace(/^\./, "")) ?? null;
}

function tokenSegments(lines) {
  const segments = [];
  for (const [lineIndex, line] of lines.entries()) {
    for (const token of line) {
      const explanation = token.explanation ?? [];
      if (explanation.length) {
        for (const item of explanation) {
          const scopes = item.scopes.map(({ scopeName }) => scopeName);
          segments.push({
            text: item.content,
            scope: scopes.length > 1 ? scopes.at(-1) : null,
            scopes,
          });
        }
      } else {
        segments.push({ text: token.content, scope: null, scopes: [] });
      }
    }
    if (lineIndex < lines.length - 1) segments.push({ text: "\n", scope: null, scopes: [] });
  }
  return segments;
}

/**
 * Create a Flow/ESH highlighter backed by the same TextMate grammars as the
 * VS Code extension. Initialize once per app and reuse it for all code blocks.
 */
export async function createCodeHighlighter() {
  const engine = await createOnigurumaEngine(() => import("shiki/wasm"));
  const highlighter = await createHighlighterCore({
    engine,
    langs: grammars,
    themes: [],
  });

  return Object.freeze({
    tokenizeCode(source, language) {
      const text = String(source ?? "");
      const canonical = resolveCodeLanguage(language);
      if (!canonical) return [{ text, scope: null, scopes: [] }];

      const lines = highlighter.codeToTokensBase(text, {
        lang: canonical,
        includeExplanation: "scopeName",
      });
      return tokenSegments(lines);
    },
    dispose() {
      highlighter.dispose();
    },
  });
}

let defaultHighlighter;

/** Tokenize source with a lazily initialized shared highlighter. */
export async function tokenizeCode(source, language) {
  defaultHighlighter ??= createCodeHighlighter();
  return (await defaultHighlighter).tokenizeCode(source, language);
}
