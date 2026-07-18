use std::{path::Path, process::Command};

pub fn changed_files(root: &Path, since: &str) -> Result<Vec<String>, String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(["diff", "--name-only", since, "--"])
        .output()
        .map_err(|err| format!("failed to execute git diff: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git diff failed: {}", stderr.trim()));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim().replace('\\', "/"))
        .filter(|line| !line.is_empty())
        .collect())
}
