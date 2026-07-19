use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use tempfile::tempdir;

fn driftguard() -> Command {
    Command::new(env!("CARGO_BIN_EXE_driftguard"))
}

#[test]
fn init_creates_default_config() {
    let dir = tempdir().unwrap();

    let output = driftguard()
        .current_dir(dir.path())
        .arg("init")
        .output()
        .unwrap();

    assert_success(&output);
    assert!(dir.path().join("driftguard.toml").is_file());
    let config = fs::read_to_string(dir.path().join("driftguard.toml")).unwrap();
    assert!(!config.contains("[prompts."));
}

#[test]
fn check_fails_for_missing_env_manifest_key() {
    let dir = tempdir().unwrap();
    assert_success(
        &driftguard()
            .current_dir(dir.path())
            .arg("init")
            .output()
            .unwrap(),
    );
    write_file(dir.path(), ".env.example", "DATABASE_URL=\n");
    write_file(
        dir.path(),
        "src/index.ts",
        "const token = process.env.DEEPSEEK_API_KEY;\n",
    );

    let output = driftguard()
        .current_dir(dir.path())
        .arg("check")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("DEEPSEEK_API_KEY"));
    assert!(stderr.contains("ENVIRONMENT DRIFT"));
}

#[test]
fn check_json_emits_machine_readable_failure() {
    let dir = tempdir().unwrap();
    assert_success(
        &driftguard()
            .current_dir(dir.path())
            .arg("init")
            .output()
            .unwrap(),
    );
    write_file(dir.path(), ".env.example", "DATABASE_URL=\n");
    write_file(
        dir.path(),
        "src/index.ts",
        "const token = process.env.MISSING_TOKEN;\n",
    );

    let output = driftguard()
        .current_dir(dir.path())
        .args(["check", "--json"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["format_version"], 1);
    assert_eq!(report["verdict"], "failed");
    assert_eq!(report["environment"][0]["key"], "MISSING_TOKEN");
}

#[test]
fn doctor_reports_valid_minimal_project() {
    let dir = tempdir().unwrap();
    run_git(dir.path(), &["init"]);
    write_file(
        dir.path(),
        "driftguard.toml",
        "env_files = [\".env.example\"]\nsource_globs = [\"**/*.ts\"]\n",
    );
    write_file(dir.path(), ".env.example", "DATABASE_URL=\n");

    let output = driftguard()
        .current_dir(dir.path())
        .args(["doctor", "--json"])
        .output()
        .unwrap();

    assert_success(&output);
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["format_version"], 1);
    assert!(report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .all(|check| check["status"] != "fail"));
}

#[test]
fn check_rejects_missing_configured_manifest() {
    let dir = tempdir().unwrap();
    write_file(
        dir.path(),
        "driftguard.toml",
        "env_files = [\".env.example\"]\nsource_globs = [\"**/*.ts\"]\n",
    );

    let output = driftguard()
        .current_dir(dir.path())
        .arg("check")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("configured env manifest"));
}

#[test]
fn install_hook_writes_pre_commit_hook() {
    let dir = tempdir().unwrap();
    run_git(dir.path(), &["init"]);
    assert_success(
        &driftguard()
            .current_dir(dir.path())
            .arg("install-hook")
            .output()
            .unwrap(),
    );

    let hook = fs::read_to_string(dir.path().join(".git/hooks/pre-commit")).unwrap();
    assert!(hook.contains("driftguard check"));
}

#[test]
fn changed_env_scope_ignores_unchanged_missing_env_usage() {
    let dir = tempdir().unwrap();
    run_git(dir.path(), &["init"]);
    run_git(dir.path(), &["config", "user.email", "test@example.com"]);
    run_git(dir.path(), &["config", "user.name", "DriftGuard Test"]);
    assert_success(
        &driftguard()
            .current_dir(dir.path())
            .arg("init")
            .output()
            .unwrap(),
    );
    write_file(dir.path(), ".env.example", "DATABASE_URL=\n");
    write_file(
        dir.path(),
        "src/old.ts",
        "const ignored = process.env.UNDECLARED_OLD_KEY;\n",
    );
    run_git(dir.path(), &["add", "."]);
    run_git(dir.path(), &["commit", "-m", "base"]);
    write_file(
        dir.path(),
        "src/new.ts",
        "const ok = process.env.DATABASE_URL;\n",
    );
    run_git(dir.path(), &["add", "src/new.ts"]);

    let output = driftguard()
        .current_dir(dir.path())
        .args(["check", "--since", "HEAD", "--env-scope", "changed"])
        .output()
        .unwrap();

    assert_success(&output);
}

fn write_file(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn run_git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert_success(&output);
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
