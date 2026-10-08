# DriftGuard Autopilot - GitLab Life After Code

## Current Status

This branch is a contest-period implementation candidate for the GitLab Life After Code hackathon. It is not yet registered, imported to GitLab, deployed, or submitted.

The repository includes:

- `.gitlab/duo/flows/driftguard-autopilot.yml`: a GitLab Duo custom flow with analysis, human approval, remediation, and independent verification stages.
- `.gitlab/duo/agent-config.yml`: the Rust execution environment and bounded network policy for GitLab Duo flow jobs.
- `.gitlab-ci.yml`: formatting, Clippy, test, and DriftGuard contract jobs with a machine-readable report artifact.
- `AGENTS.md`: repository scope, safety boundaries, and the commands required before a change may be presented as complete.

## Flow Contract

1. The analyzer reproduces an environment or prompt-contract failure without editing files.
2. A human reviews the proposed files, test plan, and expected commands and must approve, modify, or reject the plan.
3. The remediator applies only the approved change, adds focused regression coverage, and opens a merge request only after the required checks pass.
4. The verifier independently inspects the diff and reruns the checks before posting evidence.

This is intentionally a supervised flow. It does not claim autonomous production remediation.

## Reproducible Demo Scenario

Use a small merge request that adds an undeclared environment read to the broken example:

```typescript
const newlyRequiredKey = process.env.NEW_REQUIRED_KEY;
```

Expected sequence:

1. The GitLab pipeline fails `driftguard-contracts` and retains `driftguard-report.json`.
2. The Duo flow reproduces the failure and proposes the smallest declaration or code correction plus a regression test.
3. The human approval gate pauses execution before any edit.
4. After approval, the flow makes the bounded change and runs:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --locked
cargo run --locked -- check --json
```

5. The verifier reports the observed outcomes and rejects the change if scope expanded or a check failed.

## Submission Acceptance Criteria

- Contributor onboarding and hackathon registration are confirmed in the authenticated GitLab and Devpost interfaces.
- The existing MIT repository is imported into a public GitLab project with visible contest-period history.
- The flow is created or enabled through the GitLab Duo Agent Platform and is shown running, not merely represented by a local script.
- At least one human approval and one rejected or modified proposal are demonstrated.
- GitLab CI retains the JSON contract artifact and all required gates pass on the final commit.
- A public deployment exists before any submission text says the project is deployed.
- The public YouTube demo is under three minutes and shows the live GitLab flow, approval, pipeline, and evidence artifact.

## Truthful Boundaries

- Local YAML parsing proves syntax only; GitLab must still validate the custom flow schema after onboarding.
- Local Rust checks do not prove GitLab runner, Duo session, trigger, or hosted deployment behavior.
- No prize, acceptance, production use, or autonomous repair claim is valid until independently confirmed.
