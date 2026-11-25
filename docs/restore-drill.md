# Restore drill

A backup that has never been restored is a guess. `scripts/restore-drill.sh`
turns the guess into a command with an exit code.

```bash
bash scripts/restore-drill.sh
```

It seeds the deterministic fixture into a fresh temporary directory, backs the whole
workspace up, restores it into an empty directory, and asserts that nothing
observable changed. It prints the milliseconds each step took and exits 0 only when
every check below passes.

## What it asserts

| Check | How |
| --- | --- |
| The chain still verifies | `ledger log verify` on the restored copy, compared byte-for-byte with the seeded run |
| The head did not move | the restored `head:` line must equal the published fixture head `d347a639…` |
| Signatures still verify | `ledger verify-signatures` output identical, exit 0, `149` signed records |
| The bytes are the same | `demo::fingerprint` recomputed over the restored directory equals the value computed before the backup |
| An independent verifier agrees | `verify_log.py <restored>/send_log.jsonl` prints the same `head:` |
| The checks are live | one covered value in a copy of the restored send log is edited; `log verify` must exit non-zero |

The last row matters: without a negative control, a verifier that always prints `OK`
would pass the drill.

## What it measured

On the author's machine (Apple silicon, macOS, Rust 1.100 debug build, Python 3.13),
one run on 25 November 2026:

```text
seed            3483 ms
backup            42 ms
restore           43 ms
verify all       578 ms
tamper check      93 ms

OK restore drill: 147 entries · head d347a639 · fingerprint 38fce1b7 (this run) · signatures 149
```

The timings include process start-up for each verifier and the Python fingerprint
pass. The drill does not assert on them: they are printed so a slow restore is
visible, not so that a specific number becomes a promise.

## Why the fingerprint is computed, not read

`ledger demo seed --record` prints the summary fingerprint `64f9e1f9…`, which is
the value `demo::seed` computes over the files it wrote. The CLI then appends one
line to `operator.jsonl` for the command it just ran — carrying the real clock and
the process id — so the directory on disk has one more file than the summary saw,
and its fingerprint changes from run to run. The drill therefore computes the
fingerprint itself on both sides of the copy and compares those, prints it marked
`(this run)`, and separately asserts the published chain head. `tests/golden.rs`
keeps pinning the library-level fingerprint.

## Restoring a real pilot

The drill restores clean copies of a synthetic workspace. A real restore is the same
copy with three extra obligations:

1. **Stop the writers.** The workspace is guarded by `flock` (`.lock`), but a backup
   taken while a dispatch is running can capture a torn tail. Take backups between
   commands, or copy with the workspace idle.
2. **Copy the whole directory**, including `operators/` (the Ed25519 key files).
   Restoring the logs without the keys leaves `verify-signatures` unable to check
   anything; restoring keys without the logs loses the evidence.
3. **Verify against something off-host.** Compare the restored `head:` with the newest
   receipt in `anchors/` (see `ledger anchor`) and with a second machine. A restore
   that verifies only against itself proves the copy is self-consistent, not that it
   matches what was published.

Then run, in order, on the restored directory:

```bash
ledger log verify --data <restored>
./verify_log.py <restored>/send_log.jsonl
ledger verify-signatures --data <restored>
ledger metrics --requisition <REQ> --data <restored>
```

`verify_log.py` and `ledger log verify` must print the same head. Any other outcome is
a failed restore, not a warning.

## What the drill cannot prove

- That the backup is **off the host** or on a different failure domain. It copies to a
  temporary directory on the same disk.
- That the backup is **recent**. It seeds and backs up in one run.
- That a **restored workspace is the one Legal believes in**: only the anchor receipts
  and the signed access review do that.
- Anything about **retention**: a backup taken before a purge still holds the purged
  PII, which is why `PRODUCTION.md` treats retention and backups together.

Those remain human obligations; the drill removes only the part a machine can settle.
