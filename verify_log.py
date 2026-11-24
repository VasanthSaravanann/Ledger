#!/usr/bin/env python3
"""Independently verify a Ledger send log.

Deliberately shares no code with the Rust implementation: standard library only,
so a reviewer can read the whole verifier in one sitting and compare it with
FORMAT_SPEC.md. It re-derives every digest from the parsed fields.

    ./verify_log.py pilot/send_log.jsonl

Exit codes: 0 verified, 1 chain or digest failure, 2 usage/parse error.
"""

import hashlib
import json
import sys
import unicodedata
from datetime import datetime, timezone

GENESIS = "0" * 64
DIGEST_VERSION = 2
SCHEMA_VERSION = 1

# FORMAT_SPEC.md §4, exactly. Anything not listed here is outside digest coverage.
COVERED = [
    "digest_version", "seq", "prev_hash", "send_id", "application_id",
    "candidate_first_name", "candidate_email", "requisition", "recruiter",
    "decision_maker", "disposition_id", "disposition", "gate_code",
    "captured_at", "sent_at", "template", "text_version", "subject", "body",
    "text_digest", "delivery_status",
]


def nfc(value):
    return unicodedata.normalize("NFC", value)


def epoch_seconds(stamp):
    parsed = datetime.fromisoformat(stamp.replace("Z", "+00:00"))
    return int(parsed.astimezone(timezone.utc).timestamp())


def payload(entry):
    fields = {
        "digest_version": DIGEST_VERSION,
        "seq": entry["seq"],
        "prev_hash": entry["prev_hash"],
        "send_id": entry["send_id"],
        "application_id": entry["application_id"],
        "candidate_first_name": nfc(entry["candidate_first_name"]),
        "candidate_email": entry["candidate_email"],
        "requisition": entry["requisition"],
        "recruiter": entry["recruiter"],
        "decision_maker": entry["decision_maker"],
        "disposition_id": entry["disposition_id"],
        "disposition": entry["disposition"],
        "gate_code": entry.get("gate_code"),
        "captured_at": epoch_seconds(entry["captured_at"]),
        "sent_at": epoch_seconds(entry["sent_at"]),
        "template": entry["template"],
        "text_version": entry["text_version"],
        "subject": nfc(entry["subject"]),
        "body": nfc(entry["body"]),
        "text_digest": entry["text_digest"],
        "delivery_status": entry["delivery_status"],
    }
    assert sorted(fields) == sorted(COVERED), "payload drift from FORMAT_SPEC §4"
    return fields


def encode_string(value):
    # FORMAT_SPEC §3: minimal escaping; control characters use the long
    # \u00XX form, never the short \n form.
    out = ['"']
    for character in value:
        if character == '"':
            out.append('\\"')
        elif character == "\\":
            out.append("\\\\")
        elif ord(character) < 0x20:
            out.append("\\u%04x" % ord(character))
        else:
            out.append(character)
    out.append('"')
    return "".join(out)


def encode(value):
    if value is None:
        return "null"
    if value is True:
        return "true"
    if value is False:
        return "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, str):
        return encode_string(value)
    if isinstance(value, dict):
        return "{" + ",".join(
            encode_string(key) + ":" + encode(value[key]) for key in sorted(value)
        ) + "}"
    raise TypeError(f"canonical encoding does not support {type(value)!r}")


def digest(entry):
    return hashlib.sha256(encode(payload(entry)).encode("utf-8")).hexdigest()


def text_digest(entry):
    exact = entry["subject"] + "\n" + entry["body"]
    return hashlib.sha256(exact.encode("utf-8")).hexdigest()


def main(path):
    with open(path, "rb") as handle:
        raw = handle.read()
    if raw and not raw.endswith(b"\n"):
        print(f"FAIL {path}: torn write (file does not end with a newline)")
        return 1

    lines = [line for line in raw.decode("utf-8").split("\n") if line.strip()]
    if not lines:
        print(f"OK {path}: empty log")
        return 0

    previous_hash = GENESIS
    previous_sent = None
    for index, line in enumerate(lines):
        try:
            entry = json.loads(line)
        except json.JSONDecodeError as error:
            print(f"FAIL entry {index}: not JSON ({error})")
            return 1

        if entry.get("seq") != index:
            print(f"FAIL entry {index}: seq is {entry.get('seq')}")
            return 1
        if entry.get("schema_version", SCHEMA_VERSION) > SCHEMA_VERSION:
            print(f"FAIL entry {index}: unknown schema_version {entry['schema_version']}")
            return 1
        if entry.get("prev_hash") != previous_hash:
            print(f"FAIL entry {index}: prev_hash does not link to the previous entry")
            return 1
        if digest(entry) != entry.get("entry_hash"):
            print(f"FAIL entry {index}: entry_hash does not cover the stored fields")
            return 1
        if text_digest(entry) != entry.get("text_digest"):
            print(f"FAIL entry {index}: text_digest does not cover subject + body")
            return 1

        captured = epoch_seconds(entry["captured_at"])
        sent = epoch_seconds(entry["sent_at"])
        if captured > sent:
            print(f"FAIL entry {index}: captured_at is after sent_at")
            return 1
        if previous_sent is not None and sent < previous_sent:
            print(f"FAIL entry {index}: sent_at is earlier than the previous entry")
            return 1

        previous_hash = entry["entry_hash"]
        previous_sent = sent

    print(f"OK {path}: {len(lines)} entries verified")
    print(f"head: {previous_hash}")
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__.strip(), file=sys.stderr)
        sys.exit(2)
    try:
        sys.exit(main(sys.argv[1]))
    except (KeyError, ValueError) as error:
        print(f"FAIL {sys.argv[1]}: {error}")
        sys.exit(2)
