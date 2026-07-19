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
