# The audit evidence pack

An auditor should read one directory for ten minutes, not ask six questions.
`scripts/evidence-pack.sh` collects the answers and names each file with its SHA-256.

```bash
# Against a fresh fixture: proves the collector, changes nothing real.
bash scripts/evidence-pack.sh

# Against a pilot workspace: the pack that is handed to the auditor.
bash scripts/evidence-pack.sh --data /srv/ledger-data --operator "M. Okafor"
```

Output is `evidence/<date>/`, plus a copy of its index at `evidence/index.json` (the
newest pack, for the path in `PRODUCTION.md`). `evidence/` is git-ignored: the pack is
produced on demand and handed over, not committed to the repository under audit.

## What is in it

| File | What it proves | Produced by |
| --- | --- | --- |
| `chain-verify.txt` | the send log's entries and head verify locally | `ledger log verify` |
| `independent-verify.txt` | the same log verifies under the independent verifier, with the same head | `verify_log.py` |
| `signatures.txt` | dispositions and every signed artefact, with the four verdicts; the exit code is recorded even when red | `ledger verify-signatures` |
| `purge-preview.json` | the retention cutoff, what would be tombstoned, and how many send-log entries stay | `ledger purge --dry-run --json` |
| `restore-drill.txt` | a backup was restored into an empty directory and still verified, with timings | `scripts/restore-drill.sh` |
| `anchors/*.json` | the off-host chain-head receipts this workspace published | copied from `anchors/` |
| `access-review.md` | who could sign what, signed by the registry owner | copied from `docs/access-review.md` when present |
| `release/*.sha256` | the bytes of the released binaries, as published | `gh release download <tag> --pattern '*.sha256'` |
| `release/release.json` | tag, publication time and the full asset list | `gh release view <tag>` |
| `index.json` | every file above with its size and SHA-256, the head both verifiers agreed on, and what was missing | this script |

The index's `notes` array is part of the evidence: if the access review is unsigned or
no anchor exists, the pack says so instead of omitting the row. A missing file that is
recorded as missing is auditable; a missing file that is silently skipped is not.

## What it deliberately does not collect

- **Candidate data.** No names, addresses, message bodies or disposition text. The pack
  is about the chain and the process; the send log itself stays in the workspace.
- **The whole workspace.** Only the verifier outputs and the receipts are copied, so a
  pack can be handed over without handing over the personal data it describes.
- **A fresh anchor.** Publishing one needs the coordinator's key and writes to the
  workspace; run `ledger anchor` first, then build the pack.
- **A signature over the pack.** `index.json` carries digests, not a signature. Keep the
  pack's own hash in the ticket or the release notes if you need to prove it was not
  altered after the fact.

## Reading it in ten minutes

1. Open `index.json`. The head in `chain.head` must equal `chain.independent_head`, and
   `signatures_exit` must be `0`.
2. Compare `chain.head` with the newest file in `anchors/`. A head that matches an
   off-host receipt is a head that was published, not one that was computed after the
   fact.
3. Read `purge-preview.json`: the cutoff date should equal today minus the policy's
   retention window, and the send-log count should equal `chain.entries`.
4. Read `restore-drill.txt`: the head and fingerprint must match the pack's, and the
   tamper check must have failed as designed.
5. Read `access-review.md` if present. If it is not, the pack's notes say which task is
   still waiting for a person.

Anything else is a NO-GO, with the failing line quoted.

## What the pack cannot prove

- **That the workspace was not replaced before the pack was built.** Only the anchors,
  compared off-host, witness that.
- **That the operators are the people they claim to be.** Signatures prove keys; the
  access review and key custody prove people.
- **That the release was not rebuilt from different source.** The checksums prove the
  bytes the release published; the tag and the commit history prove the source.
- **That nothing was deleted.** A pack is a snapshot; a chain that was deleted entirely
  leaves no pack to read. The anchors are the defence.
