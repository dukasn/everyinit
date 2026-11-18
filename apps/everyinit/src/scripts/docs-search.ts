import { createTranslator } from "../i18n";
import { loadSearchIndex, searchDocs } from "virtual:evin/search";

const t = createTranslator(
  document.querySelector<HTMLDialogElement>("#docs-search-dialog")?.dataset.locale,
);
const header = document.querySelector<HTMLElement>("#docs-site-header");
const dialog = document.querySelector<HTMLDialogElement>("#docs-search-dialog");
const openButton = document.querySelector<HTMLButtonElement>("#docs-search-open");
const sidebarSearch = document.querySelector<HTMLButtonElement>("#docs-sidebar-search");
const mobileSearch = document.querySelector<HTMLButtonElement>("#docs-mobile-search");
const input = document.querySelector<HTMLInputElement>("#docs-search-input");
const variantMenus = [...document.querySelectorAll<HTMLDetailsElement>(".docs-variant")];
const prompt = document.querySelector<HTMLDivElement>("#docs-search-prompt");
const empty = document.querySelector<HTMLDivElement>("#docs-search-empty");
const emptyMessageNode = document.querySelector<HTMLParagraphElement>("#docs-search-empty-message");
const status = document.querySelector<HTMLParagraphElement>("#docs-search-status");
const results = document.querySelector<HTMLDivElement>("#docs-search-results");
const emptyMessage = t("docs-search-empty-copy");
const docsBase = dialog?.dataset.docsBase ?? "/docs";
const docsHref = (path: string) => (path === "/" ? `${docsBase}/` : `${docsBase}${path}`);
let visibleOptions: HTMLAnchorElement[] = [];
let activeIndex = -1;
let searchIndexPromise: ReturnType<typeof loadSearchIndex> | undefined;
let searchRequest = 0;
let returnFocusTo: HTMLElement | null = null;
let scrollLock: {
  rootOverflow: string;
  bodyOverflow: string;
  rootOverscroll: string;
  bodyOverscroll: string;
} | null = null;
const resultCache = new Map<string, Awaited<ReturnType<typeof searchDocs>>>();
const resultCacheLimit = 40;
const highlightName = "docs-search-match";

const clearSearchHighlights = () => {
  if ("highlights" in CSS) {
    CSS.highlights.delete(highlightName);
  }
};

const highlightSearchMatches = (query: string) => {
  clearSearchHighlights();
  if (!("highlights" in CSS) || typeof Highlight !== "function" || !results) return;
  const terms = [...new Set(query.trim().toLowerCase().split(/\s+/).filter(Boolean))].sort(
    (a, b) => b.length - a.length,
  );
  if (!terms.length) return;

  const ranges: Range[] = [];
  const targets = results.querySelectorAll<HTMLElement>(
    ".docs-search-option-title, .docs-search-option-path, .docs-search-option-snippet",
  );
  for (const element of targets) {
    const textNode = element.firstChild;
    if (!(textNode instanceof Text) || !textNode.data) continue;
    const haystack = textNode.data.toLowerCase();
    for (const term of terms) {
      let fromIndex = 0;
      while (fromIndex < haystack.length) {
        const matchIndex = haystack.indexOf(term, fromIndex);
        if (matchIndex === -1) break;
        const range = new Range();
        range.setStart(textNode, matchIndex);
        range.setEnd(textNode, matchIndex + term.length);
        ranges.push(range);
        fromIndex = matchIndex + term.length;
      }
    }
  }

  if (ranges.length) {
    CSS.highlights.set(highlightName, new Highlight(...ranges));
  }
};

const setActive = (index: number) => {
  activeIndex =
    !visibleOptions.length || index === -1
      ? -1
      : ((index % visibleOptions.length) + visibleOptions.length) % visibleOptions.length;
  for (const option of visibleOptions) {
    option.setAttribute("aria-selected", "false");
    option.classList.remove("is-active");
  }
  if (activeIndex < 0) input?.removeAttribute("aria-activedescendant");
  else {
    const option = visibleOptions[activeIndex];
    option.setAttribute("aria-selected", "true");
    option.classList.add("is-active");
    input?.setAttribute("aria-activedescendant", option.id);
    option.scrollIntoView({ block: "nearest" });
  }
};

const search = async (value: string) => {
  const request = ++searchRequest;
  const query = value.trim();
  visibleOptions = [];
  results?.replaceChildren();
  clearSearchHighlights();
  results?.classList.toggle("hidden", !query);
  prompt?.classList.toggle("hidden", Boolean(query));
  input?.removeAttribute("aria-activedescendant");
  if (!query) {
    empty?.classList.add("hidden");
    empty?.classList.remove("is-loading", "is-error");
    if (status) status.textContent = "";
    activeIndex = -1;
    return;
  }
  if (emptyMessageNode) emptyMessageNode.textContent = t("docs-search-loading");
  empty?.classList.add("is-loading");
  empty?.classList.remove("is-error");
  empty?.classList.remove("hidden");
  if (status) status.textContent = t("docs-search-status-loading");
  try {
    let matches = resultCache.get(query);
    if (matches) {
      resultCache.delete(query);
      resultCache.set(query, matches);
    } else {
      searchIndexPromise ??= loadSearchIndex(
        dialog?.dataset.version ?? "latest",
        dialog?.dataset.locale ?? "en",
      );
      const index = await searchIndexPromise;
      if (request !== searchRequest) return;
      matches = searchDocs(index, query, { limit: 10 });
      resultCache.set(query, matches);
      if (resultCache.size > resultCacheLimit) {
        resultCache.delete(resultCache.keys().next().value!);
      }
    }
    if (request !== searchRequest) return;
    for (const match of matches) {
      const option = document.createElement("a");
      option.id = `docs-search-option-${visibleOptions.length}`;
      option.className = "docs-search-option";
      option.dataset.searchOption = "";
      option.href = docsHref(match.route);
      option.setAttribute("role", "option");
      option.setAttribute("aria-selected", "false");
      const title = document.createElement("span");
      title.className = "docs-search-option-title";
      title.textContent = match.title;
      const path = document.createElement("span");
      path.className = "docs-search-option-path";
      path.textContent = docsHref(match.route);
      const snippet = document.createElement("span");
      snippet.className = "docs-search-option-snippet";
      snippet.textContent = match.snippet;
      option.append(title, path, snippet);
      results?.append(option);
      visibleOptions.push(option);
    }
    empty?.classList.remove("is-loading");
    empty?.classList.toggle("hidden", matches.length > 0);
    if (!matches.length && emptyMessageNode) emptyMessageNode.textContent = emptyMessage;
    if (status)
      status.textContent = matches.length
        ? t("docs-search-results", { count: matches.length })
        : t("docs-search-no-results");
    highlightSearchMatches(query);
    setActive(-1);
  } catch (error) {
    if (request !== searchRequest) return;
    searchIndexPromise = undefined;
    empty?.classList.remove("is-loading");
    empty?.classList.add("is-error");
    empty?.classList.remove("hidden");
    if (emptyMessageNode) emptyMessageNode.textContent = t("docs-search-failed-copy");
    if (status) status.textContent = t("docs-search-failed-status");
    console.error("Failed to search Evin documentation", error);
  }
};

const openSearch = (invoker?: HTMLElement | null) => {
  header?.classList.remove("is-hidden");
  document.documentElement.classList.remove("docs-header-hidden");
  if (invoker) returnFocusTo = invoker;
  else if (!dialog?.open)
    returnFocusTo = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  if (!dialog?.open) {
    const root = document.documentElement;
    const body = document.body;
    scrollLock ??= {
      rootOverflow: root.style.overflow,
      bodyOverflow: body.style.overflow,
      rootOverscroll: root.style.overscrollBehavior,
      bodyOverscroll: body.style.overscrollBehavior,
    };
    root.style.overflow = "hidden";
    body.style.overflow = "hidden";
    root.style.overscrollBehavior = "none";
    body.style.overscrollBehavior = "none";
    dialog?.showModal();
  }
  input?.setAttribute("aria-expanded", "true");
  requestAnimationFrame(() => input?.focus());
};

openButton?.addEventListener("click", (event) => openSearch(event.currentTarget as HTMLElement));
sidebarSearch?.addEventListener("click", (event) => openSearch(event.currentTarget as HTMLElement));
mobileSearch?.addEventListener("click", (event) => openSearch(event.currentTarget as HTMLElement));
header?.addEventListener("focusin", () => {
  header.classList.remove("is-hidden");
  document.documentElement.classList.remove("docs-header-hidden");
});
let previousScrollY = window.scrollY;
let scrollFrame = 0;
window.addEventListener(
  "scroll",
  () => {
    if (scrollFrame) return;
    scrollFrame = requestAnimationFrame(() => {
      const currentScrollY = window.scrollY;
      const delta = currentScrollY - previousScrollY;
      const shouldShow =
        currentScrollY < 72 || delta < -5 || header?.matches(":focus-within") || dialog?.open;
      const shouldHide = currentScrollY > 120 && delta > 7 && !dialog?.open;
      if (shouldShow) {
        header?.classList.remove("is-hidden");
        document.documentElement.classList.remove("docs-header-hidden");
      } else if (shouldHide) {
        header?.classList.add("is-hidden");
        document.documentElement.classList.add("docs-header-hidden");
      }
      previousScrollY = currentScrollY;
      scrollFrame = 0;
    });
  },
  { passive: true },
);
document.addEventListener("click", (event) => {
  for (const menu of variantMenus) {
    if (!menu.contains(event.target as Node)) menu.open = false;
  }
});
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    for (const menu of variantMenus) menu.open = false;
  }
});
dialog?.addEventListener("click", (event) => {
  const target = event.target as HTMLElement;
  const queryButton = target.closest<HTMLButtonElement>("[data-search-query]");
  if (queryButton && input) {
    input.value = queryButton.dataset.searchQuery ?? "";
    input.focus();
    void search(input.value);
  }
  if (target.closest("[data-search-clear]") && input) {
    input.value = "";
    input.focus();
    void search("");
  }
  if (event.target === dialog) dialog.close();
});
dialog?.addEventListener("cancel", () => {
  input?.setAttribute("aria-expanded", "false");
});
dialog?.addEventListener("close", () => {
  if (dialog.open) return;
  input?.setAttribute("aria-expanded", "false");
  input?.removeAttribute("aria-activedescendant");
  setActive(-1);
  if (scrollLock) {
    const root = document.documentElement;
    const body = document.body;
    root.style.overflow = scrollLock.rootOverflow;
    body.style.overflow = scrollLock.bodyOverflow;
    root.style.overscrollBehavior = scrollLock.rootOverscroll;
    body.style.overscrollBehavior = scrollLock.bodyOverscroll;
    scrollLock = null;
  }
  const target =
    returnFocusTo?.isConnected && returnFocusTo.getClientRects().length
      ? returnFocusTo
      : openButton;
  target?.focus();
});
input?.addEventListener("input", () => void search(input.value));
results?.addEventListener("focusin", (event) => {
  const option = (event.target as HTMLElement).closest<HTMLAnchorElement>("[data-search-option]");
  const index = option ? visibleOptions.indexOf(option) : -1;
  if (index >= 0) setActive(index);
});
input?.addEventListener("keydown", (event) => {
  if (event.key === "ArrowDown") {
    event.preventDefault();
    setActive(activeIndex + 1);
  } else if (event.key === "ArrowUp") {
    event.preventDefault();
    setActive(activeIndex < 0 || activeIndex === 0 ? visibleOptions.length - 1 : activeIndex - 1);
  } else if (event.key === "Home" && visibleOptions.length) {
    event.preventDefault();
    setActive(0);
  } else if (event.key === "End" && visibleOptions.length) {
    event.preventDefault();
    setActive(visibleOptions.length - 1);
  } else if (event.key === "PageDown" && visibleOptions.length) {
    event.preventDefault();
    setActive(Math.min(visibleOptions.length - 1, (activeIndex < 0 ? 0 : activeIndex) + 5));
  } else if (event.key === "PageUp" && visibleOptions.length) {
    event.preventDefault();
    setActive(Math.max(0, (activeIndex < 0 ? 0 : activeIndex) - 5));
  } else if (event.key === "Enter" && activeIndex >= 0) {
    event.preventDefault();
    visibleOptions[activeIndex].click();
  } else if (event.key === "Escape") {
    event.preventDefault();
    dialog?.close();
  }
});
document.addEventListener("keydown", (event) => {
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
    event.preventDefault();
    openSearch();
  }
});
