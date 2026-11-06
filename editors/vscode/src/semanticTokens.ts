import * as vscode from "vscode";
import {
  ClientCapabilities,
  DocumentSelector,
  FeatureState,
  LanguageClient,
  SemanticTokensOptions,
  SemanticTokensRefreshRequest,
  SemanticTokensRegistrationOptions,
  SemanticTokensRequest,
  ServerCapabilities,
  StaticFeature,
} from "vscode-languageclient/node";

export const FLOW_SEMANTIC_TOKEN_TYPES = [
  "flowRoot",
  "flowTask",
  "flowDirective",
  "flowProperty",
  "flowEnum",
  "flowAction",
  "flowVariable",
  "flowParameter",
  "flowTaskRef",
] as const;

export type FlowSemanticTokenType = (typeof FLOW_SEMANTIC_TOKEN_TYPES)[number];

const FLOW_TOKEN_INDEX = new Map<FlowSemanticTokenType, number>(
  FLOW_SEMANTIC_TOKEN_TYPES.map((name, index) => [name, index])
);

export const FLOW_SEMANTIC_LEGEND = new vscode.SemanticTokensLegend(
  [...FLOW_SEMANTIC_TOKEN_TYPES],
  []
);

const FLOW_DIRECTIVES = new Set(["on", "watch", "steps"]);
const FLOW_CONSTANTS = new Set(["true", "false", "null", "~"]);
const MAPPING_KEY_SUFFIX_RE = /^["']?\s*:(?:\s|$)/;
const TEMPLATE_EXPR_RE = /\$\{[^{}\r\n]+\}/g;

/**
 * Replaces vscode-languageclient's default semantic token registration for Flow
 * so standard LSP token types emitted by `evin lsp` are mapped onto dedicated
 * Flow semantic token types instead of colliding with generic theme overrides.
 */
export class FlowSemanticTokensFeature implements StaticFeature {
  private serverLegendTypes: string[] | null = null;
  private disposables: vscode.Disposable[] = [];

  constructor(private readonly client: LanguageClient) {}

  public fillClientCapabilities(_capabilities: ClientCapabilities): void {
    // Built-in SemanticTokensFeature advertises standard LSP semantic token client capabilities.
  }

  public preInitialize(
    capabilities: ServerCapabilities,
    _documentSelector: DocumentSelector | undefined
  ): void {
    const provider = capabilities.semanticTokensProvider as
      | SemanticTokensOptions
      | SemanticTokensRegistrationOptions
      | undefined;
    if (!provider?.legend?.tokenTypes) {
      this.serverLegendTypes = null;
      return;
    }

    this.serverLegendTypes = [...provider.legend.tokenTypes];
    // Prevent built-in SemanticTokensFeature from registering a second provider with standard token names.
    capabilities.semanticTokensProvider = undefined;
  }

  public initialize(
    _capabilities: ServerCapabilities,
    documentSelector: DocumentSelector | undefined
  ): void {
    this.clear();
    const serverLegendTypes = this.serverLegendTypes;
    if (!serverLegendTypes) {
      return;
    }

    const refreshEmitter = new vscode.EventEmitter<void>();
    this.disposables.push(refreshEmitter);

    this.disposables.push(
      this.client.onRequest(SemanticTokensRefreshRequest.type, () => {
        refreshEmitter.fire();
      })
    );

    const selector: vscode.DocumentSelector = documentSelector
      ? this.client.protocol2CodeConverter.asDocumentSelector(documentSelector)
      : [{ scheme: "file", language: "flow" }];

    const provider: vscode.DocumentSemanticTokensProvider = {
      onDidChangeSemanticTokens: refreshEmitter.event,
      provideDocumentSemanticTokens: async (document, token) => {
        try {
          const params = {
            textDocument: this.client.code2ProtocolConverter.asTextDocumentIdentifier(document),
          };
          const result = await this.client.sendRequest(SemanticTokensRequest.type, params, token);
          if (token.isCancellationRequested || !result) {
            return null;
          }
          return translateFlowSemanticTokens(document, result.data, serverLegendTypes);
        } catch (error) {
          return this.client.handleFailedRequest(SemanticTokensRequest.type, token, error, null);
        }
      },
    };

    this.disposables.push(
      vscode.languages.registerDocumentSemanticTokensProvider(
        selector,
        provider,
        FLOW_SEMANTIC_LEGEND
      )
    );
  }

  public getState(): FeatureState {
    return { kind: "static" };
  }

  public clear(): void {
    for (const disposable of this.disposables.splice(0, this.disposables.length)) {
      disposable.dispose();
    }
  }
}

export function translateFlowSemanticTokens(
  document: vscode.TextDocument,
  data: readonly number[],
  serverLegendTypes: readonly string[]
): vscode.SemanticTokens {
  const builder = new vscode.SemanticTokensBuilder(FLOW_SEMANTIC_LEGEND);
  let line = 0;
  let startChar = 0;
  let prevPushedLine = -1;
  let prevPushedEnd = -1;

  for (let index = 0; index + 4 < data.length; index += 5) {
    const deltaLine = data[index];
    const deltaStart = data[index + 1];
    const length = data[index + 2];
    const rawType = data[index + 3];

    if (deltaLine > 0) {
      line += deltaLine;
      startChar = deltaStart;
    } else {
      startChar += deltaStart;
    }

    if (length <= 0 || line < 0 || line >= document.lineCount) {
      continue;
    }

    const serverType = serverLegendTypes[rawType];
    if (!serverType || serverType === "string") {
      // Leave plain string spans to the TextMate grammar so quotes, escapes,
      // block scalar headers, booleans, and numbers keep their fine-grained scopes.
      continue;
    }

    const lineText = document.lineAt(line).text;
    if (startChar < 0 || startChar >= lineText.length) {
      continue;
    }

    let emitStart = startChar;
    let emitLength = Math.min(length, lineText.length - startChar);
    if (emitLength <= 0) {
      continue;
    }

    let tokenText = lineText.slice(emitStart, emitStart + emitLength);
    if (tokenText.trim().length === 0) {
      continue;
    }

    const afterToken = lineText.slice(emitStart + emitLength);
    const isMappingKey = MAPPING_KEY_SUFFIX_RE.test(afterToken);

    // Snap template variable references inside quoted/escaped scalars to the exact `${...}` span.
    if (serverType === "variable" && !isMappingKey) {
      const snapped = findOverlappingTemplateSpan(lineText, emitStart, emitStart + emitLength);
      if (!snapped) {
        // Leave `${...}` inside multiline scalars where line offsets drifted to TextMate's exact match.
        continue;
      }
      emitStart = snapped.start;
      emitLength = snapped.length;
      tokenText = lineText.slice(emitStart, emitStart + emitLength);
    } else if (!isMappingKey && isWrappedInQuotes(tokenText)) {
      // Keep surrounding quote characters styled as string delimiters.
      emitStart += 1;
      emitLength -= 2;
      if (emitLength <= 0) {
        continue;
      }
      tokenText = lineText.slice(emitStart, emitStart + emitLength);
    }

    const mapped = classifyFlowToken(serverType, emitStart, tokenText, isMappingKey);
    if (!mapped) {
      continue;
    }

    if (line === prevPushedLine && emitStart < prevPushedEnd) {
      continue;
    }

    const mappedIndex = FLOW_TOKEN_INDEX.get(mapped);
    if (mappedIndex !== undefined) {
      builder.push(line, emitStart, emitLength, mappedIndex, 0);
      prevPushedLine = line;
      prevPushedEnd = emitStart + emitLength;
    }
  }

  return builder.build();
}

function isWrappedInQuotes(text: string): boolean {
  return (
    text.length >= 2 &&
    ((text.startsWith('"') && text.endsWith('"')) || (text.startsWith("'") && text.endsWith("'")))
  );
}

function findOverlappingTemplateSpan(
  lineText: string,
  rangeStart: number,
  rangeEnd: number
): { start: number; length: number } | null {
  TEMPLATE_EXPR_RE.lastIndex = 0;
  let best: { start: number; length: number } | null = null;
  let bestOverlap = 0;
  let match: RegExpExecArray | null;
  while ((match = TEMPLATE_EXPR_RE.exec(lineText)) !== null) {
    const start = match.index;
    const end = start + match[0].length;
    const overlap = Math.min(end, rangeEnd) - Math.max(start, rangeStart);
    if (overlap > bestOverlap) {
      bestOverlap = overlap;
      best = { start, length: match[0].length };
    }
  }
  return best;
}

function classifyFlowToken(
  serverType: string,
  startChar: number,
  tokenText: string,
  isMappingKey: boolean
): FlowSemanticTokenType | null {
  switch (serverType) {
    case "function":
      return isMappingKey ? "flowTask" : "flowTaskRef";

    case "namespace":
    case "property":
    case "modifier": {
      if (startChar === 0 && isMappingKey) {
        return "flowRoot";
      }
      const key = tokenText.trim().replace(/^["']|["']$/g, "");
      if (isMappingKey && FLOW_DIRECTIVES.has(key)) {
        return "flowDirective";
      }
      return "flowProperty";
    }

    case "enumMember": {
      if (FLOW_CONSTANTS.has(tokenText.trim())) {
        return null;
      }
      return "flowEnum";
    }

    case "method":
      return "flowAction";

    case "variable":
      return "flowVariable";

    case "parameter":
      return "flowParameter";

    default:
      return null;
  }
}
