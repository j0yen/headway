//! `headway` CLI — sanctioned cloudbuild-only build primitive.
//!
//! Subcommands:
//! - `headway build <crate-dir> [--dry-run] [--no-dry-run] [--format json|table]`
//! - `headway verify <daemon> [--format json|table]`
//! - `headway run <crate-dir> <daemon> [--format json|table] [--unit <svc>]`

#![deny(unsafe_code)]
#![warn(missing_docs, unreachable_pub)]

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use headway::{
    build, format_plan_table, format_run_table, format_verdict_table, format_verify_table,
    plan, run, verify, BuildConfig, RunConfig, VerifyConfig, VerifyOutcome,
};

/// Sanctioned cloudbuild-only build primitive.
#[derive(Debug, Parser)]
#[command(name = "headway", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Build a crate via cloudbuild.sh (never local cargo).
    Build(BuildArgs),
    /// Re-check a daemon's binstale verdict after a rebuild + install + reload.
    Verify(VerifyArgs),
    /// Compose build → install/reload → verify into a single receipt.
    Run(RunArgs),
}

#[derive(Debug, clap::Args)]
struct BuildArgs {
    /// Path to the crate directory to build.
    crate_dir: std::path::PathBuf,

    /// Dry-run mode (default): print the plan and cloudbuild command, but do
    /// not invoke cloudbuild.sh.
    #[arg(long, default_value_t = true, overrides_with = "no_dry_run")]
    dry_run: bool,

    /// Apply mode: actually invoke cloudbuild.sh build <name>.
    #[arg(long = "no-dry-run")]
    no_dry_run: bool,

    /// Output format.
    #[arg(long, default_value = "table")]
    format: OutputFormat,

    /// Disable the no-op-fresh guard. Build even if installed_version matches
    /// source_head.
    #[arg(long)]
    no_require_fresh: bool,
}

#[derive(Debug, clap::Args)]
struct VerifyArgs {
    /// Name of the daemon to probe (passed to `pgrep -n` to find the PID).
    daemon: String,

    /// Output format.
    #[arg(long, default_value = "table")]
    format: OutputFormat,
}

#[derive(Debug, clap::Args)]
struct RunArgs {
    /// Path to the crate directory to build.
    crate_dir: std::path::PathBuf,

    /// Name of the daemon to verify after build + install.
    daemon: String,

    /// Output format.
    #[arg(long, default_value = "table")]
    format: OutputFormat,

    /// systemd user unit name to restart after install (e.g. `wm-brain.service`).
    #[arg(long)]
    unit: Option<String>,

    /// Disable the no-op-fresh guard on the build step.
    #[arg(long)]
    no_require_fresh: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Json,
    Table,
}

fn main() -> Result<()> {
    // SIGPIPE reset — must be the first line of main() per the standing lesson.
    sigpipe::reset();

    let cli = Cli::parse();

    match cli.command {
        Commands::Build(args) => cmd_build(args),
        Commands::Verify(args) => cmd_verify(args),
        Commands::Run(args) => cmd_run(args),
    }
}

fn cmd_build(args: BuildArgs) -> Result<()> {
    let dry_run = !args.no_dry_run;
    let cfg = BuildConfig {
        cloudbuild_path: headway::resolve_cloudbuild_path_pub(),
        require_fresh: !args.no_require_fresh,
    };

    let bp = plan(&args.crate_dir, &cfg)
        .with_context(|| format!("failed to plan build for {}", args.crate_dir.display()))?;

    if dry_run {
        match args.format {
            OutputFormat::Json => {
                let j = serde_json::to_string_pretty(&bp)
                    .context("serialize BuildPlan")?;
                println!("{j}");
            }
            OutputFormat::Table => {
                println!("{}", format_plan_table(&bp));
            }
        }
        return Ok(());
    }

    // Apply mode
    let verdict = build(&bp, &cfg)?;

    // Non-zero exit for cloudbuild-unreachable and build-failed
    let exit_code = match verdict.status {
        headway::Status::Built | headway::Status::NoOpFresh => 0,
        headway::Status::CloudbuildUnreachable | headway::Status::BuildFailed => 1,
    };

    match args.format {
        OutputFormat::Json => {
            let j = serde_json::to_string_pretty(&verdict)
                .context("serialize BuildVerdict")?;
            println!("{j}");
        }
        OutputFormat::Table => {
            println!("{}", format_verdict_table(&verdict));
        }
    }

    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}

fn cmd_verify(args: VerifyArgs) -> Result<()> {
    let cfg = VerifyConfig {
        binstale_path: headway::resolve_binstale_path(),
    };

    // For standalone verify: pid_before is the current pid, verdict_before is unknown
    // (we have no prior snapshot in this call path).
    let pid_before = headway::probe_pid_pub(&args.daemon);
    let receipt = verify(
        &args.daemon,
        pid_before,
        headway::BinstaleVerdict::Unknown,
        &cfg,
    );

    let exit_code = match receipt.outcome {
        VerifyOutcome::Confirmed => 0,
        VerifyOutcome::Contradicted => 1,
        VerifyOutcome::Inconclusive => 2,
    };

    match args.format {
        OutputFormat::Json => {
            let j = serde_json::to_string_pretty(&receipt).context("serialize VerifyReceipt")?;
            println!("{j}");
        }
        OutputFormat::Table => {
            println!("{}", format_verify_table(&receipt));
        }
    }

    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}

fn cmd_run(args: RunArgs) -> Result<()> {
    let cfg = RunConfig {
        build: BuildConfig {
            cloudbuild_path: headway::resolve_cloudbuild_path_pub(),
            require_fresh: !args.no_require_fresh,
        },
        verify: VerifyConfig {
            binstale_path: headway::resolve_binstale_path(),
        },
        systemd_unit: args.unit,
    };

    let receipt = run(&args.crate_dir, &args.daemon, &cfg)?;

    // Exit code driven by verify outcome when present; otherwise by build status.
    let exit_code = if let Some(ref v) = receipt.verify {
        match v.outcome {
            VerifyOutcome::Confirmed => 0,
            VerifyOutcome::Contradicted => 1,
            VerifyOutcome::Inconclusive => 2,
        }
    } else {
        match receipt.build.status {
            headway::Status::Built | headway::Status::NoOpFresh => 0,
            headway::Status::CloudbuildUnreachable | headway::Status::BuildFailed => 1,
        }
    };

    match args.format {
        OutputFormat::Json => {
            let j = serde_json::to_string_pretty(&receipt).context("serialize RunReceipt")?;
            println!("{j}");
        }
        OutputFormat::Table => {
            println!("{}", format_run_table(&receipt));
        }
    }

    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}
