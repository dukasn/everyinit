export type EvinCodeLanguage = "flow" | "esh";
export const supportedCodeLanguages: readonly EvinCodeLanguage[];

export interface EvinCodeToken {
  text: string;
  /** Most specific TextMate scope from the VS Code Flow or ESH grammar. */
  scope: string | null;
  /** Full TextMate scope stack, from the language root to the token scope. */
  scopes: readonly string[];
}

export function resolveCodeLanguage(language: string | null | undefined): EvinCodeLanguage | null;
export interface EvinCodeHighlighter {
  tokenizeCode(source: string, language: string | null | undefined): EvinCodeToken[];
  dispose(): void;
}

/** Initialize the shared VS Code TextMate grammars for use in web frameworks. */
export function createCodeHighlighter(): Promise<EvinCodeHighlighter>;
/** Tokenize using a shared, lazily initialized highlighter. */
export function tokenizeCode(source: string, language: string | null | undefined): Promise<EvinCodeToken[]>;
