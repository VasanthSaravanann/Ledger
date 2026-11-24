# Threat model

**Scope:** the `ledger` CLI, its workspace directory, and the artefacts it emits.
**Out of scope:** the employer's ATS, their mail provider, the recruiter's laptop
after the workspace is copied, and anything the tool does not run.

## What the tool defends against

| Threat | Control | Residual risk |
| --- | --- | --- |
| A rejection sent with no recorded reason | the gate-blocking protocol refuses to render without a locked disposition; the attempt is logged | an operator could dispatch outside the tool entirely; nothing here can prove they did not |
| A rejection whose reason contradicts the candidate's own answer | `KO_ANSWER_0n` vs form answer check; mismatch blocks and escalates, never auto-corrects | mismatch rate is only measurable if the export carries the knockout answers |
| Copy drift between what Legal approved and what was sent | `TEXT_VERSION` on every block and paragraph; the send log records the version that rendered; editing copy clears sign-off | a later version is not compared against the approved wording automatically |
| Post-hoc editing of the send log | SHA-256 chain, per `FORMAT_SPEC.md` §6 | see "what it cannot defend" below |
| Truncation of the send log | chain length check: a missing tail breaks the sequence at the next entry, or leaves a torn line, which fails closed | deleting the **last** entry's suffix and its predecessors' replacement is a re-chain, i.e. a rewrite |
| A crash mid-append corrupting the log | whole-line write, `fsync` file, `fsync` directory; a partial line is detected on load and refused | the operator must quarantine deliberately; the tool never "repairs" silently |
| Two operators writing at once | exclusive `flock` on the workspace, exit code `4` | a determined operator can run a second binary against a copy of the directory |
| A crafted CSV injecting headers, control characters or path traversal | per-cell validation at import (control characters, RFC-shaped email, no path components), 0600 permissions on state files | a value that is merely *unusual* but valid still reaches the email; the templates never echo free-text fields |
| Re-expansion of candidate-supplied `{{...}}` | the renderer is single-pass; substituted values are never rescanned | a template author could still write a literal `{{...}}` inside approved copy, which is caught at construction |
| HTML injection into the published artefact pages | all dynamic values escaped (`& < > " '`) at generation | the pages are rebuilt from the CLI, not fetched from the workspace |

## What it cannot defend against

State this plainly in any procurement conversation.

1. **Wholesale rewriting.** Anyone with write access to the workspace can delete
   `send_log.jsonl`, fabricate entries, recompute the chain, and produce a log that
   verifies. The chain proves *internal consistency*, not *provenance*. Closing
   this requires replicating the head hash off-host — emailed to the coordinator,
   committed to a separate repository, or written to an append-only service — and
   comparing it later. That is an operational control, not a feature of this
   binary.
2. **A hostile operator with root on the machine.** They can patch the binary, read
   the PII, or alter files without leaving evidence the binary will notice.
3. **Email content after it leaves the tool.** Delivery receipts, the candidate's
   inbox, and the mail provider's logs are outside this system.
4. **Truthfulness of the captured disposition.** The tool records what a named
   decision maker selected. It cannot detect a decision maker selecting `FIELD`
   because it is faster.
5. **Timestamps.** They are the operator machine's clock. There is no trusted time
   source and no monotonic clock guarantee beyond the ordering checks in
   `FORMAT_SPEC.md` §5.

## Trust boundaries

```
ATS export (untrusted text) ──► importer ──► roster
form capture (semi-trusted)  ──► validation ──► dispositions ──┐
approved copy (signed)       ──► renderer ──► send log (chained)
operator (trusted, logged)   ──► quarantine / migrate ──► operator.jsonl
```

- The ATS export is **untrusted input**: it is parsed defensively and fuzzed.
- The form is **semi-trusted**: values are validated, but the choice itself is a
  human act.
- Approved copy is **trusted but versioned**, so drift is visible.
- The operator is **trusted but logged**: quarantine and migration write an
  operator note, and neither is silent.

## Reporting

If you find a way to make the tool render an email for a blocked row, accept an
edited log, or accept a torn tail without quarantine, that is a bug worth
escalating before the pilot starts.
