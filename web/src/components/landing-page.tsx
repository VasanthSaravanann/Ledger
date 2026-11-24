import { useState } from "react";
import { Link } from "@tanstack/react-router";

/*
 * The public explainer for the pilot. It shares the workspace's tokens (paper, ink,
 * indigo, pass/warn/fail) but not its chrome: no sidebar, no cards, one document
 * column with a hanging margin that carries citations rather than decoration.
 */

const RECORD = [
  "Dear Ada,",
  "Thank you for applying for Registered Nurse at Example Health.",
  "Your application did not meet a requirement stated in the advertisement:",
  "This role runs until 30 June 2027 and requires work rights covering at least 12 months. The appointment is a fixed-term 12-month contract.",
  "In your application you indicated a position that does not meet this requirement. If the information above is wrong, reply to this email and a person will correct the record and reconsider your application: careers-feedback@example.com.au.",
  "This decision was made by a person on the information in your application, and was not produced by an automated assessment.",
  "Regards,\nR. Patel\nRecruitment Coordinator, Example Health",
];

const RECORD_TEXT = RECORD.join("\n\n");

const CITATIONS = [
  ["Gate", "GATE-02, work-rights duration"],
  ["Copy", "v1.2, signed by the hiring manager and Legal"],
  ["Disposition", "captured 09:02, corrected only by superseding"],
  ["Second check", "QA by qa-patel at 09:07"],
  ["Dispatch", "recorded queued, then confirmed by a person"],
];

const STEPS: { title: string; artifact: string; body: string }[] = [
  {
    title: "Document the requisition",
    artifact: "requisition facts",
    body: "Role end date, minimum term, the reason for the duration, the regulator, the location. A gate block whose facts are missing is unavailable, so the disposition has to fall back to FIELD instead of inventing a requirement.",
  },
  {
    title: "Sign the copy",
    artifact: "signed gate library",
    body: "Five knockouts, GATE-01 to GATE-05, each carrying a version. Hiring manager and Legal sign every one, and the signature binds to the hash of that copy, so an edit makes the approval stale rather than silently inherited.",
  },
  {
    title: "Capture the disposition",
    artifact: "disposition record",
    body: "One locked form: GATE, FIELD or ADMIN. No free text, no “other”, a named decision maker and a locked timestamp. A correction supersedes the earlier record; nothing is overwritten.",
  },
  {
    title: "Check it twice",
    artifact: "QA record",
    body: "Every GATE email and a fixed 10% sample of FIELD and ADMIN need a second person’s check before anything leaves. The sample is keyed on requisition and application, so it cannot be gamed.",
  },
  {
    title: "Render, then chain",
    artifact: "send log entry",
    body: "The tool renders an RFC 5322 draft, refuses to send without a disposition and a matching outcome, and appends one hash-chained entry per send. A draft is recorded queued; delivery is confirmed by a person, never assumed.",
  },
];

const BANDS = [
  ["Reply volume per 1,000 sends", "under 30", "30 to 80", "over 80"],
  ["Form-block rate", "under 10%", "10 to 25%", "over 25%"],
  ["Gate-mismatch rate", "under 2%", "2 to 10%", "over 10%"],
  ["Candidates who found the reply useful", "over 50%", "30 to 50%", "under 30%"],
];

const LIMITS = [
  "No model in the send path. A person selects the reason; the tool renders the text and nothing else writes to the log.",
  "No email sending. The tool produces drafts and a manifest, and leaves delivery to a mail client.",
  "No ATS integration. One CSV in, drafts out, with the initial-screen filter applied on import.",
  "No claim of delivery. A draft stays queued until someone records that it arrived.",
  "No hosted tenancy. The workspace is a directory the team holds, on macOS or Linux.",
];

function chunk(hex: string) {
  return hex.replace(/(.{8})/g, "$1 ").trim();
}

export function LandingPage() {
  const [digest, setDigest] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);

  async function verify() {
    setBusy(true);
    setFailed(false);
    try {
      const bytes = new TextEncoder().encode(RECORD_TEXT);
      const hash = await crypto.subtle.digest("SHA-256", bytes);
      setDigest(
        Array.from(new Uint8Array(hash))
          .map((byte) => byte.toString(16).padStart(2, "0"))
          .join(""),
      );
    } catch {
      setFailed(true);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="lp">
      <header className="lp-bar">
        <Link to="/" className="lp-word">
          ledger<span>.</span>
        </Link>
        <nav className="lp-bar-links" aria-label="Sections">
          <a href="#how">How it works</a>
          <a href="#why">Why it matters</a>
          <Link to="/triage" className="lp-enter">
            Open the workspace
          </Link>
        </nav>
      </header>

      <main className="lp-main">
        <section className="lp-hero">
          <h1>
            Every rejection
            <br />
            leaves a record.
          </h1>
          <p className="lp-deck">
            Ledger runs a manual candidate-feedback pilot: one employer, one requisition, everyone
            rejected at the initial screen told why. The reason is a code a person chose, the copy
            is text Legal signed, and the log proves the exact words that were sent.
          </p>
        </section>

        <section className="lp-row lp-record" aria-labelledby="record-title">
          <div className="lp-margin">
            <h2 id="record-title" className="lp-margin-title">
              The record
            </h2>
            <dl className="lp-cites">
              {CITATIONS.map(([term, detail]) => (
                <div key={term}>
                  <dt>{term}</dt>
                  <dd>{detail}</dd>
                </div>
              ))}
            </dl>
          </div>

          <div className="lp-doc">
            <div className="lp-doc-body">
              {RECORD.map((paragraph) => (
                <p
                  key={paragraph}
                  className={paragraph.startsWith("This role runs") ? "lp-quote" : undefined}
                >
                  {paragraph}
                </p>
              ))}
            </div>
            <div className="lp-doc-foot">
              <div>
                <button type="button" className="lp-verify" onClick={verify} disabled={busy}>
                  {busy ? "Hashing…" : digest ? "Hash it again" : "Hash this text"}
                </button>
                <p className="lp-hint">
                  {failed
                    ? "This browser would not compute the digest. The chain check itself runs in Rust."
                    : digest
                      ? "Recomputed here from the text above. The send log stores this value as text_digest; the chain that links every entry is verified in Rust and in a standalone Python script."
                      : "Computes SHA-256 over the text above: the same digest the send log stores for every entry."}
                </p>
              </div>
              <div className="lp-digest" aria-live="polite">
                <span className="lp-digest-label">text_digest</span>
                {digest ? (
                  <code>{chunk(digest)}</code>
                ) : (
                  <span className="lp-digest-empty">not computed yet</span>
                )}
              </div>
            </div>
            <p className="lp-doc-note">
              A sample record from the fixture: real gate copy, real template, invented candidate.
            </p>
          </div>
        </section>

        <section className="lp-row" id="what" aria-labelledby="what-title">
          <div className="lp-margin">
            <h2 className="lp-margin-title" id="what-title">
              What it is
            </h2>
          </div>
          <div className="lp-prose">
            <p>
              A rejection is a decision, and decisions of this kind get contested: by the candidate,
              by a union, by a regulator, by a hiring manager who does not remember the
              conversation. Ledger is built so that the answer to “why was I rejected, and who
              decided that?” is a record rather than a recollection.
            </p>
            <dl className="lp-parts">
              <div>
                <dt>The pilot</dt>
                <dd>
                  One employer, one requisition, the initial screen, every rejected candidate. No
                  sampling and no opt-in, because a pilot that tells only the persuasive candidates
                  is not evidence of anything.
                </dd>
              </div>
              <div>
                <dt>The record</dt>
                <dd>
                  A disposition code, the copy version that rendered it, the person who decided, the
                  second person who checked, and the digest of the text.
                </dd>
              </div>
              <div>
                <dt>The log</dt>
                <dd>
                  Append-only and hash-chained, readable and verifiable by a script that has no
                  access to the tool and no reason to trust it.
                </dd>
              </div>
            </dl>
          </div>
        </section>

        <section className="lp-row" id="how" aria-labelledby="how-title">
          <div className="lp-margin">
            <h2 className="lp-margin-title" id="how-title">
              How it works
            </h2>
            <p className="lp-margin-note">Five steps, in this order, each leaving a file behind.</p>
          </div>
          <ol className="lp-steps">
            {STEPS.map((step, index) => (
              <li key={step.title}>
                <div className="lp-step-margin">
                  <span className="lp-step-number">{String(index + 1).padStart(2, "0")}</span>
                  <span className="lp-step-artifact">{step.artifact}</span>
                </div>
                <div className="lp-step-body">
                  <h3>{step.title}</h3>
                  <p>{step.body}</p>
                </div>
              </li>
            ))}
          </ol>
        </section>

        <section className="lp-row" id="why" aria-labelledby="why-title">
          <div className="lp-margin">
            <h2 className="lp-margin-title" id="why-title">
              Why it matters
            </h2>
          </div>
          <div className="lp-prose">
            <p>
              <strong>Most rejections are silent.</strong> A candidate who hears nothing cannot tell
              a stated rule from a mistake, and an employer who sent nothing cannot show which one
              it was. Silence protects the employer until the first complaint, then protects nobody.
            </p>
            <p>
              <strong>“Not the right fit” cannot be audited.</strong> A reason that names no
              requirement cannot be checked, corrected or defended. A gate code points at a sentence
              in the advertisement and at the evidence that failed it, so the same decision reads
              the same way to the candidate, to Legal and to a reviewer a year later.
            </p>
            <p>
              <strong>Evidence decays.</strong> Without an append-only record, the account of what
              was said lives in a sent-items folder and someone’s memory. Ledger keeps the text and
              its digest in a chain where an edit or a deletion is visible to anyone who kept the
              last chain head.
            </p>

            <h3 className="lp-subhead">
              The pilot is judged on five measures, fixed before it started
            </h3>
            <p className="lp-lede">
              Four are numeric and are read after a hundred sends. The fifth is the decision the
              numbers force.
            </p>
            <table className="lp-bands">
              <caption className="lp-visually-hidden">
                Pre-registered metric bands for the pilot
              </caption>
              <thead>
                <tr>
                  <th scope="col">Measure</th>
                  <th scope="col">Green</th>
                  <th scope="col">Amber</th>
                  <th scope="col">Red</th>
                </tr>
              </thead>
              <tbody>
                {BANDS.map(([measure, green, amber, red]) => (
                  <tr key={measure}>
                    <th scope="row">{measure}</th>
                    <td data-band="pass">{green}</td>
                    <td data-band="warn">{amber}</td>
                    <td data-band="fail">{red}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <p className="lp-footnote">
              Criteria feasibility is agreed in one session, agreed with external input, or not
              agreed at all. If it is not agreed, the pilot stops and validates the criteria before
              it sends another email.
            </p>
          </div>
        </section>

        <section className="lp-row" id="limits" aria-labelledby="limits-title">
          <div className="lp-margin">
            <h2 className="lp-margin-title" id="limits-title">
              What it refuses
            </h2>
          </div>
          <div className="lp-prose">
            <p>
              The scope is small on purpose. Each line below is a decision that makes the rest of
              the tool easier to defend.
            </p>
            <ul className="lp-limits">
              {LIMITS.map((limit) => (
                <li key={limit}>{limit}</li>
              ))}
            </ul>
          </div>
        </section>

        <section className="lp-cta" aria-labelledby="cta-title">
          <h2 id="cta-title">Walk through the workspace</h2>
          <p>
            The interface you can open here is a preview with forty-eight synthetic candidates and a
            session that ends when the tab does. It shows the shape of the job: triage, disposition,
            second check, dispatch, governance, audit.
          </p>
          <div className="lp-cta-actions">
            <Link to="/triage" className="lp-button">
              Open the sample workspace
            </Link>
            <a className="lp-link" href="https://github.com/VasanthSaravanann/Ledger">
              Read the source
            </a>
          </div>
          <p className="lp-cta-note">
            It is a preview, not the ledger. Nothing here is signed, chained or persisted; the Rust
            command line tool is what records a real disposition.
          </p>
        </section>
      </main>

      <footer className="lp-footer">
        <nav aria-label="Workspace">
          <Link to="/triage">Triage</Link>
          <Link to="/import">Import</Link>
          <Link to="/dispatch">Dispatch</Link>
          <Link to="/governance">Governance</Link>
          <Link to="/audit">Audit</Link>
        </nav>
        <p>
          Ledger, a manual candidate-feedback pilot. The roster in this preview is generated; no
          candidate data is stored.
        </p>
      </footer>
    </div>
  );
}
