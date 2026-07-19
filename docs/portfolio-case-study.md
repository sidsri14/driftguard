# DriftGuard engineering case study

## Problem

AI applications often rely on two contracts that ordinary unit tests do not
consistently enforce: environment keys expected at runtime and structured JSON
expected from prompt-driven workflows. DriftGuard checks both before merge.

## Constraints

- No customer source code, prompt text, or secret values leave the runner.
- No hosted model calls or cloud compute are required.
- Results must be deterministic and useful in local hooks and CI.
- False positives must be reviewable and suppressible.

## Design

```text
source files + git diff          prompt files + golden fixtures
           |                                |
           v                                v
  environment scanner              JSON Schema validator
           |                                |
           +---------------+----------------+
                           v
              terminal / Markdown / JSON
                           |
                    exit 0 / 1 / 2
```

The scanner uses bounded regex patterns for supported environment-access forms,
comment masking, configurable source globs, ignored directories, and explicit
suppressions. Prompt checks validate local fixture outputs against JSON Schema
and verify that prompt template variables exist in fixture inputs.

## Engineering evidence

- 19 library tests and 7 CLI integration tests.
- Strict Clippy with warnings denied.
- Linux, macOS, and Windows CI coverage.
- A versioned JSON report for machine integrations.
- `driftguard doctor` for deterministic setup diagnostics.
- A Criterion benchmark that exercises the production scanner.
- Read-only onboarding trials against six public AI repositories.

One local Windows benchmark scanned 500 generated TypeScript files in roughly
200-270 ms. This is a development measurement, not a universal latency claim;
filesystem and hardware dominate the result.

## What the pilot changed

The public-repository trials exposed two onboarding issues before launch:

1. A missing configured env manifest could previously pass when no supported env
   access was found. Runtime preflight now treats this as an execution error.
2. Placeholder prompt contracts generated warnings in projects without prompt
   files. New configs now start with environment checks and require prompt
   contracts to be added intentionally.

Large repositories also confirmed the need for source scoping and narrow
suppressions for host-provided or generated keys.

## Scope boundaries

DriftGuard does not claim to predict stochastic model behavior, replace a
compiler, evaluate secret values, or perform security vulnerability scanning.
Its value is enforcing local, deterministic deployment contracts.

## Resume bullet

Built a local-first Rust CI CLI that detects undocumented JS/TS, Python, and
Rust environment dependencies and validates AI prompt fixtures against JSON
Schema; added versioned JSON/Markdown reporting, Git diff scoping, diagnostics,
cross-platform CI, and 26 automated tests.
