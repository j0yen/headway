# Changelog

## v0.2.0 — 2026-06-18

### Added (PRD-headway-verify)

- `headway verify <daemon>` — re-queries `binstale` after a rebuild+install+reload
  and maps `fresh→confirmed`, `behind-head→contradicted`, `unknown→inconclusive`.
  Exit codes: 0/1/2. Contradicted outcome is never silently swallowed.
- `headway run <crate-dir> <daemon>` — composes build→reload→verify into a single
  receipt (`RunReceipt`). Optional `--unit <svc>` triggers `systemctl --user restart`.
- `VerifyReceipt` JSON with `pid_before`/`pid_after` so readers can confirm the
  daemon actually bounced on reload.
- `BinstaleVerdict` and `VerifyOutcome` types, fully serde-serializable.

## v0.1.0 — 2026-06-17

Initial release: `headway build` subcommand routing all crate recompiles through
`cloudbuild.sh` (Hetzner), never local cargo. Dry-run default, `BuildPlan` +
`BuildVerdict` JSON receipts.
