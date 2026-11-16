import { FluentBundle, FluentResource } from "@fluent/bundle";

const fallbackLocale = "en";
const catalogFiles = import.meta.glob<string>("./catalogs/*.ftl", {
  eager: true,
  import: "default",
  query: "?raw",
});

const catalogs = new Map(
  Object.entries(catalogFiles).map(([path, source]) => [
    path.slice("./catalogs/".length, -".ftl".length),
    source,
  ]),
);

if (!catalogs.has(fallbackLocale)) {
  throw new Error(`The Astro localization catalog '${fallbackLocale}.ftl' is required`);
}

const bundles = new Map<string, FluentBundle>();

export const availableLocales = [...catalogs.keys()].sort((left, right) =>
  left.localeCompare(right),
);

export function resolveLocale(locale: string | undefined): string {
  if (!locale) return fallbackLocale;

  let canonicalLocale: string;
  try {
    canonicalLocale = new Intl.Locale(locale).toString();
  } catch {
    return fallbackLocale;
  }

  if (catalogs.has(canonicalLocale)) return canonicalLocale;

  const language = new Intl.Locale(canonicalLocale).language;
  return catalogs.has(language) ? language : fallbackLocale;
}

export function createTranslator(locale: string | undefined) {
  const resolvedLocale = resolveLocale(locale);
  let bundle = bundles.get(resolvedLocale);

  if (!bundle) {
    bundle = new FluentBundle(resolvedLocale, { useIsolating: false });
    bundle.addResource(new FluentResource(catalogs.get(fallbackLocale)!));

    if (resolvedLocale !== fallbackLocale) {
      const errors = bundle.addResource(new FluentResource(catalogs.get(resolvedLocale)!), {
        allowOverrides: true,
      });
      if (errors.length) {
        throw new Error(`Could not load Astro localization catalog '${resolvedLocale}.ftl'`, {
          cause: errors[0],
        });
      }
    }

    bundles.set(resolvedLocale, bundle);
  }

  return (id: string, args?: Record<string, string | number | Date>): string => {
    const message = bundle.getMessage(id);
    if (!message?.value) return `[${id}]`;

    const errors: Error[] = [];
    const value = bundle.formatPattern(message.value, args, errors);
    return errors.length ? `[${id}]` : value;
  };
}
