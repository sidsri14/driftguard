use std::path::Path;

use crate::{
    env_scan::{normalize_path, EnvFailure},
    prompt::{PromptFailure, PromptFailureKind},
};

pub fn print_success() {
    println!("OK: Deployment contracts passed.");
}

pub fn print_failures(root: &Path, env: &[EnvFailure], prompts: &[PromptFailure], markdown: bool) {
    if markdown {
        print_markdown(root, env, prompts);
    } else {
        print_terminal(root, env, prompts);
    }
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
                "| `{}` | `{}` | Add `{}=` to `{}` |",
                format!("{}:{}", normalize_path(root, &failure.file), failure.line),
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
    }
}
