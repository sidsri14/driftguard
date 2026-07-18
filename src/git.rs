use std::{path::Path, process::Command};

pub fn changed_files(root: &Path, since: &str) -> Result<Vec<String>, String> {
    if !git_ref_exists(root, since) {
        return Err(missing_ref_error(root, since));
    }

    let output = Command::new("git")
        .current_dir(root)
        .args(["diff", "--name-only", since, "--"])
        .output()
        .map_err(|err| format!("failed to execute git diff: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format_git_diff_error(root, since, stderr.trim()));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim().replace('\\', "/"))
        .filter(|line| !line.is_empty())
        .collect())
}

fn git_ref_exists(root: &Path, git_ref: &str) -> bool {
    Command::new("git")
        .current_dir(root)
        .args(["rev-parse", "--verify", "--quiet", git_ref])
        .status()
        .is_ok_and(|status| status.success())
}

fn is_shallow_repository(root: &Path) -> bool {
    let Ok(output) = Command::new("git")
        .current_dir(root)
        .args(["rev-parse", "--is-shallow-repository"])
        .output()
    else {
        return false;
    };

    output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "true"
}

fn missing_ref_error(root: &Path, since: &str) -> String {
    let shallow_hint = if is_shallow_repository(root) {
        "\nThis repository is shallow. In GitHub Actions, use `actions/checkout@v4` with `fetch-depth: 0`."
    } else {
        ""
    };

    format!(
        "git ref `{since}` is not available locally.{shallow_hint}\nFetch the base ref before running DriftGuard, for example: `git fetch origin {base}:refs/remotes/origin/{base}`.",
        base = since.strip_prefix("origin/").unwrap_or(since)
    )
}

fn format_git_diff_error(root: &Path, since: &str, stderr: &str) -> String {
    let mut message = format!("git diff failed for `{since}`: {stderr}");
    if is_shallow_repository(root)
        || stderr.contains("unknown revision")
        || stderr.contains("ambiguous argument")
    {
        message.push_str(
            "\nIf this is running in GitHub Actions, configure checkout with `fetch-depth: 0` so the base ref exists.",
        );
    }
    message
}

#[cfg(test)]
mod tests {
    use super::format_git_diff_error;
    use std::path::Path;

    #[test]
    fn adds_checkout_guidance_for_missing_base_ref_errors() {
        let error = format_git_diff_error(
            Path::new("."),
            "origin/main",
            "fatal: ambiguous argument 'origin/main': unknown revision",
        );

        assert!(error.contains("fetch-depth: 0"));
    }
}
