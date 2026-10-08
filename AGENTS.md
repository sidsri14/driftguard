# DriftGuard Agent Guide

## Purpose

DriftGuard is a local-first Rust CLI for deterministic environment and AI output contract checks. Keep changes bounded to this scope. It does not evaluate model quality, inspect secret values, or replace language-specific compilers and security scanners.

## Required Checks

Run these commands before presenting a change as complete:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --locked
cargo run --locked -- check --json
```

## Change Rules

- Reproduce the failure before editing code.
- Prefer the smallest fix that addresses the reproduced failure.
- Add or update a focused regression test for behavioral changes.
- Preserve JSON report compatibility unless the task explicitly changes the format version.
- Do not add dependencies, change release workflows, or weaken checks without explicit human approval.
- Never read, print, commit, or request secret values.
- Report the exact commands run and their outcomes. Do not invent test counts, benchmark results, or deployment evidence.
