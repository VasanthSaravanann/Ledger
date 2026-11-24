/**
 * The one place the public address is written down. Every route declares it as
 * its canonical URL and its og:url, so a share of `/audit` points at `/audit`
 * rather than at whatever page the crawler happened to fetch first.
 */
export const SITE_URL = "https://ledger-pilot-chi.vercel.app";

export function canonical(path: string) {
  const url = `${SITE_URL}${path === "/" ? "/" : path}`;
  return { meta: [{ property: "og:url", content: url }], links: [{ rel: "canonical", href: url }] };
}
