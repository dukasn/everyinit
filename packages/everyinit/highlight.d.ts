export type EvinCodeLanguage = "flow" | "esh" | "json";
export const supportedCodeLanguages: readonly EvinCodeLanguage[];

export interface EvinCodeToken {
  text: string;
  /** Most specific TextMate scope from the Flow, ESH, or JSON grammar. */
  scope: string | null;
  /** Full TextMate scope stack, from the language root to the token scope. */
  scopes: readonly string[];
}

export function resolveCodeLanguage(language: string | null | undefined): EvinCodeLanguage | null;
export interface EvinCodeHighlighter {
  tokenizeCode(source: string, language: string | null | undefined): EvinCodeToken[];
  dispose(): void;
}

/** Initialize the Flow and ESH VS Code grammars plus JSON for web frameworks. */
export function createCodeHighlighter(): Promise<EvinCodeHighlighter>;
/** Tokenize using a shared, lazily initialized highlighter. */
export function tokenizeCode(source: string, language: string | null | undefined): Promise<EvinCodeToken[]>;
