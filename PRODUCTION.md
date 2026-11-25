# Production readiness plan

Two different things can go live here, and they need different work:

- **A. The recorder** — the Rust CLI used for a real pilot with one employer, one
  requisition, and real rejected candidates. Nothing in it has been run against real
  people yet, and the pilot checklist is **0/13** signed.
- **B. The preview site** — `ledger-pilot-chi.vercel.app`. It is public, synthetic and
  session-only. It is "live" already; making it *production-grade* means deciding
  whether it stays a demo or becomes a real front end (which would need auth,
  persistence and the chain wired in — none of which exist).

Current state: the software path is complete for everything except **delivery
confirmation**; the human and legal acts are entirely undone; the preview is honest but
has no operational story (no error tracking, no monitoring, no auth by design).

---

## 1. Readiness at a glance

| Area | State | Blocks a real pilot? |
| --- | --- | --- |
| Dispositions, gate copy, signing, QA, chained log | implemented, 136 tests, golden fixtures | no |
| Retention, migration, anchoring, backup snapshots | implemented; **never drilled** | **yes** (no restore evidence) |
| Delivery confirmation (`queued` → `sent`) | **missing** — nothing records arrival | **yes** (the "confirmed sent" KPI cannot be non-zero) |
| Operator lifecycle (rotation, removal, access review) | registration only | **yes** |
| Monitoring / alerting on chain verification and purge | **none** | **yes** (a silent corruption is discovered late) |
| Legal: privacy opinion, retention sign-off, candidate notice | **not started** | **yes** |
| Mailbox: real monitored address, owner, 24 h SLA cover | **not started** | **yes** |
| Signatures: named HM and Legal, real requisition facts | **not started** | **yes** |
| Release: tagged build, checksums, changelog per tag | workflow exists, **no tag cut** | no (but expected by an auditor) |
| Preview site: auth, persistence, wiring to the CLI | **absent by design** | no, while it stays a demo |
| Design consistency (landing ↔ workspace) | unified | no |
| Accessibility of the preview | good, but no skip link; 30 design-gate findings | no |

## 2. P0 — before a single real candidate is emailed

These are cheap to do and embarrassing to skip.

| ID | Item | Acceptance | Effort |
| --- | --- | --- | --- |
| P0-1 | **Delivery confirmation** | A chained `DELIVERY_CONFIRMED` event per send, written by a named operator; `delivery_status` moves `queued → sent` only through it; `metrics` can report a non-zero confirmed-sent share; a golden fixture covers both statuses | M |
| P0-2 | **Backup and restore drill** | `backups/<utc>/` snapshot restored into an empty directory, `log verify` and `verify-signatures` pass on the restored copy, and the procedure is written down with the timings measured | S |
| P0-3 | **Key custody and rotation** | Documented: who holds the HM and Legal keys, where they live, how a key is rotated without invalidating old signatures, and what a lost key means for `verify-signatures` | S |
| P0-4 | **Access review** | The operator registry lists exactly the people who should have it, each with a role; a written review date and an owner for repeating it | S |
| P0-5 | **Chain monitoring** | `ledger log verify` (or `verify-signatures`) on a schedule with an alert on failure, and the anchor from `ledger anchor` kept off-host on a second schedule | M |
| P0-6 | **Purge rehearsal** | `purge --dry-run` on a copy of the real workspace, reviewed by Legal, showing exactly what tombstones and what stays; the outcome recorded in `policy.json` | S |
| P0-7 | **Human acts recorded** | Privacy opinion (task 1.1), monitored mailbox with a named owner and SLA cover (1.3), requisition facts for the real role and its gate selection (2.2), HM/Legal signatures (1.2, 2.3, 2.4, 2.5) all signed with real names | M |

## 3. P1 — before the pilot sends at volume

| ID | Item | Acceptance | Effort |
| --- | --- | --- | --- |
| P1-1 | **`--json` on every read command** | 25/25 commands emit JSON (currently 11); `status`, `governance`, `gate`, `requisition`, `templates`, `playbook`, `ats`, `disposition`, `qa`, `reply`, `signal`, `checklist`, `operator`, `find` included; one schema test per command | M |
| P1-2 | **Incident runbook** | One page: torn tail, failed verification, lost key, wrong disposition, hostile reply, data-subject request. Each with the first three commands to run and who is called | S |
| P1-3 | **On-call for the 24 h SLA** | The escalation owner named in `policy.json` is reachable, backed up, and the playbook's escalation script has been tested with a real reply | S |
| P1-4 | **Release tag** | `v0.1.0` tagged; `release.yml` produces tarballs + SHA-256 for macOS arm64/x86_64 and Linux; the verifier, `FORMAT_SPEC.md` and `THREAT_MODEL.md` ship inside | S |
| P1-5 | **Data-subject path** | A written answer to "candidate asks what you hold": what `find --hash`, `purge` and the send log can and cannot do (send-log PII is inside the chain) | S |
| P1-6 | **Schema-version the state documents** | `schema_version` on every JSON state file, not just the send log; `migrate` handles each older version; a test per version | M |

## 4. P2 — during and after the pilot

| ID | Item | Acceptance | Effort |
| --- | --- | --- | --- |
| P2-1 | **Property tests** | The three properties (no send without disposition, chain stays linked, supersede never rewrites) as `proptest` suites, running in CI | M |
| P2-2 | **Restore and retention evidence pack** | One directory an auditor can read in ten minutes: anchor receipts, purge reports, access reviews, restore logs | S |
| P2-3 | **Accessibility pass on the preview** | Skip link first in tab order, focus never under the header, 44 px targets at 390, contrast ≥ 4.5:1 on every label; `consistency.mjs` `focus-ring`, `skip-link`, `tap-size` all pass | S |
| P2-4 | **Branch protection and signed commits** | `main` protected: required checks (`test`, `msrv`, `web`, `supply-chain`, `coverage`), no force-push, commit signing for maintainers | S |
| P2-5 | **Dependency automation** | Scheduled `cargo update` PR plus a `bun` update PR, both gated by the existing CI; `cargo-deny` unchanged | S |
| P2-6 | **Design-gate decision** | Either accept the 30 open tasteskill findings in writing (they are the hanging-margin design and the missing kit grid) or port the landing to the kit's `.wrap` structure. Decide once, record it, stop re-litigating | M |

## 5. P3 — deliberately deferred

| ID | Item | Why it can wait |
| --- | --- | --- |
| P3-1 | Preview authentication and persistence | It is a demo. Adding auth without persistence and a real operator model would be security theatre. |
| P3-2 | Wiring the preview to the CLI | Needs a decision about whether the product is a local desktop tool or a hosted service, which is a product question, not a task. |
| P3-3 | 1200×630 share card (`og:image`) | Link previews are nice; a cropped screenshot would be worse than none. |
| P3-4 | Typestate for approved requisitions | Runtime checks and tests already hold the invariant; the refactor buys nothing for an auditor. |
| P3-5 | Windows support | No binary is claimed and none is tested; adding it means a CI matrix and CSV/CRLF work. |
| P3-6 | WebAssembly demo | The core pulls `store`/`std::fs`; it would need a `ledger-core` split first. |

## 6. Sequence

| Week | Milestone | Exit criteria |
| --- | --- | --- |
| 1 | **Legal and mailbox unblocked** | P0-7 signed: opinion, mailbox owner + SLA cover, reasons recorded, gate selection agreed |
| 2 | **Restore and destroy rehearsed** | P0-2 and P0-6 done on a copy; timings and outputs filed |
| 3 | **Delivery confirmation shipped** | P0-1 merged, golden fixture updated, `metrics` shows confirmed sends |
| 4 | **Operational cover** | P0-3, P0-4, P0-5, P1-2, P1-3 done; alerting tested by breaking a copy on purpose |
| 5 | **Release and interoperability** | P1-4 tagged and downloadable, P1-1 and P1-6 merged, `--json` documented |
| 6 | **Go/no-go** | The gate below passes; the pilot either starts or records why not |

## 7. Go/no-go gate

Run this on the day, in this order, and record the output. **Do not send anything if any
line fails.**

```bash
# 1. The workspace is intact.
ledger --operator "<name>" log verify --data <pilot>
verify_log.py <pilot>/send_log.jsonl

# 2. Every disposition is signed by a registered operator.
ledger --operator "<name>" verify-signatures --data <pilot>

# 3. The copy that will be sent is the copy that was signed (no drift).
ledger --operator "<name>" governance show --data <pilot>   # artefact verdicts
ledger --operator "<name>" status --data <pilot>            # checklist 13/13

# 4. The roster is the real one, filtered.
ledger --operator "<name>" ats import --data <pilot> --requisition <REQ> --file <export.csv>

# 5. Dry run: what would send, and what is blocked and why.
ledger --operator "<name>" dispatch --preview --data <pilot> --requisition <REQ>
```

- [ ] 13/13 checklist tasks signed, each by a named person
- [ ] `log verify` and `verify_log.py` agree on the same head
- [ ] zero `Invalid` signatures; any `Stale` is re-approved
- [ ] every blocked row has a named reason a person accepts
- [ ] a restore from the newest backup verified on a second machine
- [ ] the escalation phone answered a test message inside the SLA

## 8. Risks

| Risk | Likelihood | Impact | Mitigation |
| --- | --- | --- | --- |
| A gate answer contradicts the ATS export for a real candidate | medium | block + manual review | the third breaker already blocks; name a reviewer for the day |
| An operator loses a key | medium | signatures unusable for new records | P0-3: rotation procedure and a second holder |
| Chain corruption goes unnoticed | low | evidence challenged | P0-5: scheduled verify + off-host anchors |
| Retention purge surprises Legal | medium | compliance finding | P0-6: dry-run on a copy, signed off before the pilot |
| Someone treats the preview as the product | medium | reputational | every screen says "preview"; README and this plan repeat it |
| Scope creep into a hosted product | high | pilot never runs | P3-1/P3-2 are out until the pilot produces evidence |

## 9. Definition of done

**Recorder:** P0-1…P0-7 and P1-1, P1-2, P1-4, P1-6 complete; `cargo test --locked`
green; a real requisition imported, dispersed and dispatched end to end in a rehearsal
with a person playing the candidate; the audit pack from P2-2 assembled.

**Preview:** stays a demo unless P3-1/P3-2 are approved. Done means it is honest (all
claims labelled), reachable, and carries no personal data: no analytics, no cookies, no
storage.

**Docs:** `README.md` runs as written on a clean machine (it does today); `CHANGELOG.md`
has an entry per shipped change; this file is updated when an item closes, with the
commit that closed it.
