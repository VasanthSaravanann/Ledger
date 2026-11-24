import { createContext, useContext, useState, type ReactNode } from "react";
import { initialCandidates, disposeBatch, type Candidate, type Disposition } from "@/lib/ledger";
type LedgerState = {
  candidates: Candidate[];
  excluded: number;
  events: string[];
  dispose: (ids: string[], status: Exclude<Disposition, "Pending">, reason: string) => void;
  importCandidates: (rows: Candidate[], excluded: number) => void;
  markSent: (ids: string[]) => void;
};
const LedgerContext = createContext<LedgerState | null>(null);
export function LedgerProvider({ children }: { children: ReactNode }) {
  const [candidates, setCandidates] = useState(initialCandidates);
  const [excluded, setExcluded] = useState(7);
  const [events, setEvents] = useState([
    "Sample workspace opened",
    "Sample roster loaded · 48 InitialScreen candidates",
    "7 non-InitialScreen rows excluded from sample roster",
  ]);
  const log = (text: string) => setEvents((e) => [text, ...e]);
  const dispose = (ids: string[], status: Exclude<Disposition, "Pending">, reason: string) => {
    const next = disposeBatch(candidates, ids, status, reason);
    setCandidates(next);
    log(`${ids.length} candidate${ids.length === 1 ? "" : "s"} disposed · ${reason}`);
  };
  const importCandidates = (rows: Candidate[], count: number) => {
    setCandidates(rows);
    setExcluded(count);
    log(`CSV imported · ${rows.length} candidates · ${count} excluded`);
  };
  const markSent = (ids: string[]) => {
    setCandidates((rows) => rows.map((c) => (ids.includes(c.id) ? { ...c, delivery: "Sent" } : c)));
    log(`${ids.length} deliveries manually confirmed`);
  };
  return (
    <LedgerContext.Provider
      value={{ candidates, excluded, events, dispose, importCandidates, markSent }}
    >
      {children}
    </LedgerContext.Provider>
  );
}
export function useLedger() {
  const value = useContext(LedgerContext);
  if (!value) throw new Error("LedgerProvider is required");
  return value;
}
