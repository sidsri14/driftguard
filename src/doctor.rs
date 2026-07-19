use std::{path::Path, process::Command};

use glob::{glob, Pattern};
use serde::Serialize;

use crate::config::{self, Config};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Serialize)]
pub struct DoctorCheck {
    pub status: CheckStatus,
    pub name: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub format_version: u8,
    pub checks: Vec<DoctorCheck>,
}

impl DoctorReport {
    pub fn has_failures(&self) -> bool {
        self.checks
            .iter()
            .any(|check| check.status == CheckStatus::Fail)
    }
}

pub fn inspect(root: &Path) -> DoctorReport {
    let mut checks = Vec::new();
    let config_path = root.join(config::CONFIG_FILE);

    if !config_path.is_file() {
        checks.push(check(
            CheckStatus::Fail,
            "config",
            "driftguard.toml was not found; run `driftguard init`.",
        ));
        check_git(root, &mut checks);
        return DoctorReport {
            format_version: 1,
            checks,
        };
    }

    let config = match config::load_config(root) {
        Ok(config) => {
            checks.push(check(
                CheckStatus::Pass,
                "config",
                "driftguard.toml parsed successfully.",
            ));
            config
        }
        Err(error) => {
            checks.push(check(CheckStatus::Fail, "config", error));
            check_git(root, &mut checks);
            return DoctorReport {
                format_version: 1,
                checks,
            };
        }
    };

    check_env_files(root, &config, &mut checks);
    check_source_globs(&config, &mut checks);
    check_prompt_contracts(root, &config, &mut checks);
    check_git(root, &mut checks);

    DoctorReport {
        format_version: 1,
        checks,
    }
}

pub fn print(report: &DoctorReport, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(report).expect("doctor report should serialize")
        );
        return;
    }

    println!("DriftGuard doctor");
    for check in &report.checks {
        let label = match check.status {
            CheckStatus::Pass => "PASS",
            CheckStatus::Warn => "WARN",
            CheckStatus::Fail => "FAIL",
        };
        println!("[{label}] {}: {}", check.name, check.message);
    }

    if report.has_failures() {
        println!("Verdict: configuration needs attention.");
    } else {
        println!("Verdict: ready.");
    }
}

fn check_env_files(root: &Path, config: &Config, checks: &mut Vec<DoctorCheck>) {
    if config.env_files.is_empty() {
        checks.push(check(
            CheckStatus::Fail,
            "env_files",
            "At least one environment manifest must be configured.",
        ));
        return;
    }

    for env_file in &config.env_files {
        let status = if root.join(env_file).is_file() {
            CheckStatus::Pass
        } else {
            CheckStatus::Fail
        };
        let message = if status == CheckStatus::Pass {
            format!("`{env_file}` exists.")
        } else {
            format!("`{env_file}` is missing.")
        };
        checks.push(check(status, "env_file", message));
    }
}

fn check_source_globs(config: &Config, checks: &mut Vec<DoctorCheck>) {
    if config.source_globs.is_empty() {
        checks.push(check(
            CheckStatus::Fail,
            "source_globs",
            "At least one source glob must be configured.",
        ));
        return;
    }

    for source_glob in &config.source_globs {
        let result = Pattern::new(source_glob);
        let (status, message) = match result {
            Ok(_) => (CheckStatus::Pass, format!("`{source_glob}` is valid.")),
            Err(error) => (
                CheckStatus::Fail,
                format!("`{source_glob}` is invalid: {error}"),
            ),
        };
        checks.push(check(status, "source_glob", message));
    }
}

fn check_prompt_contracts(root: &Path, config: &Config, checks: &mut Vec<DoctorCheck>) {
    if config.prompts.is_empty() {
        checks.push(check(
            CheckStatus::Pass,
            "prompts",
            "No prompt contracts are configured; environment checks remain active.",
        ));
        return;
    }

    for (name, contract) in &config.prompts {
        let invalid_prompt_globs = contract
            .files
            .iter()
            .filter_map(|pattern| Pattern::new(pattern).err().map(|error| (pattern, error)))
            .collect::<Vec<_>>();
        if !invalid_prompt_globs.is_empty() {
            for (pattern, error) in invalid_prompt_globs {
                checks.push(check(
                    CheckStatus::Fail,
                    format!("prompt.{name}.files"),
                    format!("`{pattern}` is invalid: {error}"),
                ));
            }
            continue;
        }

        let prompt_count = contract
            .files
            .iter()
            .map(|pattern| glob_count(root, pattern))
            .sum::<usize>();
        if prompt_count == 0 {
            checks.push(check(
                CheckStatus::Warn,
                format!("prompt.{name}"),
                "No configured prompt files currently exist; this contract is inactive.",
            ));
            continue;
        }

        checks.push(check(
            CheckStatus::Pass,
            format!("prompt.{name}.files"),
            format!("Matched {prompt_count} prompt file(s)."),
        ));

        let schema_status = if root.join(&contract.schema).is_file() {
            CheckStatus::Pass
        } else {
            CheckStatus::Fail
        };
        checks.push(check(
            schema_status,
            format!("prompt.{name}.schema"),
            if schema_status == CheckStatus::Pass {
                format!("`{}` exists.", contract.schema)
            } else {
                format!("`{}` is missing.", contract.schema)
            },
        ));

        match Pattern::new(&contract.golden) {
            Ok(_) => {}
            Err(error) => {
                checks.push(check(
                    CheckStatus::Fail,
                    format!("prompt.{name}.golden"),
                    format!("`{}` is invalid: {error}", contract.golden),
                ));
                continue;
            }
        }
        let fixture_count = glob_count(root, &contract.golden);
        checks.push(check(
            if fixture_count > 0 {
                CheckStatus::Pass
            } else {
                CheckStatus::Fail
            },
            format!("prompt.{name}.golden"),
            if fixture_count > 0 {
                format!("Matched {fixture_count} golden fixture(s).")
            } else {
                format!("`{}` matched no golden fixtures.", contract.golden)
            },
        ));
    }
}

fn check_git(root: &Path, checks: &mut Vec<DoctorCheck>) {
    let output = Command::new("git")
        .current_dir(root)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output();
    let status = match output {
        Ok(output)
            if output.status.success()
                && String::from_utf8_lossy(&output.stdout).trim() == "true" =>
        {
            CheckStatus::Pass
        }
        _ => CheckStatus::Warn,
    };
    checks.push(check(
        status,
        "git",
        if status == CheckStatus::Pass {
            "Git repository detected."
        } else {
            "Git repository was not detected; `--since` checks will be unavailable."
        },
    ));
}

fn glob_count(root: &Path, pattern: &str) -> usize {
    let full_pattern = root.join(pattern).to_string_lossy().to_string();
    glob(&full_pattern)
        .map(|paths| {
            paths
                .filter_map(Result::ok)
                .filter(|path| path.is_file())
                .count()
        })
        .unwrap_or(0)
}

fn check(status: CheckStatus, name: impl Into<String>, message: impl Into<String>) -> DoctorCheck {
    DoctorCheck {
        status,
        name: name.into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn reports_missing_config() {
        let dir = tempdir().unwrap();
        let report = inspect(dir.path());

        assert!(report.has_failures());
        assert_eq!(report.checks[0].name, "config");
    }

    #[test]
    fn accepts_minimal_valid_config() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("driftguard.toml"),
            "env_files = [\".env.example\"]\nsource_globs = [\"**/*.ts\"]\n",
        )
        .unwrap();
        fs::write(dir.path().join(".env.example"), "DATABASE_URL=\n").unwrap();

        let report = inspect(dir.path());

        assert!(!report.has_failures());
    }
}
