//! The JSON contract (P1-1).
//!
//! `--json` is a global flag: every command accepts it, and every command that accepts
//! it prints exactly one object on stdout and nothing on stderr. The first test
//! enumerates the real clap tree through `--help`, so a new command that forgets the
//! flag fails here; the second runs a fixture and parses what comes out.

use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_ledger");

fn run(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("run ledger {args:?}: {error}"))
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Command names from a `Commands:` section, in help order, without `help`.
fn commands_from_help(help: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut inside = false;
    for line in help.lines() {
        if line.starts_with("Commands:") {
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        // A command line begins with exactly two spaces; wrapped descriptions are
        // indented to the description column and would fail this test.
        let is_command = line.starts_with("  ") && !line.starts_with("   ");
        if !is_command || line.trim().is_empty() {
            break;
        }
        let name = line.split_whitespace().next().unwrap_or_default();
        if !name.is_empty() && name != "help" {
            names.push(name.to_owned());
        }
    }
    names
}

fn top_level_commands() -> Vec<String> {
    let help = run(&["--help"]);
    assert!(help.status.success(), "ledger --help failed");
    let names = commands_from_help(&stdout(&help));
    assert!(
        names.len() >= 25,
        "parsed only {} commands from --help: {names:?}",
        names.len()
    );
    names
}

#[test]
fn json_contract_every_command_and_subcommand_accepts_json() {
    for command in top_level_commands() {
        let help = run(&[command.as_str(), "--help"]);
        assert!(help.status.success(), "`ledger {command} --help` failed");
        assert!(
            stdout(&help).contains("--json"),
            "`ledger {command}` does not accept --json"
        );
        for sub in commands_from_help(&stdout(&help)) {
            let nested = run(&[command.as_str(), sub.as_str(), "--help"]);
            assert!(
                nested.status.success(),
                "`ledger {command} {sub} --help` failed"
            );
            assert!(
                stdout(&nested).contains("--json"),
                "`ledger {command} {sub}` does not accept --json"
            );
        }
    }
}

fn fixture(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ledger-cli-json-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let seed = run(&[
        "demo",
        "seed",
        "--record",
        "--data",
        &dir.display().to_string(),
    ]);
    assert!(seed.status.success(), "demo seed failed");
    dir
}

/// Runs one `--json` command against the fixture and checks the contract.
fn assert_one_object(data: &str, args: &[&str]) -> serde_json::Map<String, serde_json::Value> {
    let mut full: Vec<&str> = args.to_vec();
    full.extend(["--operator", "rec-rui", "--json", "--data", data]);
    let output = run(&full);
    let out = stdout(&output);
    let err = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "`ledger {}` exited {:?}\nstdout: {out}\nstderr: {err}",
        args.join(" "),
        output.status.code()
    );
    assert!(
        err.is_empty(),
        "`ledger {}` wrote prose to stderr: {err}",
        args.join(" ")
    );
    let value: serde_json::Value = serde_json::from_str(&out).unwrap_or_else(|error| {
        panic!(
            "`ledger {}` did not print one JSON value ({error}):\n{out}",
            args.join(" ")
        )
    });
    let object = value
        .as_object()
        .unwrap_or_else(|| panic!("`ledger {}` printed no object", args.join(" ")));
    assert_eq!(
        object
            .get("command")
            .and_then(|value| value.as_str())
            .and_then(|value| value.split(' ').next()),
        Some(args[0]),
        "`ledger {}` stamped the wrong command",
        args.join(" ")
    );
    object.clone()
}

#[test]
fn json_contract_every_read_command_prints_one_object() {
    let dir = fixture("read");
    let data = dir.display().to_string();
    let handle = ledger::redact::id("APP-1001");

    let checks: Vec<Vec<&str>> = vec![
        vec!["status"],
        vec!["governance", "show"],
        vec!["gate", "list"],
        vec!["requisition", "list"],
        vec!["requisition", "show", "--id", "REQ-2026-014"],
        vec!["templates", "show"],
        vec!["playbook", "show"],
        vec!["playbook", "script", "--scenario", "individual-feedback"],
        vec!["disposition", "list", "--requisition", "REQ-2026-014"],
        vec!["qa", "list", "--requisition", "REQ-2026-014"],
        vec!["delivery", "list"],
        vec!["log", "verify"],
        vec!["log", "export", "--requisition", "REQ-2026-014"],
        vec!["metrics", "--requisition", "REQ-2026-014"],
        vec!["review", "route", "--requisition", "REQ-2026-014"],
        vec!["checklist", "show"],
        vec!["operator", "list"],
        vec!["operator", "whoami"],
        vec!["verify-signatures"],
        vec!["find", "--hash", &handle],
        vec!["dispatch", "--preview", "--requisition", "REQ-2026-014"],
    ];

    for args in checks {
        let object = assert_one_object(&data, &args);
        assert!(
            object.len() >= 2,
            "`ledger {}` printed only the command key",
            args.join(" ")
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn json_contract_writes_print_one_object() {
    let dir = fixture("write");
    let data = dir.display().to_string();

    // A write that appends to an unchained log.
    let signal = assert_one_object(
        &data,
        &[
            "signal",
            "record",
            "--requisition",
            "REQ-2026-014",
            "--value",
            "yes",
        ],
    );
    assert_eq!(signal["recorded"], 1);

    // A write that appends to the delivery chain, after a real send exists.
    let confirm = assert_one_object(&data, &["delivery", "confirm", "--send-id", "SND-000001"]);
    assert_eq!(confirm["confirmation"]["send_id"], "SND-000001");
    assert_eq!(
        confirm["confirmation"]["type"],
        ledger::delivery::CONFIRMED,
        "the entry type is the contract"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
