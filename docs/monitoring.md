# Monitoring the chain

Corruption is discovered by a schedule, not by whoever happens to run a command. The
alarm is one script and one workflow:

| Thing | Where | When |
| --- | --- | --- |
| The check | `scripts/verify-chain.sh` | every run, and on the pilot host by cron or a systemd timer |
| The schedule | `.github/workflows/verify.yml` | daily at **06:17 UTC**, plus manual dispatch |
| The page | the workflow run's failure notification | as soon as the job fails |
| The person | `escalation_owner` in `policy.json` (`ledger governance show`) | acknowledges inside `escalation_sla_hours` (default **24 hours**) |

## What runs, and what each check means

`scripts/verify-chain.sh` runs four checks and requires them to agree:

| Check | Catches | Exit on failure |
| --- | --- | --- |
| `ledger log verify` | an edited value, a broken link, a wrong sequence, a backwards timestamp, an unknown schema, a torn tail | `3` |
| `verify_log.py` | the same rules, reimplemented in the Python standard library | `1` |
| `ledger verify-signatures` | a disposition or artefact whose signature does not cover its bytes | `3` |
| `ledger delivery list` | a delivery confirmation whose chain does not verify | `3` |

The script also compares the two heads. If `ledger log verify` and `verify_log.py` ever
print different heads, the run fails: one of the two is wrong, and neither is trusted
until a person says which.

On success it prints one line:

```text
OK verify-chain: 147 entries · head d347a639 · signatures 149 · independent verifier agrees
```

On failure it prints one `ALARM …` line per failed check, then:

```text
ALARM chain verification FAILED for <workspace>
```

The exit code is `1` for a failed check and `2` for a tooling problem (no `python3`, no
binary, no such directory). A monitor that treats `2` as "all clear" is broken.

## What the daily job actually verifies

The GitHub job verifies a **restored copy** of the deterministic fixture: it seeds the
147-entry fixture, copies the workspace to a fresh directory, tampers with nothing, and
verifies the copy. CI has no access to a pilot's disk, and a scheduled job should never
write to a live workspace anyway.

It also runs the restore drill and then requires `--fixture-tampered` to **fail**, so a
verifier that always prints `OK` cannot pass the job. Both of those are the alarm testing
itself, not a claim about a pilot's data.

For a real pilot the same script runs on the host that holds the workspace:

```bash
ledger log verify --data <workspace>            # verify once, by hand
bash scripts/verify-chain.sh --data <workspace> # what the timer should run
```

A daily timer on the pilot host, roughly:

```cron
17 6 * * * cd /srv/ledger && ./scripts/verify-chain.sh --data /srv/ledger-data \
  >> /var/log/ledger-verify.log 2>&1
```

…with the host's alarm agent watching that log for `^ALARM ` or a non-zero exit. This
repository cannot page anyone: routing the notification to the escalation owner is a
human setup step, recorded in the access review.

## Who is paged, and what they run first

**Paged:** the `escalation_owner` named by `ledger governance show` — on the fixture
that is `M. Okafor, Employee Relations`, and in a pilot it is the person who signed the
escalation policy. **Deputy:** the hiring manager who signed the requisition. The SLA
starts when the workflow run fails, not when someone notices it.

First fifteen minutes, in order:

1. **Copy, do not touch.** Take a byte-for-byte copy of the whole workspace, including
   `operators/` and `anchors/`, somewhere the pilot cannot write to. An append or a
   quarantine before this point destroys the evidence you are about to need.
2. **Re-run the check by hand** and keep its output:
   ```bash
   bash scripts/verify-chain.sh --data <workspace>
   ledger log verify --data <workspace>
   ./verify_log.py <workspace>/send_log.jsonl
   ledger verify-signatures --data <workspace>
   ledger delivery list --data <workspace>
   ```
3. **Compare the head with the newest receipt** in `anchors/` (`ledger anchor` writes
   one). A head that matches the newest receipt but fails locally means the local copy
   is damaged; a head that matches nothing means the workspace was replaced or rolled
   back.
4. **Decide: restore or stop.** Restore from the newest backup that verifies
   (`docs/restore-drill.md`), or stop sending. What must not happen is appending to a
   chain that failed verification.

## Reading the failure

| Message | What it means | First action |
| --- | --- | --- |
| `entry hash does not cover the stored fields` | a stored value was edited after it was hashed | preserve; restore from a backup that verifies; find out who had write access |
| `ends with a partial line` | a crash or a copy happened mid-append | preserve; then `ledger log quarantine --reason <note> --yes` on a copy, or restore |
| `INVALID: DSP-…` / `INVALID artefacts` | a signature does not cover the record | stop sending; re-capture or re-approve, never edit the record |
| `verifiers disagree on the head` | the Rust chain and the independent verifier see different histories | treat both as suspect; do not append |
| `unknown schema_version` | the workspace was written by a newer build | upgrade the binary; do not downgrade the data |

## What the alarm does not cover

- **The live workspace, unless the timer runs there.** The GitHub job proves the alarm
  and the restore path; pointing it at real data is a host-level setup step.
- **Anchoring.** The daily job cannot publish an off-host receipt: it has no workspace
  and no operator key. Run `ledger anchor` on the pilot host after a clean verification.
- **The delivery chain's head.** It is verified but not anchored; only the send log has
  off-host receipts today.
- **Availability.** A job that never runs is not a green job. GitHub disables scheduled
  workflows in repositories with no activity for 60 days; the same timer on the pilot
  host is the durable answer.
- **Retention.** A backup taken before a purge still contains the purged personal data;
  retention and backup age are decided together in `PRODUCTION.md`.
