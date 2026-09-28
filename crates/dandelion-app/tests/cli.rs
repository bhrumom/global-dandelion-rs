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
