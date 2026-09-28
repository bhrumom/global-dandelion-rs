use std::process::{Command, Output};
fn cli(args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dandelion"));
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("DANDELION_") {
            command.env_remove(key);
        }
    }
    command.args(args).output().unwrap()
}
#[test]
fn help_version_and_completions() {
    for args in [&["--help"][..], &["--version"], &["completions", "bash"]] {
        let out = cli(args);
        assert!(out.status.success());
        assert!(!out.stdout.is_empty());
    }
}
#[test]
fn config_roundtrip_and_no_side_effect_dry_run() {
    let out = cli(&["config", "init"]);
    assert!(out.status.success());
    let config: dandelion_config::Config =
        toml::from_str(std::str::from_utf8(&out.stdout).unwrap()).unwrap();
    config.validate().unwrap();
    let out = cli(&["run", "--dry-run", "--json"]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["transport"]["mode"], "loopback");
}
#[test]
fn invalid_args_and_config_are_nonzero() {
    assert_eq!(
        cli(&["run", "--rate-bytes-per-second", "0"]).status.code(),
        Some(2)
    );
    assert_eq!(cli(&["run", "--unknown"]).status.code(), Some(2));
    assert_eq!(cli(&["run"]).status.code(), Some(2));
}
#[test]
fn selftest_uses_real_pipeline_and_clean_json_stdout() {
    let out = cli(&["self-test", "--json", "--log-format", "json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["verified_payload_bytes"], 65537);
    assert_eq!(json["files_completed"], 2);
    assert_eq!(json["status"], "completed");
}

#[test]
fn closed_stdout_pipe_is_a_normal_cli_exit() {
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_dandelion"))
        .args(["completions", "bash"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn new_platform_and_scan_commands_have_real_behavior() {
    let out = cli(&["platform", "inspect", "--json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let observation: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(!observation["mounts"].as_array().unwrap().is_empty());
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("data"), b"1234567").unwrap();
    let root = dir.path().to_str().unwrap();
    let meta = cli(&["scan", "metadata", "--source", root, "--json"]);
    assert!(
        meta.status.success(),
        "{}",
        String::from_utf8_lossy(&meta.stderr)
    );
    let meta: serde_json::Value = serde_json::from_slice(&meta.stdout).unwrap();
    assert_eq!(meta["regular_files"], 1);
    assert_eq!(meta["total_file_bytes"], 7);
    assert_eq!(meta["payload_bytes_read"], 0);
    let read = cli(&["scan", "read", "--source", root, "--json"]);
    assert!(
        read.status.success(),
        "{}",
        String::from_utf8_lossy(&read.stderr)
    );
    let read: serde_json::Value = serde_json::from_slice(&read.stdout).unwrap();
    assert_eq!(read["verified_payload_bytes"], 7);
    assert_eq!(read["accepted_wire_bytes"], 0);
    assert_eq!(read["attempted_wire_bytes"], 0);
    assert_eq!(std::fs::read(dir.path().join("data")).unwrap(), b"1234567");
}
#[test]
fn repeat_budget_and_persistence_across_processes() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir(&data).unwrap();
    std::fs::write(data.join("x"), b"12345").unwrap();
    let config = dir.path().join("settings.toml");
    let state = dir.path().join("state");
    let mut c = dandelion_config::Config::default();
    c.scanner.roots = vec![data];
    c.state.enabled = true;
    c.state.directory = state.clone();
    c.state.lifetime_limit_bytes = 170;
    std::fs::write(&config, c.to_toml().unwrap()).unwrap();
    let args = ["--config", config.to_str().unwrap(), "run", "--json"];
    for _ in 0..2 {
        let out = cli(&args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    assert_eq!(cli(&args).status.code(), Some(75));
    let out = cli(&["state", "inspect", "--directory", state.to_str().unwrap()]);
    let ledger: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(ledger["charged_bytes"], 170);
}
#[test]
fn repeated_runs_share_one_nonpersistent_budget() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("x"), b"12345").unwrap();
    let out = cli(&[
        "run",
        "--source",
        dir.path().to_str().unwrap(),
        "--repeat",
        "3",
        "--interval-ms",
        "100",
        "--traffic-limit-bytes",
        "170",
        "--json",
    ]);
    assert_eq!(out.status.code(), Some(75));
    let stats: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(stats["attempted_wire_bytes"], 170);
    assert_eq!(stats["files_completed"], 2);
}
#[test]
fn legacy_import_and_invalid_input_preservation() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("s");
    let legacy = dir.path().join("traffic_state.json");
    std::fs::write(
        &legacy,
        br#"{"current_month":"2026-09","total_sent_bytes":1234}"#,
    )
    .unwrap();
    let args = [
        "state",
        "import",
        "--directory",
        state.to_str().unwrap(),
        "--file",
        legacy.to_str().unwrap(),
    ];
    let first = cli(&args);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let second = cli(&args);
    assert!(second.status.success());
    let value: serde_json::Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_eq!(value["imported"], false);
    assert_eq!(value["ledger"]["charged_bytes"], 1234);
    assert!(legacy.exists());
}
