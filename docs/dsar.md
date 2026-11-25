# Data-subject requests

Someone asks: "What do you hold about me?" or "Delete it." In a pilot of this shape,
the answer is an **export plus a deletion date**, and it is never an edited log.

`bash scripts/dsar-drill.sh` walks the path on the fixture so the answer is not a
thought experiment. This page is the answer in one sitting.

## Who answers

| Step | Owner |
| --- | --- |
| Confirm the requester is the candidate | privacy owner, with Legal — the tool has no identity check of its own |
| Produce the export and the retention preview | coordinator, using the three commands below |
| Decide what is disclosed and when it is erased | privacy owner + Legal |
| Send the answer | Legal's wording, signed off as the access review requires |

Do not answer from memory and do not disclose another candidate's row: filter by the
handle and the application id first.

## The three commands

```bash
ledger find --hash '#<handle>' --data <workspace>                     # resolve the redacted id
ledger log export --requisition <REQ> --out /secure/dsar-<date>.csv --data <workspace>
ledger purge --dry-run --operator <approver> --data <workspace>       # what retention would tombstone, and when
```

`find --hash` maps the `#a1b2c3d4` handle printed in errors and operator logs back to
the record. `log export` writes the exact text sent, with `REPLY_RECEIVED_FLAG`. The
dry run names the cutoff: `now − retention_days` from `policy.json` (2557 days, 7.0
years, by default).

`scripts/dsar-drill.sh` runs all three against `APP-1001` in the fixture and prints the
split below. On the fixture today: 1 matching disposition, 1 of 147 send rows for that
application, 0 disposition records purged because the cutoff is 2019 and the fixture was
captured in 2026 — the retention clock has not run.

## What is erasable, and what is not

| Where | Holds | Erasure |
| --- | --- | --- |
| `send_log.jsonl` | name, email, subject, exact body — inside the hash chain | **only by deleting the whole workspace** at end of retention; a chained line is never edited |
| `dispositions.jsonl` | name, email, decision, signature | `purge --yes` after the cutoff: PII replaced with a stable tombstone, the record and its signature kept |
| `roster-<REQ>.json` | imported name, email, knockout answers | `purge --yes` after the cutoff: rows tombstoned |
| `qa.jsonl`, `replies.jsonl`, `signals.jsonl` | application id, QA name, timestamps, scenario, signal | application ids only — no name, no email, no message text; not tombstoned |
| `operator.jsonl` | command, operator, time; argv has emails redacted to `#hash` | not personal data beyond the operator's own name |
| `anchors/` | entry count and chain head | no personal data |
| the source CSV you imported | whatever the ATS exported | **yours to delete**; the tool keeps the parsed roster, not the file |

The asymmetry is the design, not an oversight: the chain exists to prove the exact text
sent, so its bytes cannot be rewritten. `FORMAT_SPEC.md` §10 and `THREAT_MODEL.md` say so
in the same words. A request that requires the send log to be erased is answered by
deleting the workspace — and therefore the evidence — at the end of the retention
window, with the anchors kept off-host first.

Backups are part of the answer. A backup taken before a purge still contains the purged
data, and a restore brings it back. Retention and backup age are decided together
(`docs/restore-drill.md`).

## What the drill proves

- The handle resolution, the export and the retention preview all run, in that order,
  and print what each found.
- The split between "tombstoned in place" and "only with the workspace" is visible, not
  asserted: the numbers come from the fixture and the policy on disk.
- The export contains the candidate's rows and not the rest of the roster (the drill
  counts both).

## What it does not prove

- That the requester is the candidate. That is Legal's check.
- That a deleted backup is gone from every copy, replica or tape. The tool has no
  control over storage.
- That the policy's retention window is lawful for your jurisdiction. `policy.json` is
  a recorded decision, not legal advice.
- That erasing one person is possible while keeping the chain. It is not, and the drill
  says so on its last line.
