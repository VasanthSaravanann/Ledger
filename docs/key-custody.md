# Key custody and rotation

Every decision Ledger records is attributed to an Ed25519 key, not to a `$USER`
string. This document says whose key it is, where it lives, how it changes hands, and
what a lost key costs. It is a procedure, not a promise: the file it describes is
`operators.json` plus one secret per operator.

```text
<workspace>/
├── operators.json          # names, roles, public keys, registered_at
└── operators/
    ├── j-chen.key          # 32-byte Ed25519 secret, lowercase hex, mode 0600
    └── …
```

- Directory mode `0700`, keyfile mode `0600`; both are set by `store::ensure_dir` and
  `store::write_secret`, not by the umask.
- The public half is in `operators.json`, so a signature can be checked without
  reading any secret.
- A signature also embeds its own public key (`Signed.public_key`), so a record
  verifies with no registry at all. That is what makes rotation safe (§ Rotation).
- There is no passphrase on the keyfile. Custody therefore means: full-disk
  encryption, an OS account that is not shared, and a workspace directory that is not
  world-readable. A copied keyfile is a working key.
- Never commit a workspace. `/ledger-data` is ignored for this reason; the fixture
  lives in a temporary directory and its keys are derived, not secret.

## Holders

This repository ships **no real keys and names no real people**. In a deployed pilot
the names are the rows of `operators.json`, and the people are the ones named in the
signed access review (`PRODUCTION.md` P0-4). The table is the shape that file must
have; the fixture column is the synthetic example (`ledger demo seed --record`).

| Key | Role it may act as | Fixture example | Lives at | What it may sign | Rotation decided by |
| --- | --- | --- | --- | --- | --- |
| Hiring-manager key | `approver` | `j-chen` | `operators/j-chen.key` | gate copy, templates, playbook, requisition, comparative paragraph | hiring manager + coordinator |
| Legal key | `approver` | `m-okafor` | `operators/m-okafor.key` | the same artefacts, as the second required sign-off | Legal + coordinator |
| Coordinator key | `coordinator` | `rec-rui` | `operators/rec-rui.key` | dispositions, QA, dispatch, delivery confirmations, anchors | coordinator + hiring manager |
| Second-reviewer key | `coordinator` | `qa-patel` | `operators/qa-patel.key` | the second-person QA check | coordinator |
| Auditor key | `auditor` | `aud-bell` | `operators/aud-bell.key` | nothing: read-only verification | auditor + Legal |

Rules that follow from the roles (see `Role::permits` in `src/identity.rs`):

- **Two approver keys, two different people.** A hiring manager and Legal both sign
  the copy; one person holding both keys makes the two signatures one signature.
- **The coordinator key does not approve copy, and the approver key does not
  dispatch.** The access review exists to check this is true in practice.
- **The auditor key signs nothing.** It can run `verify-signatures`, `log verify`,
  `metrics` and every other read-only command, and nothing else.
- Only the operator's name is used on the command line; the key is read from the
  file. Losing the file does not change who the name belonged to.

## Rotation

Keys are rotated by **registering a new identity**, never by replacing the key behind
an existing name:

```bash
ledger operator register --name "S. Iyer" --role approver --data <workspace>
ledger operator list --data <workspace>          # both names, both fingerprints
ledger verify-signatures --data <workspace>      # old records still `valid`
```

Why a new name and not a new key for the old one:

1. `operators.json` refuses a duplicate name (`"…" is already registered`), and there
   is no in-place re-key command. That is deliberate: a signature names its key, so a
   name that silently changed keys would make "who signed this?" unanswerable.
2. Each `Signed` record embeds the public key it was made with, and both
   `NewDisposition::signature_valid()` and `Approval::artefact_status()` verify
   against **that** key. Old signatures therefore keep verifying after the registry
   has moved on: rotation does not invalidate the past.
3. An artefact is only `Stale` when its **copy changed** after the sign-off, and
   `Unsigned` when nobody signed. A rotated key is neither. `verify-signatures` after
   rotation prints the same `signed records` count and the same artefact verdicts as
   before, plus nothing new — the registry is not consulted for verdicts.
4. An **unchanged** artefact cannot be re-signed under the new identity: the
   approval refuses a repeat of the same copy hash (`AlreadySignedOff`), so the first
   sign-off stays visible instead of being silently replaced. The new identity signs
   the next copy; the old signature stays attached to the copy it approved.

Order of operations for a real rotation:

1. Record the reason and the date in the access review, and open the incident/log
   entry that the audit trail will point at.
2. Register the new identity; confirm `operator list` shows the expected role.
3. Have the new holder sign the next artefact that needs them. Confirm
   `verify-signatures` reports it `valid`.
4. Take a backup (`scripts/restore-drill.sh` proves the procedure), then destroy the
   old keyfile once nothing pending needs it: `shred -u` on Linux, or an
   overwrite-then-delete on macOS. On SSD-backed storage that is best-effort, which
   is why custody, not deletion, is the control that matters.
5. Keep the old name in `operators.json`. Deleting it does not delete history and
   would only break the story of who signed what.

## Loss

A lost or destroyed keyfile costs exactly one thing: **the ability to sign new
records as that name.**

- **Past signatures stay valid.** They were made by the key, the public half is
  embedded in each record, and rotation does not touch them.
- **The name cannot be reused.** Register a new name and say so in the access review;
  "who signed" then reads as two people, which is the truth.
- **Nothing is unlocked.** A lost private key cannot decrypt anything: the workspace
  has no encrypted payloads, and read access is a filesystem permission.
- **A leaked key is worse than a lost one.** Whoever holds it can sign as that
  operator — bounded by the role, but real. There is no revocation list: the
  mitigation is to supersede with a new identity, treat anything signed by the old
  key after the incident time as suspect, and enumerate those records from
  `operator.jsonl` and the record timestamps.
- **The registry is a file.** Anyone with write access to the workspace can replace
  `operators.json`. Signatures prove *which key* decided something, not which human
  held it; only custody of the directory, the off-host anchors and the signed access
  review close that gap. `THREAT_MODEL.md` states the same limit.

If a key is lost and no backup of it exists, the pilot continues under the new
identity: no data is lost, no signature is invalidated, and the access review carries
the date and the reason. What cannot be recovered is the ability to make a *new*
signature that appears to come from the old name — which is the correct outcome.
