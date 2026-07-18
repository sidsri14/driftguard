mod config;
mod env_scan;
mod git;
mod prompt;
mod report;

use std::{env, process};

use clap::{Parser, Subcommand};

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

        /// Print a GitHub PR friendly Markdown report.
        #[arg(long)]
        markdown: bool,
    },
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
        Commands::Check { since, markdown } => {
            let config = config::load_config(&root)?;
            let changed_files = match since {
                Some(ref base) => git::changed_files(&root, base)?,
                None => Vec::new(),
            };

            let env_failures = env_scan::check_env(&root, &config);
            let prompt_failures = prompt::check_prompts(&root, &config, &changed_files);

            if env_failures.is_empty() && prompt_failures.is_empty() {
                report::print_success();
            } else {
                report::print_failures(&root, &env_failures, &prompt_failures, markdown);
                process::exit(1);
            }
        }
    }

    Ok(())
}
