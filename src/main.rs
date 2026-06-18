//! `headway` CLI — sanctioned cloudbuild-only build primitive.
//!
//! Usage: `headway build <crate-dir> [--dry-run] [--no-dry-run] [--format json|table]`

#![deny(unsafe_code)]
#![warn(missing_docs, unreachable_pub)]

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use headway::{
    build, format_plan_table, format_verdict_table, plan, BuildConfig,
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
