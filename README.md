# headway

`headway` recompiles a crate the sanctioned way — every build routed through `cloudbuild.sh` on the Hetzner builder, never local `cargo` — and returns a structured verdict for build, install-and-reload, and verify.

## Why it exists

The fleet has a standing rule: all cargo builds route through cloudbuild, not the local machine. But the rule had no primitive behind it. The one automated rebuild path that existed, `rollout`, called `cargo build --release` locally — which is exactly what the rule forbids. A rule without a tool to enforce it is a rule that gets broken under deadline.

`headway` is that tool. Point it at a crate, and it builds on the remote builder, pulls the artifact, and tells you what happened — in a form a script or a person can act on. There is no code path that runs `cargo build` or `cargo install` locally; a test greps the build path to keep it that way.

## Install

```sh
git clone https://github.com/j0yen/headway.git
cd headway
cargo install --path .
```

Requires `cargo` / `rustc 1.85+`. `headway` shells out to `cloudbuild.sh`; point `HEADWAY_CLOUDBUILD` at the script (or a fixture for testing).

## Quickstart

```sh
# Show the plan and the exact cloudbuild command — mutates nothing (this is the default)
headway build ./my-crate --format json

# Actually build on the remote builder
headway build ./my-crate --no-dry-run

# After a rebuild+install+reload, confirm a daemon picked up the new binary
headway verify wm-brain

# Compose build → install/reload → verify into one receipt
headway run ./my-crate wm-brain --unit wm-brain.service
```

Dry-run is the default posture for `build`: it prints a `BuildPlan` and the `cloudbuild.sh build <name>` line it would run, then exits 0 without touching anything.

## Subcommands

| Command | What it does |
|---|---|
| `build <crate-dir>` | Routes the build through `cloudbuild.sh`, returns a `BuildVerdict`. Dry-run by default; `--no-dry-run` applies. Skips as `no-op-fresh` when the installed version already matches source HEAD, unless `--no-require-fresh`. |
| `verify <daemon>` | Re-queries `binstale` after a rebuild and maps the result: `fresh → confirmed`, `behind-head → contradicted`, `unknown → inconclusive`. Records `pid_before`/`pid_after` so you can confirm the daemon actually bounced. Exit 0/1/2. |
| `run <crate-dir> <daemon>` | Composes build → install/reload → verify into one `RunReceipt`. `--unit <svc>` restarts a systemd user unit after install. |

## Behavior worth knowing

- **Never builds local.** If `cloudbuild.sh` is absent or returns the unreachable code, `build` reports `status: cloudbuild-unreachable`, writes a structured error to stderr, and exits non-zero. It does not fall back to a local build.
- **Freshness guard.** A build against an already-fresh crate is a `no-op-fresh` and skips cloudbuild entirely — the same `--require-fresh` precedent as the agorabus reload path.
- **Honest verdicts.** A `contradicted` verify outcome is never silently swallowed.

## Where it fits

The foundation crate of the `headway` family and the build primitive for the wider fleet: `cloudbuild` does the remote compile, `binstale` reports whether a running daemon is behind HEAD, and `headway` ties build, reload, and verify into one receipt.

## Status

Three subcommands shipped — `build` (v0.1.0), `verify` and `run` (v0.2.0). Each acceptance criterion has a matching integration test under `tests/`; cloudbuild and binstale are stubbed in tests via `HEADWAY_CLOUDBUILD` and `HEADWAY_BINSTALE`.

## License

MIT © Joe Yen
