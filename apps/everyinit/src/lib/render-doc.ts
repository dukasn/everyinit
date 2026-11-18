import { createCodeHighlighter, resolveCodeLanguage } from "everyinit/highlight";

const codeHighlighter = await createCodeHighlighter();

type AstNode = {
  type?: string;
  [key: string]: unknown;
};

const escapeHtml = (value: unknown): string =>
  String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");

const safeUrl = (value: unknown): string => {
  const url = String(value ?? "").trim();
  return /^(?:javascript|vbscript|data):/i.test(url) ? "#" : url;
};

const docsLink = (value: unknown): string => {
  const url = safeUrl(value);
  if (url === "/") return "/docs/";
  if (url === "/docs") return "/docs/";
  if (url.startsWith("/") && !url.startsWith("//") && !url.startsWith("/docs/")) {
    return `/docs${url}`;
  }
  return url;
};

const syntaxClass = (scope: string): string => {
  if (scope.includes(".section.")) return "syntax-section";
  if (scope.startsWith("comment.")) return "syntax-comment";
  if (scope.startsWith("string.")) return "syntax-string";
  if (scope.startsWith("constant.character.escape.")) return "syntax-escape";
  if (scope.startsWith("entity.name.tag.enum.")) return "syntax-enum";
  if (scope.startsWith("keyword.control.property.")) return "syntax-key";
  if (scope.startsWith("support.type.property-name.")) return "syntax-key";
  if (scope.startsWith("keyword.operator.")) return "syntax-punctuation";
  if (scope.startsWith("entity.name.type.directive.")) return "syntax-keyword";
  if (scope.startsWith("keyword.")) return "syntax-keyword";
  if (scope.startsWith("markup.bold.")) return "syntax-task";
  if (scope.startsWith("entity.name.function.action.")) return "syntax-builtin";
  if (scope.startsWith("entity.name.tag.")) return "syntax-enum";
  if (scope.startsWith("variable.")) return "syntax-variable";
  if (scope.startsWith("support.function.")) return "syntax-builtin";
  if (scope.startsWith("constant.")) return "syntax-constant";
  if (scope.startsWith("punctuation.")) return "syntax-punctuation";
  return "syntax-token";
};

export const renderHighlightedCode = (source: string, language: string): string =>
  codeHighlighter
    .tokenizeCode(source, language)
    .map(({ text, scope }) =>
      scope
        ? `<span class="${syntaxClass(scope)}" data-scope="${escapeHtml(scope)}">${escapeHtml(text)}</span>`
        : escapeHtml(text),
    )
    .join("");

const inline = (value: unknown, assets: Record<string, string>): string => {
  if (!value || typeof value !== "object") return "";
  const node = value as AstNode;
  const children = Array.isArray(node.children)
    ? node.children.map((child) => inline(child, assets)).join("")
    : "";
  switch (node.type) {
    case "Text":
      return escapeHtml(node.content);
    case "Strong":
      return `<strong>${children}</strong>`;
    case "Emphasis":
      return `<em>${children}</em>`;
    case "Code":
      return `<code>${escapeHtml(node.content)}</code>`;
    case "Link":
      return `<a href="${escapeHtml(docsLink(node.href))}"${node.title ? ` title="${escapeHtml(node.title)}"` : ""}>${escapeHtml(node.content)}</a>`;
    case "Image":
      return `<img src="${escapeHtml(safeUrl(assets[String(node.src)] ?? node.src))}" alt="${escapeHtml(node.alt)}" loading="lazy"${node.title ? ` title="${escapeHtml(node.title)}"` : ""}>`;
    case "Math":
      return `<code class="math-inline">${escapeHtml(node.latex)}</code>`;
    default:
      return escapeHtml(node.content ?? "");
  }
};

const blocks = (
  values: unknown,
  assets: Record<string, string>,
  diagrams: Record<string, string>,
): string =>
  Array.isArray(values) ? values.map((value) => block(value, assets, diagrams)).join("\n") : "";

const block = (
  value: unknown,
  assets: Record<string, string>,
  diagrams: Record<string, string>,
): string => {
  if (!value || typeof value !== "object") return "";
  const node = value as AstNode;
  const children = Array.isArray(node.children)
    ? node.children.map((child) => inline(child, assets)).join("")
    : "";
  const body = blocks(node.body ?? node.items, assets, diagrams);
  switch (node.type) {
    case "Heading": {
      const level = Math.min(6, Math.max(1, Number(node.level) || 2));
      const anchor = node.anchor ? ` id="${escapeHtml(node.anchor)}"` : "";
      return `<h${level}${anchor}>${children || escapeHtml(node.text)}</h${level}>`;
    }
    case "Paragraph":
      return `<p>${children}</p>`;
    case "CodeBlock": {
      const rawLanguage = String(node.language ?? "");
      const language = rawLanguage ? ` class="language-${escapeHtml(rawLanguage)}"` : "";
      const canonicalLanguage = resolveCodeLanguage(rawLanguage);
      const languageData = canonicalLanguage ? ` data-language="${canonicalLanguage}"` : "";
      const code = String(node.code ?? "");
      const renderedCode = canonicalLanguage
        ? renderHighlightedCode(code, canonicalLanguage)
        : escapeHtml(code);
      const title = node.title ? `<div class="code-title">${escapeHtml(node.title)}</div>` : "";
      return `<figure class="code-block">${title}<pre><code${language}${languageData}>${renderedCode}</code></pre></figure>`;
    }
    case "Media": {
      if (node.kind === "video")
        return `<video controls preload="metadata" src="${escapeHtml(safeUrl(assets[String(node.src)] ?? node.src))}">${escapeHtml(node.alt)}</video>`;
      return `<figure><img src="${escapeHtml(safeUrl(assets[String(node.src)] ?? node.src))}" alt="${escapeHtml(node.alt)}" loading="lazy"><figcaption>${escapeHtml(node.title ?? node.alt)}</figcaption></figure>`;
    }
    case "Admonition":
      return `<aside class="admonition ${escapeHtml(node.kind)}"><strong>${escapeHtml(node.title ?? node.kind)}</strong>${blocks(node.children, assets, diagrams)}</aside>`;
    case "List": {
      const tag = node.ordered ? "ol" : "ul";
      const start = node.ordered && Number(node.start) > 1 ? ` start="${Number(node.start)}"` : "";
      const items = Array.isArray(node.items)
        ? node.items.map((item) => `<li>${blocks(item, assets, diagrams)}</li>`).join("")
        : "";
      return `<${tag}${start}>${items}</${tag}>`;
    }
    case "TaskList": {
      const items = Array.isArray(node.items)
        ? node.items
            .map((item) => {
              const task = item as AstNode;
              return `<li><input type="checkbox" disabled${task.checked ? " checked" : ""}>${blocks(task.body, assets, diagrams)}</li>`;
            })
            .join("")
        : "";
      return `<ul class="task-list">${items}</ul>`;
    }
    case "Table": {
      const headers = Array.isArray(node.headers)
        ? node.headers
            .map(
              (cell) =>
                `<th scope="col">${Array.isArray(cell) ? cell.map((child) => inline(child, assets)).join("") : ""}</th>`,
            )
            .join("")
        : "";
      const rows = Array.isArray(node.rows)
        ? node.rows
            .map(
              (row) =>
                `<tr>${Array.isArray(row) ? row.map((cell) => `<td>${Array.isArray(cell) ? cell.map((child) => inline(child, assets)).join("") : ""}</td>`).join("") : ""}</tr>`,
            )
            .join("")
        : "";
      return `<div class="table-scroll"><table><thead><tr>${headers}</tr></thead><tbody>${rows}</tbody></table></div>`;
    }
    case "HorizontalRule":
      return "<hr>";
    case "MathBlock":
      return `<pre class="math-block">${escapeHtml(node.latex)}</pre>`;
    case "Diagram": {
      const id = String(node.id ?? "");
      const svg = diagrams[id];
      const caption = node.caption ?? node.title ?? node.alt ?? node.kind;
      return svg
        ? `<figure class="diagram" data-diagram-kind="${escapeHtml(node.kind)}"><figcaption>${escapeHtml(caption)}</figcaption>${svg}</figure>`
        : `<figure class="diagram"><figcaption>${escapeHtml(caption)}</figcaption><pre><code>${escapeHtml(node.code)}</code></pre></figure>`;
    }
    case "Tape":
      return `<pre class="tape-block"><code>${escapeHtml(node.script)}</code></pre>`;
    case "RawHtml":
      return `<pre class="raw-html"><code>${escapeHtml(node.content)}</code></pre>`;
    case "LspSymbol":
      return `<details class="api-symbol"><summary><code>${escapeHtml(node.name)}</code> <span>${escapeHtml(node.summary)}</span></summary>${blocks(node.children, assets, diagrams)}</details>`;
    case "Directive":
      return blocks(node.body, assets, diagrams);
    case "DefinitionList":
      return `<dl>${
        Array.isArray(node.items)
          ? node.items
              .map((item) => {
                const entry = item as AstNode;
                return `<dt>${Array.isArray(entry.term) ? entry.term.map((child) => inline(child, assets)).join("") : ""}</dt><dd>${blocks(entry.body, assets, diagrams)}</dd>`;
              })
              .join("")
          : ""
      }</dl>`;
    case "OptionList":
      return `<dl>${
        Array.isArray(node.items)
          ? node.items
              .map((item) => {
                const entry = item as AstNode;
                return `<dt><code>${escapeHtml([...(Array.isArray(entry.flags) ? entry.flags : []), entry.argument].filter(Boolean).join(" "))}</code></dt><dd>${blocks(entry.body, assets, diagrams)}</dd>`;
              })
              .join("")
          : ""
      }</dl>`;
    case "FieldList":
      return `<dl>${
        Array.isArray(node.items)
          ? node.items
              .map((item) => {
                const entry = item as AstNode;
                return `<dt>${escapeHtml(entry.name)}</dt><dd>${blocks(entry.body, assets, diagrams)}</dd>`;
              })
              .join("")
          : ""
      }</dl>`;
    case "Stepper":
      return `<ol class="stepper">${
        Array.isArray(node.steps)
          ? node.steps
              .map((item) => {
                const step = item as AstNode;
                return `<li><strong>${Array.isArray(step.title) ? step.title.map((child) => inline(child, assets)).join("") : escapeHtml(step.title)}</strong>${blocks(step.body, assets, diagrams)}</li>`;
              })
              .join("")
          : ""
      }</ol>`;
    default:
      return body;
  }
};

export function renderDocument(
  ast: unknown,
  assets: Record<string, string> = {},
  diagrams: Record<string, string> = {},
): string {
  return blocks(ast, assets, diagrams);
}
