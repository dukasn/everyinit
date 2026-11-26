import type { APIRoute } from "astro";
import docs from "virtual:evin/docs";

const docsUrl = (path: string) =>
  `https://everyinit.dev${path === "/" ? "/docs/" : `/docs${path.replace(/\/+$/, "")}/`}`;

export const GET: APIRoute = () => {
  const routes = docs.routes
    .filter((route) => route.version === docs.manifest.latest_version && !route.is_fallback)
    .filter((route) => route.path !== "/")
    .sort((left, right) => left.path.localeCompare(right.path));
  const entries = routes.map((route) => {
    const metadata = route.page.page.metadata;
    const title = String(metadata.title ?? route.page.page.slug ?? route.path);
    const description = String(metadata.description ?? "")
      .replaceAll(/\s+/g, " ")
      .trim();
    return `- [${title}](${docsUrl(route.path)})${description ? `: ${description}` : ""}`;
  });
  const body = [
    "# Everyinit documentation",
    "",
    "Everyinit coordinates repository workflows through the Evin task runner, Git hooks, IDE integrations, and CI. ESH provides scripting for tasks that need control flow and data handling.",
    "",
    "## Start here",
    "",
    `- [Documentation overview](${docsUrl("/")})`,
    `- [Why Everyinit](${docsUrl("/book/concepts/why-everyinit")})`,
    `- [Execution & trust model](${docsUrl("/book/concepts/execution-and-trust-model")})`,
    `- [Installation](${docsUrl("/book/installation")})`,
    `- [Your first workflow](${docsUrl("/book/first-workflow")})`,
    "",
    "## Guides and reference",
    "",
    ...entries,
    "",
  ].join("\n");

  return new Response(body, {
    headers: { "Content-Type": "text/plain; charset=utf-8" },
  });
};
