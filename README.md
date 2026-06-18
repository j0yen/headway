# headway

The fleet has no reusable primitive that recompiles a crate the *sanctioned*

## Overview

The fleet has no reusable primitive that recompiles a crate the *sanctioned*
way. The only automated rebuild path that exists — `rollout` — builds locally
with `cargo build --release`, which violates the standing hard rule that all
cargo routes through cloudbuild. This PRD builds `headway`, a new Rust CLI +
library whose one job is: given a crate directory, route the build through
`cloudbuild.sh build <crate>`, pull the artifact, and emit a structured verdict.
It is the foundation crate the rest of the `headway` fleet extends.


## Acceptance


1. `headway build <crate-dir> --dry-run --format json` prints a `BuildPlan` plus
   the exact `cloudbuild.sh build <name>` command line that would run, mutates
   nothing, and exits 0. (Default posture is dry-run.)
2. With `--no-dry-run` (apply), `build` invokes `cloudbuild.sh build <name>` as a
   subprocess and returns a `BuildVerdict` whose `cloudbuild_status` reflects the
   subprocess outcome. (Tested with a stub cloudbuild script via
   `HEADWAY_CLOUDBUILD` pointing at a fixture.)
3. There is no code path that runs `cargo build`/`cargo install` locally. A grep
   in the test suite asserts the crate contains no local-cargo invocation in the
   build path; the only compiler invocation is via the cloudbuild subprocess.
4. When the configured cloudbuild script is absent or exits with the
   unreachable code, `build` returns `status: cloudbuild-unreachable`, emits a
   structured error to stderr, and exits non-zero — never silently builds local.
5. When `installed_version == source_head` (already fresh), `build` returns
   `status: no-op-fresh` without invoking cloudbuild, unless `--no-require-fresh`
   is passed (matches the agorabus reload `--require-fresh` precedent).
6. `BuildVerdict` records `version_before` and `version_after`; on a successful
   build against a fixture they differ, on a no-op they are equal.
7. `headway --version` and `headway build --help` work on the freshly-built
   binary; `cargo test` is green and `clippy` produces no new warnings over the
   repo baseline.

## Install

```sh
cargo install --path .
```

## License

MIT © Joe Yen
