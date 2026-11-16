class HomeNavigation extends HTMLElement {
  private controller?: AbortController;

  connectedCallback() {
    if (this.controller) return;

    this.controller = new AbortController();
    const { signal } = this.controller;
    const navMenus = [...this.querySelectorAll<HTMLDetailsElement>("details[data-nav-menu]")];

    navMenus.forEach((menu) =>
      menu.addEventListener(
        "toggle",
        () => {
          if (menu.open) {
            navMenus
              .filter((other) => other !== menu)
              .forEach((other) => {
                other.open = false;
              });
          } else {
            menu.querySelectorAll("details").forEach((nested) => {
              nested.open = false;
            });
          }
        },
        { signal },
      ),
    );

    document.addEventListener(
      "click",
      (event) => {
        if (event.target instanceof Node && !this.contains(event.target)) {
          navMenus.forEach((menu) => {
            menu.open = false;
          });
        }
      },
      { signal },
    );

    this.addEventListener(
      "click",
      (event) => {
        if (event.target instanceof Element && event.target.closest("nav a")) {
          navMenus.forEach((menu) => {
            menu.open = false;
          });
        }
      },
      { signal },
    );

    this.addEventListener(
      "keydown",
      (event) => {
        if (event.key === "Escape") {
          const openMenu = navMenus.find((menu) => menu.open);
          if (openMenu) {
            openMenu.open = false;
            openMenu.querySelector("summary")?.focus();
          }
        }
      },
      { signal },
    );
  }

  disconnectedCallback() {
    this.controller?.abort();
    this.controller = undefined;
  }
}

if (!customElements.get("home-navigation")) {
  customElements.define("home-navigation", HomeNavigation);
}
