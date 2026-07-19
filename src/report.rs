use std::path::Path;

use serde::Serialize;

use crate::{
    env_scan::{normalize_path, EnvFailure},
    prompt::{PromptFailure, PromptFailureKind},
};

#[derive(Debug, Clone, Copy)]
pub enum ReportFormat {
    Terminal,
    Markdown,
    Json,
}

#[derive(Serialize)]
struct JsonReport {
    format_version: u8,
    verdict: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    environment: Vec<JsonEnvFailure>,
    prompts: Vec<JsonPromptFailure>,
}

#[derive(Serialize)]
struct JsonEnvFailure {
    file: String,
    line: usize,
    key: String,
    env_files: String,
}

#[derive(Serialize)]
struct JsonPromptFailure {
    kind: &'static str,
    file: Option<String>,
    contract: Option<String>,
    schema: Option<String>,
    fixture: Option<String>,
    reason: String,
}

pub fn print_success(format: ReportFormat) {
    match format {
        ReportFormat::Terminal => println!("OK: Deployment contracts passed."),
        ReportFormat::Markdown => {
            println!("## DriftGuard Report\n\n**Verdict:** Passed");
        }
        ReportFormat::Json => print_json(Path::new("."), &[], &[], "passed", None),
    }
}

pub fn print_failures(
    root: &Path,
    env: &[EnvFailure],
    prompts: &[PromptFailure],
    format: ReportFormat,
) {
    match format {
        ReportFormat::Terminal => print_terminal(root, env, prompts),
        ReportFormat::Markdown => print_markdown(root, env, prompts),
        ReportFormat::Json => print_json(root, env, prompts, "failed", None),
    }
}

pub fn print_execution_error(error: &str) {
    print_json(Path::new("."), &[], &[], "error", Some(error));
}

fn print_terminal(root: &Path, env: &[EnvFailure], prompts: &[PromptFailure]) {
    eprintln!("Error: Deployment Contract Mismatch Detected!");
    eprintln!();

    if !env.is_empty() {
        eprintln!("[ENVIRONMENT DRIFT]");
        eprintln!("----------------------------------------------------------------------");
        for failure in env {
            eprintln!("Error: Missing environment variable initialization in manifest.");
            eprintln!(
                "File:  {}:{}",
                normalize_path(root, &failure.file),
                failure.line
            );
            eprintln!("Key:   {}", failure.key);
            eprintln!(
                "Fix:   Add '{}=' to your active env manifest: {}.",
                failure.key, failure.env_file
            );
            eprintln!();
        }
    }

    if !prompts.is_empty() {
        eprintln!("[PROMPT CONTRACT DRIFT]");
        eprintln!("----------------------------------------------------------------------");
        for failure in prompts {
            eprintln!("Error: {}", prompt_title(&failure.kind));
            if let Some(file) = &failure.file {
                eprintln!("File:  {}", normalize_path(root, file));
            }
            if let Some(contract) = &failure.contract {
                eprintln!("Contract: {contract}");
            }
            if let Some(schema) = &failure.schema {
                eprintln!("Schema: {}", normalize_path(root, schema));
            }
            if let Some(fixture) = &failure.fixture {
                eprintln!("Fixture Failed: {}", normalize_path(root, fixture));
            }
            eprintln!("Reason: {}", failure.reason);
            eprintln!();
        }
    }

    eprintln!("Execution Verdict: CI Build Halted (Exit Code 1)");
}

fn print_markdown(root: &Path, env: &[EnvFailure], prompts: &[PromptFailure]) {
    println!("## DriftGuard Report");
    println!();
    println!("**Verdict:** Failed");
    println!();

    if !env.is_empty() {
        println!("### Environment Drift");
        println!();
        println!("| File | Key | Fix |");
        println!("| --- | --- | --- |");
        for failure in env {
            println!(
                "| `{}:{}` | `{}` | Add `{}=` to `{}` |",
                normalize_path(root, &failure.file),
                failure.line,
                failure.key,
                failure.key,
                failure.env_file
            );
        }
        println!();
    }

    if !prompts.is_empty() {
        println!("### Prompt Contract Drift");
        println!();
        println!("| Type | File/Fixture | Schema | Reason |");
        println!("| --- | --- | --- | --- |");
        for failure in prompts {
            let target = failure
                .fixture
                .as_ref()
                .or(failure.file.as_ref())
                .map(|path| normalize_path(root, path))
                .unwrap_or_else(|| failure.contract.clone().unwrap_or_else(|| "-".to_string()));
            let schema = failure
                .schema
                .as_ref()
                .map(|path| normalize_path(root, path))
                .unwrap_or_else(|| "-".to_string());
            println!(
                "| {} | `{}` | `{}` | {} |",
                prompt_title(&failure.kind),
                target,
                schema,
                failure.reason.replace('|', "\\|")
            );
        }
    }
}

fn print_json(
    root: &Path,
    env: &[EnvFailure],
    prompts: &[PromptFailure],
    verdict: &'static str,
    error: Option<&str>,
) {
    let report = JsonReport {
        format_version: 1,
        verdict,
        error: error.map(String::from),
        environment: env
            .iter()
            .map(|failure| JsonEnvFailure {
                file: normalize_path(root, &failure.file),
                line: failure.line,
                key: failure.key.clone(),
                env_files: failure.env_file.clone(),
            })
            .collect(),
        prompts: prompts
            .iter()
            .map(|failure| JsonPromptFailure {
                kind: prompt_kind_code(&failure.kind),
                file: failure.file.as_ref().map(|path| normalize_path(root, path)),
                contract: failure.contract.clone(),
                schema: failure
                    .schema
                    .as_ref()
                    .map(|path| normalize_path(root, path)),
                fixture: failure
                    .fixture
                    .as_ref()
                    .map(|path| normalize_path(root, path)),
                reason: failure.reason.clone(),
            })
            .collect(),
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("DriftGuard report should serialize")
    );
}

fn prompt_kind_code(kind: &PromptFailureKind) -> &'static str {
    match kind {
        PromptFailureKind::UnmappedChangedPrompt => "unmapped_changed_prompt",
        PromptFailureKind::MissingSchema => "missing_schema",
        PromptFailureKind::MissingGoldenFixtures => "missing_golden_fixtures",
        PromptFailureKind::InvalidGoldenJson => "invalid_golden_json",
        PromptFailureKind::SchemaViolation => "schema_violation",
        PromptFailureKind::InvalidSchema => "invalid_schema",
        PromptFailureKind::MissingTemplateInput => "missing_template_input",
    }
}

fn prompt_title(kind: &PromptFailureKind) -> &'static str {
    match kind {
        PromptFailureKind::UnmappedChangedPrompt => {
            "Changed prompt file has no mapped contract in driftguard.toml."
        }
        PromptFailureKind::MissingSchema => "Prompt contract schema is missing.",
        PromptFailureKind::MissingGoldenFixtures => {
            "Prompt contract has no matching golden fixtures."
        }
        PromptFailureKind::InvalidGoldenJson => "Golden fixture is invalid JSON.",
        PromptFailureKind::SchemaViolation => {
            "Prompt payload fixture breaks structural JSON contract."
        }
        PromptFailureKind::InvalidSchema => "Prompt contract schema is invalid.",
        PromptFailureKind::MissingTemplateInput => {
            "Prompt fixture is missing required template input variables."
        }
    }
}
