# Changelog

## Unreleased

- Add `check --staged` and install pre-commit hooks with staged-file checking.
- Recognize env declarations in optional deployment manifests including Docker Compose, Kubernetes YAML, Vercel JSON, Netlify TOML, and `.env.production`.
- Flag dynamic env access that cannot be statically verified.
- Fail changed checks when a mapped prompt file is deleted.
- Expand env scanner coverage for Vite, Deno, Bun, Python `os.environ.get`, Rust `var_os`, and Rust `option_env!`.

## 0.3.0 - 2026-07-20

- Add stable `check --json` reports.
- Add `doctor` configuration diagnostics with terminal and JSON output.
- Add project-wide ignored env keys and line-level suppression directives.
- Reject missing env manifests and invalid globs during check preflight.
- Stop generating inactive placeholder prompt contracts during `init`.
- Add a production-path scanner benchmark and cross-platform quality CI.
- Add reproducible pass/fail demo scripts and structured pilot feedback.

## 0.2.0 - 2026-07-19

- Add changed-file env scan scoping and Markdown reports.
- Add prompt template input validation.
- Add GitHub Actions PR reporting and release binaries.
- Ignore env access examples inside supported source-language comments.
