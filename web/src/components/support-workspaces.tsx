import { useRef, useState } from "react";
import { Link } from "@tanstack/react-router";
import Papa from "papaparse";
import { zipSync, strToU8, unzipSync, strFromU8 } from "fflate";
import {
  Upload,
  FileSpreadsheet,
  ArrowRight,
  Download,
  Mail,
  ShieldCheck,
  LockKeyhole,
  Info,
  Check,
  FileText,
  Clock3,
  Archive,
  ChevronRight,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { LedgerLayout, PageHeading, PageFooter, RequisitionBar, Status } from "./ledger-layout";
import { useLedger } from "./ledger-context";
import { downloadFile, emailPreview, isInitialScreen, type Candidate } from "@/lib/ledger";
const fields = [
  "APPLICATION_ID",
  "FIRST_NAME",
  "EMAIL",
  "STAGE",
  "KO_ANSWER_01",
  "KO_ANSWER_02",
  "KO_ANSWER_03",
  "KO_ANSWER_04",
  "KO_ANSWER_05",
];
const aliases: Record<string, string[]> = {
  APPLICATION_ID: ["applicationid", "candidateid", "id"],
  FIRST_NAME: ["firstname", "name", "candidatename"],
  EMAIL: ["email", "emailaddress"],
  STAGE: ["stage", "applicationstage", "status"],
};
export function ImportWorkspace() {
  const { importCandidates } = useLedger();
  const input = useRef<HTMLInputElement>(null);
  const [profile, setProfile] = useState("Workday");
  const [headers, setHeaders] = useState<string[]>([]);
  const [data, setData] = useState<Record<string, string>[]>([]);
  const [mapping, setMapping] = useState<Record<string, string>>({});
  const [file, setFile] = useState("");
  const [error, setError] = useState("");
  const [result, setResult] = useState("");
  const load = (f?: File) => {
    if (!f) return;
    setError("");
    setResult("");
    Papa.parse<Record<string, string>>(f, {
      header: true,
      skipEmptyLines: "greedy",
      complete: (results) => {
        if (results.errors.length) {
          setError(results.errors[0]?.message ?? "Invalid CSV");
          return;
        }
        const h = results.meta.fields ?? [];
        if (!h.length) {
          setError("CSV has no headers.");
          return;
        }
        setHeaders(h);
        setData(results.data);
        setFile(f.name);
        setMapping(
          Object.fromEntries(
            fields.map((field) => [
              field,
              h.find(
                (column) =>
                  column.toUpperCase() === field ||
                  (aliases[field] ?? []).includes(column.toLowerCase().replace(/[^a-z0-9]/g, "")),
              ) ?? "",
            ]),
          ),
        );
      },
    });
  };
  const submit = () => {
    try {
      for (const field of fields.slice(0, 4))
        if (!mapping[field]) throw new Error(`Map ${field} before importing.`);
      const rows: Candidate[] = [];
      let excluded = 0;
      const ids = new Set<string>();
      for (const row of data) {
        const get = (f: string) => row[mapping[f] ?? ""]?.trim() ?? "";
        if (get("STAGE") !== "InitialScreen") {
          excluded++;
          continue;
        }
        const id = get("APPLICATION_ID"),
          name = get("FIRST_NAME"),
          email = get("EMAIL");
        if (!id || !name || !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email))
          throw new Error("Every included row needs an application ID, name and valid email.");
        if (ids.has(id)) throw new Error(`Duplicate application ID: ${id}`);
        ids.add(id);
        rows.push({
          id,
          name,
          email,
          applied: "24 Nov 2026",
          source: profile,
          status: "Pending",
          reason: "",
          delivery: "",
          evidence: get("KO_ANSWER_02").toLowerCase() === "yes",
        });
      }
      if (!rows.length)
        throw new Error(
          "No InitialScreen candidates found. Stage values must match InitialScreen.",
        );
      importCandidates(rows, excluded);
      setResult(`${rows.length} candidates imported. ${excluded} non-InitialScreen rows excluded.`);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Import failed.");
    }
  };
  return (
    <LedgerLayout>
      <div className="page">
        <PageHeading
          title="Import candidates"
          subtitle="Bring your ATS roster into a consistent, review-ready workspace."
        />
        <RequisitionBar />
        <div className="info-band">
          <Info size={17} className="shrink-0 mt-0.5" />
          <span>
            Only candidates at <strong>InitialScreen</strong> are included. Importing replaces the
            current preview roster.
          </span>
        </div>
        <label className="form-label" htmlFor="ats">
          ATS mapping profile
        </label>
        <select
          id="ats"
          className="form-select max-w-xs"
          value={profile}
          onChange={(e) => setProfile(e.target.value)}
        >
          {["Workday", "Greenhouse", "PageUp", "SmartRecruiters"].map((p) => (
            <option key={p}>{p}</option>
          ))}
        </select>
        <div
          className="upload-zone"
          onDragOver={(e) => e.preventDefault()}
          onDrop={(e) => {
            e.preventDefault();
            load(e.dataTransfer.files[0]);
          }}
        >
          <FileSpreadsheet className="mx-auto text-primary mb-4" size={34} />
          <h2 className="font-semibold mb-2">{file || "Drop your candidate CSV here"}</h2>
          <p className="text-xs text-muted-foreground mb-5">
            {file
              ? `${data.length} rows · ${headers.length} columns`
              : "Workday, Greenhouse, PageUp or SmartRecruiters export"}
          </p>
          <Button variant="outline" onClick={() => input.current?.click()}>
            <Upload />
            {file ? "Choose another file" : "Choose CSV file"}
          </Button>
          <p className="text-[11px] text-muted-foreground mt-4">
            No export handy?{" "}
            <a
              className="underline underline-offset-2 hover:text-foreground"
              href="/sample-ats.csv"
              download
            >
              Download an eight-row sample CSV
            </a>{" "}
            — two rows fall outside the initial screen and are dropped on import.
          </p>
          <input
            ref={input}
            type="file"
            accept=".csv,text/csv"
            className="hidden"
            aria-label="Upload candidate CSV"
            onChange={(e) => load(e.target.files?.[0])}
          />
        </div>
        {headers.length > 0 && (
          <>
            <h2 className="text-base font-semibold">Column mapping</h2>
            <div className="mapping-grid">
              {fields.map((f) => (
                <div key={f}>
                  <label className="form-label" htmlFor={f}>
                    {f}
                    {fields.indexOf(f) < 4 ? " *" : ""}
                  </label>
                  <select
                    id={f}
                    className="form-select"
                    value={mapping[f] ?? ""}
                    onChange={(e) => setMapping((m) => ({ ...m, [f]: e.target.value }))}
                  >
                    <option value="">Not mapped</option>
                    {headers.map((h) => (
                      <option key={h}>{h}</option>
                    ))}
                  </select>
                </div>
              ))}
            </div>
            <div className="flex justify-end mt-7">
              <Button onClick={submit}>
                Import {data.length} rows
                <ArrowRight />
              </Button>
            </div>
          </>
        )}
        {error && (
          <p role="alert" className="bg-fail-surface text-fail p-4 text-xs rounded-md mt-4">
            {error}
          </p>
        )}
        {result && (
          <div
            role="status"
            className="bg-pass-surface text-pass p-4 text-xs rounded-md mt-4 flex justify-between items-center"
          >
            {result}
            <Button asChild variant="ghost">
              <Link to="/triage">
                View candidates
                <ArrowRight />
              </Link>
            </Button>
          </div>
        )}
        <PageFooter />
      </div>
    </LedgerLayout>
  );
}
export function DispatchWorkspace() {
  const { candidates, markSent } = useLedger();
  const [selected, setSelected] = useState<string[]>([]);
  const [filter, setFilter] = useState("Queued");
  const [preview, setPreview] = useState<Candidate | null>(null);
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);
  const verifyInput = useRef<HTMLInputElement>(null);
  const verifyBundle = async (file?: File) => {
    if (!file) return;
    try {
      if (file.size > 20_000_000) throw new Error("Bundle is too large.");
      const files = unzipSync(new Uint8Array(await file.arrayBuffer()));
      const raw = files["manifest.json"];
      if (!raw) throw new Error("Manifest is missing.");
      const manifest = JSON.parse(strFromU8(raw)) as { files?: { file: string; sha256: string }[] };
      if (!Array.isArray(manifest.files) || !manifest.files.length)
        throw new Error("Manifest has no files.");
      for (const entry of manifest.files) {
        const bytes = files[entry.file];
        if (!bytes) throw new Error(`Missing draft: ${entry.file}`);
        const hash = await crypto.subtle.digest("SHA-256", new Uint8Array(bytes).buffer);
        const sha = Array.from(new Uint8Array(hash))
          .map((b) => b.toString(16).padStart(2, "0"))
          .join("");
        if (sha !== entry.sha256) throw new Error(`Digest mismatch: ${entry.file}`);
      }
      setNotice(
        `Bundle integrity verified: ${manifest.files.length} draft digests match. This does not verify ledger signatures.`,
      );
    } catch (e) {
      setNotice(`Integrity check failed: ${e instanceof Error ? e.message : "Invalid bundle."}`);
    }
  };
  const rows = candidates.filter(
    (c) => c.status !== "Pending" && (filter === "All" || c.delivery === filter),
  );
  const exportBundle = async () => {
    setBusy(true);
    try {
      const output = candidates.filter((c) =>
        selected.length ? selected.includes(c.id) : c.delivery === "Queued",
      );
      if (!output.length) {
        setNotice("No drafts to export.");
        return;
      }
      const files: Record<string, Uint8Array> = {};
      const manifest = [];
      for (const c of output) {
        const email = emailPreview(c, c.reason);
        const contents = strToU8(
          `To: ${c.email}\r\nFrom: People Team <people@example.com>\r\nSubject: ${email.subject}\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\nX-Unsent: 1\r\n\r\n${email.body.replaceAll("\n", "\r\n")}\r\n`,
        );
        const hash = await crypto.subtle.digest("SHA-256", contents);
        const sha = Array.from(new Uint8Array(hash))
          .map((b) => b.toString(16).padStart(2, "0"))
          .join("");
        files[`${c.id}.eml`] = contents;
        manifest.push({ application_id: c.id, file: `${c.id}.eml`, sha256: sha, status: "Queued" });
      }
      files["manifest.json"] = strToU8(JSON.stringify({ preview: true, files: manifest }, null, 2));
      const zip = zipSync(files);
      downloadFile("ledger-drafts.zip", new Uint8Array(zip).buffer, "application/zip");
      setNotice(`${output.length} drafts exported with SHA-256 manifest. No emails sent.`);
    } catch {
      setNotice("Export failed. Please try again.");
    } finally {
      setBusy(false);
    }
  };
  return (
    <LedgerLayout>
      <div className="page">
        <PageHeading
          title="Dispatch queue"
          subtitle="Considerate communication. Prepared locally, delivered deliberately."
          action={
            <Button onClick={exportBundle} disabled={busy}>
              <Download />
              {busy ? "Preparing…" : "Export draft bundle"}
            </Button>
          }
        />
        <RequisitionBar />
        <div className="info-band">
          <Mail size={17} className="shrink-0 mt-0.5" />
          <span>
            Drafts are not sent automatically. Sample sender: <strong>people@example.com</strong>.
            Delivery confirmations are session-only.
          </span>
        </div>
        <div className="filters mt-6">
          <div className="filter-tabs">
            {["Queued", "Sent", "All"].map((f) => (
              <Button
                key={f}
                variant="ghost"
                data-active={filter === f}
                onClick={() => setFilter(f)}
              >
                {f}
                <span className="filter-count">
                  {
                    candidates.filter(
                      (c) => c.status !== "Pending" && (f === "All" || c.delivery === f),
                    ).length
                  }
                </span>
              </Button>
            ))}
          </div>
          <Button
            variant="outline"
            disabled={!selected.length}
            onClick={() => {
              markSent(selected);
              setNotice(`${selected.length} deliveries confirmed in this preview.`);
              setSelected([]);
            }}
          >
            <Check />
            Confirm sent
          </Button>
        </div>
        <div className="table-scroll">
          <table className="candidate-table">
            <thead>
              <tr>
                <th />
                <th>Candidate</th>
                <th>Decision</th>
                <th>Status</th>
                <th>Draft</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((c) => (
                <tr key={c.id}>
                  <td>
                    <Checkbox
                      aria-label={`Select ${c.name}`}
                      checked={selected.includes(c.id)}
                      onCheckedChange={(v) =>
                        setSelected((s) => (v ? [...s, c.id] : s.filter((id) => id !== c.id)))
                      }
                    />
                  </td>
                  <td>
                    <div className="person-name">{c.name}</div>
                    <div className="person-email">{c.email}</div>
                  </td>
                  <td>{c.reason}</td>
                  <td>
                    <Status value={c.delivery} />
                  </td>
                  <td>
                    <Button variant="ghost" size="sm" onClick={() => setPreview(c)}>
                      <Mail />
                      Preview
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {!rows.length && <div className="empty-state">No drafts in this view.</div>}
        </div>
        {notice && (
          <p role="status" className="info-band mt-5">
            {notice}
          </p>
        )}
        {preview && (
          <section className="mt-8">
            <div className="flex justify-between items-center">
              <h2 className="font-semibold">Email preview · {preview.name}</h2>
              <Button variant="ghost" onClick={() => setPreview(null)}>
                Close
              </Button>
            </div>
            <div className="email-preview">
              <header>{emailPreview(preview, preview.reason).subject}</header>
              <p>{emailPreview(preview, preview.reason).body}</p>
            </div>
          </section>
        )}
        <PageFooter />
      </div>
    </LedgerLayout>
  );
}
const artefacts = [
  ["Gate library", "Eligibility requirements and knockout rules", "gate_library"],
  ["Email templates", "Candidate communication and decision language", "templates"],
  ["Recruitment playbook", "Operating procedures and disposition guidance", "playbook"],
  ["Requisition facts", "Role requirements and evidence", "requisition"],
  ["Comparative framework", "Consistent assessment criteria", "comparative"],
];
const checklist = [
  "Confirm requisition scope",
  "Review eligibility requirements",
  "Validate knockout evidence",
  "Review comparative criteria",
  "Approve candidate templates",
  "Confirm roster source",
  "Validate column mapping",
  "Exclude non-InitialScreen rows",
  "Review disposition reasons",
  "Check candidate email previews",
  "Review dispatch manifest",
  "Confirm delivery records",
  "Complete audit handover",
];
export function GovernanceWorkspace() {
  const [attested, setAttested] = useState<number[]>([]);
  return (
    <LedgerLayout>
      <div className="page">
        <PageHeading
          title="Governance"
          subtitle="Clear standards. Deliberate approvals. Confidence in every decision."
        />
        <RequisitionBar />
        <div className="info-band">
          <LockKeyhole size={17} className="shrink-0 mt-0.5" />
          <span>
            Coordinator preview · Artefact signing requires an Approver session in the connected
            Rust ledger.
          </span>
        </div>
        <div className="flex justify-between items-center mt-8 mb-2">
          <h2 className="text-base font-semibold">Hash-bound artefacts</h2>
          <span className="text-xs text-muted-foreground">0 / 5 signed</span>
        </div>
        {artefacts.map(([title, description, key]) => (
          <div className="section-row" key={key}>
            <div className="flex gap-3 items-center">
              <FileText size={20} className="text-muted-foreground" />
              <div>
                <h3 className="font-semibold text-sm">{title}</h3>
                <p className="text-xs text-muted-foreground mt-1">{description}</p>
                <p className="font-mono text-[10px] text-muted-foreground mt-2">
                  {key} · No connected hash
                </p>
              </div>
            </div>
            <div className="flex gap-3 items-center">
              <Status value="Unsigned" />
              <Button
                variant="outline"
                size="sm"
                disabled
                title="Approver session and Rust ledger required"
              >
                <LockKeyhole />
                Approve
              </Button>
            </div>
          </div>
        ))}
        <div className="flex justify-between items-center mt-10 mb-3">
          <h2 className="text-base font-semibold">Operational checklist</h2>
          <span className="status FIELD">{attested.length}/13 · Attested (Unsigned)</span>
        </div>
        {checklist.map((text, i) => (
          <label key={text} className="section-row cursor-pointer !justify-start">
            <Checkbox
              checked={attested.includes(i)}
              onCheckedChange={(v) =>
                setAttested((s) => (v ? [...s, i] : s.filter((x) => x !== i)))
              }
            />
            <span className="text-sm">{text}</span>
          </label>
        ))}
        <PageFooter />
      </div>
    </LedgerLayout>
  );
}
export function AuditWorkspace() {
  const { candidates, events } = useLedger();
  const [notice, setNotice] = useState("");
  return (
    <LedgerLayout>
      <div className="page">
        <PageHeading
          title="Audit trail"
          subtitle="A transparent record of decisions, communication and workspace activity."
          action={
            <Button
              variant="outline"
              onClick={() => {
                downloadFile(
                  "ledger-preview-snapshot.json",
                  JSON.stringify({ preview: true, candidates, events }, null, 2),
                );
                setNotice("Preview snapshot exported. This is not a signed ledger backup.");
              }}
            >
              <Archive />
              Export preview snapshot
            </Button>
          }
        />
        <RequisitionBar />
        <div className="info-band">
          <ShieldCheck size={20} className="shrink-0" />
          <div>
            <strong>Chain verification unavailable</strong>
            <p>
              No Rust append log or operator signatures are connected. The activity below reflects
              this preview session only.
            </p>
          </div>
        </div>
        <div className="grid grid-cols-1 sm:grid-cols-3 gap-6 border-b border-border py-7">
          <div>
            <p className="text-xs text-muted-foreground">Append chain</p>
            <p className="font-semibold mt-2">Not connected</p>
          </div>
          <div>
            <p className="text-xs text-muted-foreground">Operator signatures</p>
            <p className="font-semibold mt-2">Not verified</p>
          </div>
          <div>
            <p className="text-xs text-muted-foreground">Preview activity</p>
            <p className="font-semibold mt-2">{events.length} events</p>
          </div>
        </div>
        <h2 className="text-base font-semibold mt-8 mb-3">Recent activity</h2>
        {events.map((event, i) => (
          <div className="section-row" key={`${event}-${i}`}>
            <div className="flex items-center gap-4">
              <span className="avatar">
                <Clock3 size={15} />
              </span>
              <div>
                <p className="text-sm font-medium">{event}</p>
                <p className="text-[11px] text-muted-foreground mt-1">
                  Vasanth S · Coordinator preview
                </p>
              </div>
            </div>
            <span className="font-mono text-[10px] text-muted-foreground">
              EVT-{String(events.length - i).padStart(4, "0")}
            </span>
          </div>
        ))}
        {notice && (
          <p role="status" className="info-band mt-6">
            {notice}
          </p>
        )}
        <PageFooter />
      </div>
    </LedgerLayout>
  );
}
