import { Link } from "@tanstack/react-router";
import { Layers3 } from "lucide-react";

/**
 * The one brand lockup, shared by the workspace sidebar and the landing bar.
 * The indigo tile carries the symbol; the dot sits tight against the wordmark,
 * so the flex gap on `.brand` does not open a space before it.
 */
export function BrandMark({ to = "/", title = "Ledger" }: { to?: string; title?: string }) {
  return (
    <Link to={to} className="brand" title={title}>
      <span className="brand-symbol">
        <Layers3 size={21} />
      </span>
      <span className="brand-word">
        ledger<span className="text-primary">.</span>
      </span>
    </Link>
  );
}
