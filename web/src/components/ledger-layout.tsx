import { useEffect, useRef, useState, type ReactNode } from "react";
import { Link, useRouterState } from "@tanstack/react-router";
import {
  LayoutDashboard,
  Upload,
  Send,
  ShieldCheck,
  ScrollText,
  ChevronDown,
  ChevronRight,
  LockKeyhole,
  Menu,
  X,
  CircleHelp,
  ArrowUpRight,
  BriefcaseBusiness,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { useLedger } from "./ledger-context";
import { BrandMark } from "./brand-mark";
const nav = [
  { to: "/triage", label: "Triage workspace", icon: LayoutDashboard },
  { to: "/import", label: "Import candidates", icon: Upload },
  { to: "/dispatch", label: "Dispatch queue", icon: Send },
  { to: "/governance", label: "Governance", icon: ShieldCheck },
  { to: "/audit", label: "Audit trail", icon: ScrollText },
] as const;
/**
 * The first tab stop on every route: off-screen until it has focus, then it jumps to
 * the page's `main`. The target carries `tabIndex={-1}` so focus actually moves.
 */
export function SkipLink() {
  return (
    <a href="#main" className="skip-link">
      Skip to content
    </a>
  );
}
export function Status({ value }: { value: string }) {
  return (
    <span className={`status ${value}`}>
      <span className="text-[8px]">●</span>
      {value}
    </span>
  );
}
export function LedgerLayout({ children }: { children: ReactNode }) {
  const path = useRouterState({ select: (s) => s.location.pathname });
  const [open, setOpen] = useState(false);
  const closeRef = useRef<HTMLButtonElement>(null);
  // Opening the drawer moves focus into it, so the menu button's promise is kept:
  // the next Tab walks the navigation, not the page underneath it.
  useEffect(() => {
    if (open) closeRef.current?.focus();
  }, [open]);
  const { candidates } = useLedger();
  const current = nav.find((n) => n.to === path)?.label ?? "Workspace";
  return (
    <div className="app-shell">
      <SkipLink />
      {open && <div className="drawer-backdrop md:hidden" onClick={() => setOpen(false)} />}
      <aside
        id="workspace-nav"
        className={`sidebar ${open ? "mobile-open" : ""}`}
        onKeyDown={(event) => {
          if (event.key === "Escape") setOpen(false);
        }}
        onBlur={(event) => {
          // The drawer is not modal: the moment focus leaves it, it closes rather
          // than covering the control the focus landed on.
          if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setOpen(false);
        }}
      >
        <Button
          ref={closeRef}
          variant="ghost"
          size="icon"
          className="nav-close md:hidden"
          aria-label="Close navigation"
          onClick={() => setOpen(false)}
        >
          <X />
        </Button>
        <BrandMark />
        <div className="workspace-chip">
          <BriefcaseBusiness size={17} className="text-muted-foreground" />
          <div className="min-w-0 flex-1">
            <div className="text-xs font-semibold">People Operations</div>
            <div className="text-[10px] text-muted-foreground mt-1">Recruitment workspace</div>
          </div>
          <LockKeyhole size={12} className="text-muted-foreground" />
        </div>
        <div className="nav-label">Workspace</div>
        <nav>
          {nav.map((n) => (
            <Link
              key={n.to}
              to={n.to}
              onClick={() => setOpen(false)}
              className={`nav-link ${path === n.to ? "active" : ""}`}
            >
              <n.icon />
              {n.label}
              {n.to === "/dispatch" && (
                <span className="nav-count">
                  {candidates.filter((c) => c.delivery === "Queued").length}
                </span>
              )}
            </Link>
          ))}
        </nav>
        <div className="mt-7 mx-6 border-t border-border pt-5">
          <div className="flex items-center gap-2 text-xs font-medium">
            <LockKeyhole size={14} className="text-muted-foreground" />
            Single-operator workspace
          </div>
          <div className="text-[11px] text-muted-foreground mt-2 leading-relaxed">
            Session-bound access
          </div>
        </div>
        <div className="sidebar-bottom">
          <div className="flex items-center gap-2 mb-5 text-[11px] text-muted-foreground">
            <span className="size-1.5 rounded-full bg-warn" />
            Sample workspace
          </div>
          <div className="person">
            <div className="avatar" data-tone="1">
              J
            </div>
            <div>
              <div className="text-xs font-semibold">John</div>
              <div className="text-[10px] text-muted-foreground mt-1">Coordinator</div>
            </div>
            <LockKeyhole size={13} className="ml-auto text-muted-foreground" />
          </div>
        </div>
      </aside>
      <main id="main" tabIndex={-1} className="main-shell">
        <header className="topbar">
          <div className="topbar-path">
            <Button
              variant="ghost"
              size="icon"
              className="md:hidden"
              aria-label="Open navigation"
              aria-expanded={open}
              aria-controls="workspace-nav"
              onClick={() => setOpen(true)}
            >
              <Menu />
            </Button>
            <span className="hidden sm:inline">Workspace</span>
            <ChevronRight size={13} className="hidden sm:inline" />
            <span className="text-foreground font-medium">{current}</span>
          </div>
          <div className="flex items-center gap-4">
            <span className="status FIELD">
              <LockKeyhole size={10} />
              UI preview
            </span>
            <Button
              variant="ghost"
              size="icon"
              aria-label="Workspace information"
              title="Changes last for this session only. Rust ledger is not connected."
              onClick={() =>
                window.alert(
                  "Sample workspace. Changes last for this session only. Rust storage, session security, and cryptographic verification are not connected.",
                )
              }
            >
              <CircleHelp className="text-muted-foreground" />
            </Button>
          </div>
        </header>
        {children}
      </main>
    </div>
  );
}
export function PageHeading({
  title,
  subtitle,
  action,
}: {
  title: string;
  subtitle: string;
  action?: ReactNode;
}) {
  return (
    <div className="page-heading">
      <div>
        <h1>{title}</h1>
        <p className="page-subtitle">{subtitle}</p>
      </div>
      {action}
    </div>
  );
}
export function PageFooter() {
  return (
    <footer className="page-footer">
      <span className="flex items-center gap-2">
        <LockKeyhole size={12} />
        Sample data · Not connected to the ledger
      </span>
      <span>
        ledger ui <span className="mx-2">/</span> Recruitment with a clear record.
      </span>
    </footer>
  );
}
export function RequisitionBar() {
  return (
    <div className="requisition-bar">
      <span className="font-semibold flex items-center gap-2">
        <BriefcaseBusiness size={14} className="text-muted-foreground" />
        Senior Product Designer
        <ChevronDown size={12} />
      </span>
      <span className="text-muted-foreground font-mono text-[10px]">REQ-2026-042</span>
      <span className="border-l border-border pl-4 text-muted-foreground">Design & Experience</span>
      <span className="status Valid">Open requisition</span>
    </div>
  );
}
export { ArrowUpRight, X };
