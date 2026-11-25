# Ledger on-disk format and digest specification

**Version:** `digest_version = 2`, `schema_version = 1`
**Status:** implemented for the send log, the delivery-confirmation log and every state
 document.

This document is normative. `verify_log.py` and the Rust implementation are both
written against it, and the golden fixtures in `tests/golden.rs` pin it.

---

## 1. Scope

Two artefacts are covered here:

1. `send_log.jsonl` — the append-only, hash-chained log of dispatched emails. This
   is the evidentiary artefact.
2. `delivery.jsonl` — the append-only, hash-chained log of delivery confirmations
   (§11). It never edits a send entry; it records a later fact about one.
3. The JSON state documents in the workspace (`gate_library.json`,
   `templates.json`, `playbook.json`, `requisitions.json`, `policy.json`,
   `review.json`, `checklist.json`, `demo-summary.json`) and the append-only logs
   (`dispositions.jsonl`, `qa.jsonl`, `blocked.jsonl`, `replies.jsonl`,
   `signals.jsonl`, plus `operator.jsonl` when present).

Only the send log and the delivery log carry a hash chain. The other files are
append-only by construction (there is no update or delete path in the code) but
are not individually chained.

## 2. Text encoding

- All files are UTF-8, with **no BOM**.
- Every string is Unicode **NFC**-normalised before it is hashed.
- Strings are JSON-escaped minimally: `"` → `\"`, `\` → `\\`, and every code point
  below U+0020 → `\u00XX` (lowercase hex). `'` is **not** escaped, and non-ASCII
  characters are emitted as raw UTF-8.
- No other escaping is applied (`/`, `<`, `>`, `&` stay literal).

## 3. Canonical JSON

A canonical object is serialised with:

- keys sorted by **UTF-8 byte order** (equivalently, code-point order for valid
  UTF-8), ascending;
- **no whitespace** anywhere — no spaces, tabs, or newlines;
- `,` between members and `:` between a key and its value;
- no trailing commas;
- **integers only**: no floating-point numbers, no exponents, no leading `+`, no
  leading zeros; the literal is the shortest base-10 form, with `-` for negatives;
- `true`, `false`, `null` literals only;
- arrays preserve their given order (no arrays appear in the digest payload).

Example:

```
{"a":1,"b":"x","c":true,"d":null}
```

## 4. Digest payload

`entry_hash = lowercase_hex(SHA-256(canonical_bytes(payload)))` where `payload` is
the canonical object containing **exactly** this allowlist — no more, no fewer:

| Field | Type | Encoding |
| --- | --- | --- |
| `digest_version` | integer | always `2` |
| `seq` | integer | zero-based chain position |
| `prev_hash` | string | 64 lowercase hex chars, or the genesis constant |
| `send_id` | string | `SND-` + six digits |
| `application_id` | string | as captured |
| `candidate_first_name` | string | as captured, NFC |
| `candidate_email` | string | lowercased at capture |
| `requisition` | string | `REQ_ID` |
| `recruiter` | string | named individual |
| `decision_maker` | string | named individual |
| `disposition_id` | string | `DSP-` + four digits |
| `disposition` | string | `GATE` \| `FIELD` \| `ADMIN` |
| `gate_code` | string or null | `GATE-01`..`GATE-05`, null unless `GATE` |
| `captured_at` | integer | epoch **seconds**, UTC |
| `sent_at` | integer | epoch **seconds**, UTC |
| `template` | string | `gate` \| `field` \| `admin` |
| `text_version` | string | `v<major>.<minor>` |
| `subject` | string | exact merged subject |
| `body` | string | `RENDERED_BODY_FULL`, exact merged body |
| `text_digest` | string | SHA-256 of `subject + "\n" + body`, UTF-8 |
| `delivery_status` | string | `queued` \| `sent` \| `failed` |

The stored `disposition` and `gate_code` fields are written as exactly these codes
(`"GATE"`, `"GATE-03"`), so the stored field and the digest field are the same
string. A reader may accept the legacy kebab-case spellings (`"gate"`,
`"right-to-work"`) on input, but a writer always emits codes.

Excluded from the payload, by design:

- `entry_hash` — it is the output.
- `schema_version` — advisory metadata (§8). Excluding it lets a reader fail
  closed on an unknown schema without invalidating every earlier digest, which is
  what §4.4 of V4 requires.

Consequence, stated plainly: the digest covers **values, not layout**. Reordering
the JSON keys of a stored line, or re-indenting it, does **not** change the digest
and is **not** detected. Editing any covered value — including one byte of
`body` — is detected. This is the intended trade: canonicalisation is what makes
an independent verifier possible.

## 5. Timestamps

- Stored twice: as RFC 3339 with a `Z` offset for humans, and in the digest as
  **epoch seconds** (UTC, integer).
- On write, both `captured_at` and `sent_at` are truncated to whole seconds, so
  the digest covers the entire stored instant. Sub-second components are never
  written to the send log.
- Ordering rules enforced by `log verify`:
  - `captured_at <= sent_at` for every entry;
  - `sent_at` is **non-decreasing** along the chain;
  - `captured_at` has no cross-entry ordering constraint: a recruiter may capture
    decisions out of order.

## 6. Chain

- Genesis constant: 64 `0` characters.
  `0000000000000000000000000000000000000000000000000000000000000000`
- Entry `seq = 0` must have `prev_hash` equal to the genesis constant. There is no
  genesis *line*; genesis is the empty chain.
- Entry `seq = n > 0` must have `prev_hash` equal to entry `n-1`'s `entry_hash`.
- Sequence numbers must be `0, 1, 2, …` with no gaps.
- Verification recomputes every digest from the parsed fields. On any mismatch,
  `log verify` reports the failing `seq` and refuses to append.

## 7. Line framing

- One canonical-free JSON object per line, terminated by a single `\n`.
- Lines are appended, never rewritten: `write_all(line)` → `fsync(file)` →
  `fsync(parent directory)`.
- A trailing fragment that is not a complete, newline-terminated JSON object is a
  **torn write**. It is never silently dropped: the log refuses to load, and the
  only recovery is `log quarantine`, which moves the fragment to
  `send_log.jsonl.<timestamp>.quarantine` together with an operator note, and
  records the action in `operator.jsonl`.

## 8. Schema version

There are two version numbers, both `1` in this build:

- `send_log.jsonl` entries carry `"schema_version": 1` per line (and
  `delivery.jsonl` carries its entry type instead, §11);
- every **state document** carries a top-level `"schema_version": 1`.

### 8.1 State documents

A state document is any JSON file `Store::load`/`Store::save` read and write:
`gate_library.json`, `templates.json`, `playbook.json`, `requisitions.json`,
`policy.json`, `review.json`, `checklist.json`, `operators.json`,
`demo-summary.json`, and one `roster-<REQ>.json` per requisition. Each is one JSON
object whose other keys are the document; `schema_version` sits beside them.

- A reader **must reject** a document whose `schema_version` is greater than the
  version it knows, with the reason `schema_version`. The file is left untouched.
- A document with no `schema_version` is version **0**: the shape written before
  documents were versioned. Version 0 is read, and `ledger migrate` rewrites it as
  version 1.
- Four documents were bare JSON arrays at version 0. Their version-1 shape names the
  array:

  | File | Version 0 | Version 1 |
  | --- | --- | --- |
  | `operators.json` | `[…]` | `{"schema_version":1,"operators":[…]}` |
  | `requisitions.json` | `[…]` | `{"schema_version":1,"requisitions":[…]}` |
  | `checklist.json` | `[…]` | `{"schema_version":1,"completions":[…]}` |
  | `review.json` | `[…]` | `{"schema_version":1,"records":[…]}` |

- `schema_version` is **not** digest-covered. It is metadata for readers, exactly like
  the per-entry field below.
- A migration never rewrites `send_log.jsonl` or `delivery.jsonl`; it snapshots the
  workspace first and only rewrites state documents.
- Adding an optional field is a non-breaking change; a change to the digest allowlist
  (§4) is a `digest_version` bump and requires regenerating golden fixtures.

## 9. Independent verification

`verify_log.py` is a standard-library-only implementation of §2–§6. It shares no
code with the Rust crate and is run against a real fixture in `tests/independent.rs`,
so the two cannot drift apart silently.

```
./verify_log.py pilot/send_log.jsonl
# OK pilot/send_log.jsonl: 147 entries verified
```

## 10. Known gaps

Honest list, so the spec is not mistaken for more than it is:

1. The disposition, QA, blocked, reply and signal logs are append-only in code but
   not chained; each line is plain JSON with no digest.
2. `operator.jsonl` records every mutating command, but `log quarantine` is the only
   one whose note carries structured evidence of the repair.
3. `delivery.jsonl` is verified by this crate but not yet by `verify_log.py`; the
   independent verifier covers the send log alone.
4. The chain is a local file. Anyone with write access to the directory can replace
   the whole file with a re-chained forgery; only off-host replication closes that.
   See `THREAT_MODEL.md`.

## 11. Delivery confirmations (`delivery.jsonl`)

A send entry's `DELIVERY_STATUS` is fixed when it is written: the chain cannot be
edited. A later fact — the mail provider accepted the draft — is therefore a new
chained entry in `delivery.jsonl`, never an edit of `send_log.jsonl`.

- Entry type: `"type": "DELIVERY_CONFIRMED"`.
- The chaining rules of §6 and the framing rules of §7 apply unchanged, with these
  differences: the chain is `delivery.jsonl`; `confirmed_at` is non-decreasing along
  the chain; a `SEND_ID` may appear at most once; and a confirmation may only be
  appended for a `SEND_ID` that exists in `send_log.jsonl`, with
  `confirmed_at >= SEND_TIMESTAMP`.
- `entry_hash = lowercase_hex(SHA-256(canonical_bytes(payload)))`, where `payload`
  is the canonical object containing exactly:

| Field | Type | Encoding |
| --- | --- | --- |
| `digest_version` | integer | always `2` |
| `seq` | integer | zero-based chain position in `delivery.jsonl` |
| `prev_hash` | string | 64 lowercase hex chars, or the genesis constant |
| `type` | string | always `DELIVERY_CONFIRMED` |
| `send_id` | string | `SND-` + six digits, present in `send_log.jsonl` |
| `confirmed_by` | string | the operator's registered name |
| `confirmed_by_key` | string | that operator's Ed25519 public key, lowercase hex |
| `confirmed_at` | integer | epoch **seconds**, UTC |

- Excluded, as in §4: `entry_hash` is the output. This log has no
  `schema_version` field yet; an unknown `type` fails closed instead.
- Digest coverage is again **values, not layout**: reordering keys verifies, editing
  any covered value fails.
- **The effective status** of a send is `sent` when a confirmation exists for its
  `SEND_ID`, otherwise the `delivery_status` recorded in the send entry. `queued`
  therefore means "recorded, not yet confirmed", and only a confirmation can move a
  send out of it.
- A workspace that records no confirmations has an empty or absent
  `delivery.jsonl`, and its send log is byte-for-byte what it was before this log
  existed. `tests/golden.rs` pins the head and bytes of both states.
