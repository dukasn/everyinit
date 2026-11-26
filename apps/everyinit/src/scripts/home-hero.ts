class HomeHeroTabs extends HTMLElement {
  private controller?: AbortController;

  connectedCallback() {
    if (this.controller) return;

    this.controller = new AbortController();
    const { signal } = this.controller;
    const tabs = [...this.querySelectorAll<HTMLButtonElement>('[role="tab"]')];
    const panels = [...this.querySelectorAll<HTMLElement>('[role="tabpanel"]')];

    const activate = (nextTab: HTMLButtonElement, moveFocus = false) => {
      const target = nextTab.dataset.heroTab;
      tabs.forEach((tab) => {
        const selected = tab === nextTab;
        tab.setAttribute("aria-selected", String(selected));
        tab.tabIndex = selected ? 0 : -1;
      });
      panels.forEach((panel) => {
        const active = panel.dataset.heroPanel === target;
        panel.dataset.active = String(active);
        panel.inert = !active;
      });
      if (moveFocus) nextTab.focus();
    };

    tabs.forEach((tab, index) => {
      tab.addEventListener("click", () => activate(tab), { signal });
      tab.addEventListener(
        "keydown",
        (event) => {
          let nextIndex = -1;
          if (event.key === "ArrowRight") nextIndex = (index + 1) % tabs.length;
          else if (event.key === "ArrowLeft") nextIndex = (index - 1 + tabs.length) % tabs.length;
          else if (event.key === "Home") nextIndex = 0;
          else if (event.key === "End") nextIndex = tabs.length - 1;

          if (nextIndex >= 0) {
            event.preventDefault();
            activate(tabs[nextIndex], true);
          }
        },
        { signal },
      );
    });
  }

  disconnectedCallback() {
    this.controller?.abort();
    this.controller = undefined;
  }
}

if (!customElements.get("home-hero-tabs")) {
  customElements.define("home-hero-tabs", HomeHeroTabs);
}
