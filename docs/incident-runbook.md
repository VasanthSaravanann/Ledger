# Incident runbook

For the first responder at 02:00. Every incident below has the same shape: **copy the
workspace before you touch it**, run three commands, name the decision owner, keep the
evidence. Nothing in this file asks you to edit a record or a log by hand.

Before anything else:

```bash
cp -a <workspace> /secure/incident-$(date -u +%Y%m%dT%H%M%SZ)   # the whole directory,
                                                              # including operators/ and anchors/
```

The marginal cost is a few megabytes; the cost of not having it is the incident becoming
unprovable. The copy is the evidence, not the workspace.

Who decides what:

| Role | Named by | Decides |
| --- | --- | --- |
| Escalation owner | `ledger governance show` (`policy.json`) | hostile replies, suspected compromise, anything with a 24-hour SLA |
| Hiring manager | `operators.json` / the access review | wrong decisions, re-capture, re-approval |
| Legal | same | any external statement, retention, a data-subject request |
| Coordinator | same | the mechanics: quarantine, restore, re-dispatch |
| Privacy owner | the signed access review | data-subject requests |

The operator log (`operator.jsonl`) records every mutating command automatically: its
argv, the operator, the pid and the time. It is the incident's spine — do not delete it,
and do not run mutating commands "to see what happens" before the copy exists.

## Torn tail

**Symptom:** a command exits `3` with `ends with a partial line (N byte(s) after the last
newline)`. A crash or a copy happened mid-append; the complete lines before it are
intact.

First three commands (on the copy, not the original):

```bash
ledger log verify --data /secure/incident-<stamp>        # confirm only the tail is affected
tail -c 200 /secure/incident-<stamp>/send_log.jsonl      # look at the fragment, do not edit it
ledger log quarantine --reason "torn tail: <what happened>" --yes --data /secure/incident-<stamp>
```

**Decision owner:** the coordinator, after telling the escalation owner. If the
fragment is part of a send that reached a candidate, Legal decides what is said about it.

**Evidence to keep:** the copy, the fragment as quarantined
(`send_log.jsonl.<timestamp>.quarantine`), the `quarantine` record in `operator.jsonl`,
and the head before and after (`ledger log verify` output both times). If the
quarantined fragment could have been a real send, keep the mail client's copy too.

**Do not:** append to the original until the copy verifies, and never "repair" a line by
editing it — a rewritten line is a forged one.

## Failed verification

**Symptom:** `ledger log verify`, `verify_log.py` or `scripts/verify-chain.sh` exits
non-zero with `entry hash does not cover the stored fields`, `prev_hash does not match`,
or two verifiers printing different heads.

First three commands:

```bash
bash scripts/verify-chain.sh --data /secure/incident-<stamp>
./verify_log.py /secure/incident-<stamp>/send_log.jsonl
ledger verify-signatures --data /secure/incident-<stamp>
```

Then compare the head with the newest receipt in `anchors/` — that is what tells you
whether the workspace was edited or replaced:

```bash
ls -1 anchors/ | sort | tail -1
```

**Decision owner:** the escalation owner names it an incident; Legal decides whether the
pilot continues. The coordinator restores from the newest backup that verifies
(`docs/restore-drill.md`), and only then does anyone append.

**Evidence to keep:** the copy, all three verifier outputs, the anchor receipt the head
is compared to, and the names of everyone with write access to the directory at the
time. If the anchor is what fails, keep the off-host receipt too — it is the only
witness to what was published.

**Do not:** trust either verifier alone when they disagree, restore over the evidence,
or re-run a dispatch on a workspace that failed verification.

## Lost key

**Symptom:** `operator list` shows the identity, but signing fails with `keyfile for
"…" does not match the registry`, the keyfile is missing, or the person who held it is
unavailable.

First three commands:

```bash
ledger operator list --data <workspace>            # names, roles, key fingerprints
ledger verify-signatures --data <workspace>        # what still verifies without it
ledger operator register --name "<new name>" --role <coordinator|approver|auditor> --data <workspace>
```

Past signatures stay valid: each `Signed` record embeds the public key it was made with,
so rotation does not invalidate them (see `docs/key-custody.md`). The name cannot be
reused; the access review records the date and the reason.

**Decision owner:** the registry owner for the identity, with Legal when an approver key
is involved. If the key may have been **leaked** rather than lost, treat it as a
compromise: the escalation owner decides, and anything signed with the old key after the
incident time is suspect.

**Evidence to keep:** `operator list` before and after, the access review entry, the
operator log, and — for a suspected leak — every record signed by that key after the
incident time, enumerated from the record timestamps.

## Wrong disposition

**Symptom:** the adverse-action reason recorded does not match the decision, was
captured against the wrong application, or the wrong gate answer was selected.

First three commands:

```bash
ledger disposition history --application <APP-ID> --data <workspace>
ledger disposition record --requisition <REQ> --application <APP-ID> … --supersedes <DSP-ID> --data <workspace>
ledger verify-signatures --data <workspace>
```

Records are locked: a correction is a **new** record that supersedes the old one, never
an edit. The send log keeps the text that was actually sent, so a wrong email that
already went out cannot be un-sent; the correction changes the next send and the history
shows both. GATE corrections need the second-person QA check again
(`ledger qa record`).

**Decision owner:** the named decision maker, with the coordinator; Legal if the wrong
reason was sent to a candidate.

**Evidence to keep:** both records (the superseded one and the correction), the QA check,
the signature on each, and the send-log entries that reference either record id. Note in
the operator log why the first record was wrong.

**Do not:** delete the old record, edit it, or re-dispatch to the same application
without confirming the correction supersedes it: the pipeline skips any application that
already has a send in the log, silently and by design, so a second dispatch will not
"fix" the first.

## Hostile reply

**Symptom:** a reply alleging discrimination, or otherwise hostile, arrives in the
monitored mailbox. The tool records **that** it arrived and under which playbook
scenario, not what it said.

First three commands:

```bash
ledger governance show --data <workspace>                         # who to escalate to, and the SLA
ledger playbook script --scenario discrimination-concern --data <workspace>
ledger reply record --requisition <REQ> --application <APP-ID> --scenario discrimination-concern --data <workspace> --operator <name>
```

The third command prints the escalation owner and the acknowledgement deadline
(`escalation_sla_hours`, 24 by default). Acknowledge inside it; the clock starts when
the reply arrived, not when it was recorded.

**Decision owner:** the escalation owner named by `ledger governance show`. Legal decides
any substantive response; the coordinator sends the playbook acknowledgement, and only
the playbook's wording.

**Evidence to keep:** the reply record (time, requisition, application, scenario), the
mailbox message in its original form, the escalation acknowledgement with its timestamp,
and the playbook script version that was used. The candidate's message is personal
information: keep it in the mailbox or a controlled case file, not in the workspace.

**Do not:** reply outside the playbook, discuss the candidate's allegation with the
decision maker before Legal is involved, or record a scenario that does not match what
was received.

## Data-subject request

**Symptom:** someone asks what is held about them, or asks for it to be erased. This is
a **data-subject** request; retention and erasure are Legal's call, and the answer
differs for chained and unchained state.

First three commands:

```bash
ledger find --hash '#<handle>' --data <workspace>                 # resolves the redacted id
ledger log export --requisition <REQ> --out /secure/dsar-<date>.csv --data <workspace>
ledger purge --dry-run --data <workspace>                         # what retention would tombstone, and when
```

The export is the evidence of what was sent; `purge --dry-run` shows what is erasable.
The rule is the one in `FORMAT_SPEC.md` and `docs/restore-drill.md`: dispositions, QA and
rosters can be tombstoned, but **send-log PII is inside the hash chain** — erasure means
deleting the whole workspace at end of retention, never editing a chained line. Backups
taken before a purge still contain the data; retention and backup age are decided
together.

**Decision owner:** the privacy owner, with Legal. The coordinator produces the export
and the dry-run; neither answers the requester directly.

**Evidence to keep:** the request and its date, the export, the dry-run report, Legal's
decision, and — if erasure was chosen — the date the workspace was destroyed and the
anchor receipts that show the chain existed up to that point. Destroying the workspace
destroys the evidence with it; keep the receipts off-host first.

**Do not:** erase a single chained line, delete a backup to "settle" a request, or answer
with an export that still contains another candidate's data — filter by the handle first.

## After any incident

Write one operator note with the incident id, the copy's path, the head at the time, and
the decision; run `ledger anchor` on the restored workspace once it verifies, and keep
the receipt off-host. If the alarm came from the daily job, the runbook's first command
is the same copy — the job verifies a restored copy, not the live directory
(`docs/monitoring.md`).
