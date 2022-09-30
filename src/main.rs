mod util;

use crate::util::{get_silent_failure_count_from_log_url, get_workflow_runs};
use anyhow::{anyhow, Context, Result};
use chrono::prelude::*;
use chrono::Duration;
use clap::{Parser, Subcommand};
use dotenv::dotenv;
use itertools::Itertools;
use rayon::prelude::*;
use regex::Regex;
use reqwest::blocking::Response;
use serde::Deserialize;
use std::io::{BufRead, BufReader, Cursor};
use zip::ZipArchive;

#[derive(Parser)]
#[command(author = None, version = None, about = None, long_about = None, rename_all_env = "SCREAMING_SNAKE_CASE")]
pub struct Cli {
    /// Github API TOKEN
    #[arg(long, env, hide_env_values = true)]
    github_token: String,

    /// Github user
    #[arg(long, env, default_value = "AnonosDev")]
    github_user: String,

    /// Github repository
    #[arg(long, env, default_value = "bigprivacy-engine")]
    github_repo: String,

    /// The branch to compare against
    #[arg(long, env, default_value = "develop")]
    branch: String,

    /// Name of the Github Actions workflow
    #[arg(long, env, default_value = "CD")]
    workflow: String,

    /// The number of days to look back
    #[arg(long, env, default_value = "7")]
    days_back: i64,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Find silently failing Nightwatch tests
    Nightwatch,

    /// Find failing ZAP logs
    Zap {
        /// Output file base name, "-[date].txt" will be appended
        base_name: String,
    },
}

type FilterCallback = fn(BufReader<&[u8]>) -> Result<()>;

fn nightwatch_filter(reader: BufReader<&[u8]>) -> Result<()> {
    let current_test_pattern = Regex::new(r" Running:  (.*)$")?;
    let success_pattern = Regex::new(r"OK.*total assertions passed")?;
    let fail_pattern = Regex::new(r"FAILED.* assertions failed")?;
    let stack_dump_pattern = Regex::new(r"    at ")?;

    let mut failed_tests = vec![];
    let mut current_test = "[unknown]".to_string();
    let mut failed = false;
    for line in reader.lines() {
        let line = line?;
        if let Some(cap) = current_test_pattern.captures(&line) {
            if failed {
                failed_tests.push(current_test);
                failed = false;
            }
            current_test = cap
                .get(1)
                .context("Test pattern not found.")?
                .as_str()
                .to_string();
        } else if stack_dump_pattern.is_match(&line) {
            failed = true;
        } else if success_pattern.is_match(&line) {
            if failed {
                failed_tests.push(current_test.clone());
                failed = false;
            }
        } else if fail_pattern.is_match(&line) {
            failed = false;
        }
    }
    if failed {
        failed_tests.push(current_test);
    }
    if failed_tests.len() > 0 {
        return Err(anyhow!(
            "Found silent failures in Nightwatch log.\n{}",
            failed_tests
                .into_iter()
                .map(|t| format!("  - {t}"))
                .join("\n")
        ));
    }
    Ok(())
}

fn zap_filter(reader: BufReader<&[u8]>) -> Result<()> {
    for line in reader.lines() {
        let line = line?;
        if line.contains("FAIL-NEW") {
            return Err(anyhow!("Found FAIL-NEW in ZAP log."));
        }
    }
    Ok(())
}

fn main() {
    dotenv().unwrap();
    let cli = Cli::parse();
    let mut filter_callback: FilterCallback;
    let mut pattern = Regex::new(r"^Nightwatch.*Run Nightwatch.txt$").unwrap();

    match &cli.command {
        Commands::Nightwatch => {
            println!("Finding silently failing Nightwatch tests");
            filter_callback = nightwatch_filter;
            pattern = Regex::new(r"^Nightwatch.*Run Nightwatch.txt$").unwrap();
        }
        Commands::Zap { base_name } => {
            println!("Finding ZAP");
            filter_callback = zap_filter;
            pattern = Regex::new(r"^Nightwatch.*Run Nightwatch.txt$").unwrap();
        }
    }

    rayon::ThreadPoolBuilder::new()
        .num_threads(8) // larger parallelism seems to break the Github API somehow
        .build_global()
        .unwrap();

    let api_url = format!(
        "https://api.github.com/repos/{}/{}",
        cli.github_user, cli.github_repo
    );

    let mut workflow_runs = get_workflow_runs(&cli, &api_url);
    println!(
        "Found {} workflow runs, downloading logs...",
        workflow_runs.len()
    );

    workflow_runs.sort_by_key(|run| run.run_started_at);
    workflow_runs.par_iter_mut().for_each(|run| {
        let url = format!("{}/actions/runs/{}/logs", api_url, run.id);
        get_silent_failure_count_from_log_url(
            &cli.github_token,
            &url,
            &run,
            &pattern,
            filter_callback,
        )
        .unwrap();
    });
}
