//! The scheduled alarm's failure path, exercised by the test suite (P0-5).
//!
//! `scripts/verify-chain.sh` is what the daily timer runs. A monitor whose failure
//! path has never executed is decoration, so this test requires it to fail on each of
//! the three ways a workspace breaks — an edited value, a torn tail, a broken
//! signature — and to stay quiet on a clean copy.

use std::process::{Command, Output};

fn verify(mode: &[&str]) -> Output {
    Command::new("bash")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/verify-chain.sh"
        ))
        .args(mode)
        .env("LEDGER_BIN", env!("CARGO_BIN_EXE_ledger"))
        .output()
        .expect("run scripts/verify-chain.sh")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn the_alarm_is_quiet_on_a_clean_copy() {
    let clean = verify(&["--fixture"]);
    assert!(
        clean.status.success(),
        "a clean fixture failed:\n{}",
        stderr(&clean)
    );
    assert!(
        String::from_utf8_lossy(&clean.stdout).contains("OK verify-chain"),
        "no OK line:\n{}",
        String::from_utf8_lossy(&clean.stdout)
    );
}

#[test]
fn the_alarm_fires_on_every_tamper() {
    for kind in ["value", "tear", "signature"] {
        let failing = verify(&["--fixture", "--tamper", kind]);
        assert!(
            !failing.status.success(),
            "{kind}: a tampered workspace verified, so the alarm is not live"
        );
        assert!(
            stderr(&failing).contains("ALARM chain verification FAILED"),
            "{kind}: the run failed without the alarm line:\n{}",
            stderr(&failing)
        );
    }
}
