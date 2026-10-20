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
fn doctor_reports_compilation_target_and_local_clock_is_portable() {
    let doctor = cli(&["doctor", "--json"]);
    assert!(
        doctor.status.success(),
        "{}",
        String::from_utf8_lossy(&doctor.stderr)
    );
    let doctor: serde_json::Value = serde_json::from_slice(&doctor.stdout).unwrap();
    assert_eq!(doctor["target"], env!("DANDELION_BUILD_TARGET"));
    let clock = cli(&["platform", "time", "--json"]);
    assert!(
        clock.status.success(),
        "{}",
        String::from_utf8_lossy(&clock.stderr)
    );
    let clock: serde_json::Value = serde_json::from_slice(&clock.stdout).unwrap();
    assert_eq!(clock["system_clock_modified"], false);
    assert!(clock["unix_seconds"].as_u64().unwrap() > 0);
}

#[test]
#[cfg(target_vendor = "apple")]
fn linux_only_commands_fail_before_mutation_on_apple() {
    for command in ["inspect", "mounts", "watch"] {
        let result = cli(&["platform", command, "--json"]);
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
    }
    let result = cli(&["hotspot", "plan", "--interface", "wlan0"]);
    assert!(!result.status.success());
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("data");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("a"), b"abc").unwrap();
    let mut config = dandelion_config::Config::default();
    config.scanner.roots = vec![source];
    config.state.enabled = true;
    config.state.directory = dir.path().join("state");
    let file = dir.path().join("config.toml");
    std::fs::write(&file, config.to_toml().unwrap()).unwrap();
    let result = cli(&[
        "--config",
        file.to_str().unwrap(),
        "run",
        "--resume",
        "--json",
    ]);
    assert!(!result.status.success());
    assert!(
        !config.state.directory.exists(),
        "failed resume must not reserve state"
    );
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
    #[cfg(target_os = "linux")]
    {
        let out = cli(&["platform", "inspect", "--json"]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let observation: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert!(!observation["mounts"].as_array().unwrap().is_empty());
    }
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

#[test]
#[cfg(target_os = "linux")]
fn hotspot_plan_is_read_only_and_apply_requires_explicit_consent() {
    let plan = cli(&[
        "hotspot",
        "plan",
        "--interface",
        "wlan0",
        "--ssid",
        "Example",
    ]);
    assert!(
        plan.status.success(),
        "{}",
        String::from_utf8_lossy(&plan.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert_eq!(value["password_on_argv"], false);
    assert_eq!(value["regulatory_changes"], false);
    let d = tempfile::tempdir().unwrap();
    let journal = d.path().join("not-created");
    let refused = cli(&[
        "hotspot",
        "apply",
        "--interface",
        "wlan0",
        "--journal-directory",
        journal.to_str().unwrap(),
        "--password-file",
        "not-read",
    ]);
    assert_eq!(refused.status.code(), Some(2));
    assert!(!journal.exists());
}

#[test]
#[cfg(target_os = "linux")]
fn file_level_resume_skips_only_unchanged_acknowledged_files() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("data");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("a"), b"abc").unwrap();
    std::fs::write(source.join("b"), b"defg").unwrap();
    let mut config = dandelion_config::Config::default();
    config.scanner.roots = vec![source.clone()];
    config.state.enabled = true;
    config.state.directory = dir.path().join("state");
    let file = dir.path().join("config.toml");
    std::fs::write(&file, config.to_toml().unwrap()).unwrap();
    let args = [
        "--config",
        file.to_str().unwrap(),
        "run",
        "--resume",
        "--json",
    ];
    let run = || {
        let out = cli(&args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()
    };
    assert_eq!(run()["verified_payload_bytes"], 7);
    let skipped = run();
    assert_eq!(skipped["verified_payload_bytes"], 0);
    assert_eq!(skipped["files_completed"], 0);
    std::fs::write(source.join("b"), b"new content").unwrap();
    let changed = run();
    assert_eq!(changed["verified_payload_bytes"], 11);
    assert_eq!(changed["files_completed"], 1);
}

#[test]
fn native_country_and_link_compatibility_commands() {
    let d = tempfile::Builder::new()
        .prefix("dandelion-compat-test-")
        .tempdir()
        .unwrap();
    let locations = d.path().join("locations.csv");
    let blocks = d.path().join("blocks.csv");
    std::fs::write(&locations, "geoname_id,country_name\n1,Example\n").unwrap();
    std::fs::write(&blocks, "network,geoname_id\n10.0.0.0/8,1\n").unwrap();
    let out = cli(&[
        "compat",
        "country-lookup",
        "--locations",
        locations.to_str().unwrap(),
        "--blocks",
        blocks.to_str().unwrap(),
        "--address",
        "10.2.3.4",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let m: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(m["country"], "Example");
    assert_eq!(m["authorized"], false);
    let source = d.path().join("data.bin");
    std::fs::write(&source, b"original").unwrap();
    assert_eq!(
        cli(&[
            "compat",
            "link-exercise",
            "--file",
            source.to_str().unwrap()
        ])
        .status
        .code(),
        Some(2)
    );
    let out = cli(&[
        "compat",
        "link-exercise",
        "--file",
        source.to_str().unwrap(),
        "--cycles",
        "1",
        "--interval-ms",
        "10",
        "--confirm",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let j: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(j["phase"], "cleaned");
    assert_eq!(std::fs::read(&source).unwrap(), b"original");
    let out = cli(&[
        "compat",
        "link-recover",
        "--journal-directory",
        j["workspace"].as_str().unwrap(),
        "--confirm",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
