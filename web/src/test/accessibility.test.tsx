import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { render } from "@testing-library/react";
import { createElement, type ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";

// The preview's routes render `Link`, and the shell reads the current path. Both need
// a router we are not testing here: render a plain anchor and a fixed path instead.
vi.mock("@tanstack/react-router", async () => {
  const actual =
    await vi.importActual<typeof import("@tanstack/react-router")>("@tanstack/react-router");
  return {
    ...actual,
    Link: ({
      to,
      children,
      ...rest
    }: { to: string; children?: ReactNode } & Record<string, unknown>) =>
      createElement("a", { href: to, ...rest }, children),
    useRouterState: <T,>({
      select,
    }: {
      select: (state: { location: { pathname: string } }) => T;
    }): T => select({ location: { pathname: "/triage" } }),
  };
});

import { LandingPage } from "@/components/landing-page";
import { LedgerProvider } from "@/components/ledger-context";
import { LedgerLayout } from "@/components/ledger-layout";

function focusables(root: HTMLElement): HTMLElement[] {
  return Array.from(
    root.querySelectorAll<HTMLElement>(
      'a[href], button, input:not([type="hidden"]), select, textarea, [tabindex]:not([tabindex="-1"])',
    ),
  ).filter((element) => !element.hasAttribute("disabled"));
}

describe("skip link", () => {
  it("is the first tab stop on the landing page and targets main", () => {
    const { container } = render(<LandingPage />);
    const first = focusables(container)[0];

    expect(first).toHaveClass("skip-link");
    expect(first).toHaveAttribute("href", "#main");
    expect(first).toHaveTextContent("Skip to content");

    const target = container.querySelector("#main");
    expect(target?.tagName).toBe("MAIN");
    expect(target).toHaveAttribute("tabindex", "-1");
  });

  it("is the first tab stop in the workspace and targets main", () => {
    const { container } = render(
      <LedgerProvider>
        <LedgerLayout>
          <div />
        </LedgerLayout>
      </LedgerProvider>,
    );
    const first = focusables(container)[0];

    expect(first).toHaveClass("skip-link");
    expect(first).toHaveAttribute("href", "#main");

    const target = container.querySelector("#main");
    expect(target?.tagName).toBe("MAIN");
    expect(target).toHaveAttribute("tabindex", "-1");
  });
});

describe("stylesheet floor", () => {
  // jsdom does not apply the stylesheet, and measuring a 44px tap target needs a
  // browser (the taste gate at 320 and 390). What can be checked here is that the
  // rules the browser will read are present and say what the contract needs.
  const css = readFileSync(resolve(__dirname, "../styles.css"), "utf8").replace(/\s+/g, " ");

  it("hides the skip link until it is focused", () => {
    expect(css).toContain(".skip-link {");
    expect(css).toContain("transform: translateY(-200%)");
    expect(css).toMatch(/\.skip-link:(?:focus|focus-visible) [^}]*transform: translateY\(0\)/);
  });

  it("draws a visible focus ring", () => {
    expect(css).toContain(":focus-visible");
    expect(css).toContain("outline: 2px solid");
  });

  it("holds every link and control to 44px at phone widths", () => {
    expect(css).toContain("@media (max-width: 480px)");
    expect(css).toContain("min-height: 44px");
    expect(css).toContain("min-width: 44px");
  });
});
