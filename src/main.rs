use std::{env, fs, process};

use clap::{Parser, Subcommand, ValueEnum};
use driftguard_cli::{config, doctor, env_scan, git, prompt, report};

#[derive(Debug, Parser)]
#[command(
    name = "driftguard",
    version,
    about = "Catch missing environment variables and broken AI output contracts before merge."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Generate a default driftguard.toml file.
    Init,
    /// Run deployment contract checks.
    Check {
        /// Only inspect prompt files changed since this git ref.
        #[arg(long)]
        since: Option<String>,

        /// Environment scan scope. Use `changed` with --since to scan only changed source files.
        #[arg(long, value_enum, default_value_t = EnvScope::All)]
        env_scope: EnvScope,

        /// Print a GitHub PR friendly Markdown report.
        #[arg(long, conflicts_with = "json")]
        markdown: bool,

        /// Print a stable machine-readable JSON report.
        #[arg(long, conflicts_with = "markdown")]
        json: bool,
    },
    /// Diagnose configuration, prompt mappings, manifests, and Git availability.
    Doctor {
        /// Print the diagnostic report as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Install DriftGuard as a local git pre-commit hook.
    InstallHook {
        /// Overwrite an existing pre-commit hook.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum EnvScope {
    All,
    Changed,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {err}");
        process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();
    let root = env::current_dir().map_err(|err| format!("failed to read cwd: {err}"))?;

    match cli.command {
        Commands::Init => {
            if config::init_config(&root)
                .map_err(|err| format!("failed to initialize config: {err}"))?
            {
                println!("Created driftguard.toml");
            } else {
                println!("driftguard.toml already exists");
            }
        }
        Commands::Check {
            since,
            env_scope,
            markdown,
            json,
        } => {
            let config = config::load_config(&root)?;
            config::validate_runtime(&root, &config)?;
            let changed_files = match since {
                Some(ref base) => git::changed_files(&root, base)?,
                None => Vec::new(),
            };
            if matches!(env_scope, EnvScope::Changed) && since.is_none() {
                return Err("`--env-scope changed` requires `--since`.".to_string());
            }

            let env_changed_files =
                matches!(env_scope, EnvScope::Changed).then_some(changed_files.as_slice());
            let env_failures = env_scan::check_env(&root, &config, env_changed_files);
            let prompt_changed_files = since.as_ref().map(|_| changed_files.as_slice());
            let prompt_failures = prompt::check_prompts(&root, &config, prompt_changed_files);
            let format = if json {
                report::ReportFormat::Json
            } else if markdown {
                report::ReportFormat::Markdown
            } else {
                report::ReportFormat::Terminal
            };

            if env_failures.is_empty() && prompt_failures.is_empty() {
                report::print_success(format);
            } else {
                report::print_failures(&root, &env_failures, &prompt_failures, format);
                process::exit(1);
            }
        }
        Commands::Doctor { json } => {
            let report = doctor::inspect(&root);
            doctor::print(&report, json);
            if report.has_failures() {
                process::exit(1);
            }
        }
        Commands::InstallHook { force } => install_hook(&root, force)?,
    }

    Ok(())
}

fn install_hook(root: &std::path::Path, force: bool) -> Result<(), String> {
    let git_dir = root.join(".git");
    if !git_dir.is_dir() {
        return Err("not a git repository: .git directory was not found".to_string());
    }

    let hooks_dir = git_dir.join("hooks");
    fs::create_dir_all(&hooks_dir)
        .map_err(|err| format!("failed to create git hooks dir: {err}"))?;
    let hook_path = hooks_dir.join("pre-commit");
    if hook_path.exists() && !force {
        return Err("pre-commit hook already exists. Re-run with `driftguard install-hook --force` to overwrite it.".to_string());
    }

    fs::write(&hook_path, "#!/bin/sh\nset -eu\n\ndriftguard check\n")
        .map_err(|err| format!("failed to write pre-commit hook: {err}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&hook_path)
            .map_err(|err| format!("failed to stat pre-commit hook: {err}"))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&hook_path, permissions)
            .map_err(|err| format!("failed to mark pre-commit hook executable: {err}"))?;
    }

    println!(
        "Installed DriftGuard pre-commit hook at {}",
        hook_path.display()
    );
    Ok(())
}
