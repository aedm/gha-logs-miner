use crate::{Cli, FilterCallback};
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

type Date = DateTime<Utc>;

#[derive(Deserialize, Clone, Debug)]
pub struct WorkflowRun {
    pub id: u64,
    pub name: String,
    pub run_started_at: Date,
    pub html_url: String,
}

#[derive(Deserialize)]
struct WorkflowRunsResponse {
    workflow_runs: Vec<WorkflowRun>,
}

fn request_github(token: &str, url: &str) -> reqwest::Result<Response> {
    reqwest::blocking::Client::new()
        .get(url)
        .header("User-Agent", "gha-logs-miner")
        .header("Authorization", format!("bearer {token}"))
        .send()
}

fn request_github_json<Resp: serde::de::DeserializeOwned>(token: &str, url: &str) -> Resp {
    request_github(token, url).unwrap().json::<Resp>().unwrap()
}

pub fn get_workflow_runs(cli: &Cli, api_url: &str) -> Vec<WorkflowRun> {
    let mut workflow_runs = vec![];
    let days_back = cli.days_back;
    let branch = &cli.branch;
    println!("Getting the last {days_back} days worth of workflow runs.");

    let age_limit = Utc::now() - Duration::days(days_back);
    for page in 0.. {
        let url = format!("{api_url}/actions/runs?per_page=100&page={page}&branch={branch}");
        let runs = request_github_json::<WorkflowRunsResponse>(&cli.github_token, &url)
            .workflow_runs
            .into_iter()
            .filter(|run| run.run_started_at > age_limit)
            .collect_vec();
        if runs.len() == 0 {
            break;
        }
        let mut runs = runs
            .into_iter()
            .filter(|run| run.name == cli.workflow)
            .collect_vec();
        workflow_runs.append(&mut runs);
        println!("  ...page {}, got {} so far", page, workflow_runs.len());
    }
    workflow_runs
}

pub fn get_silent_failure_count_from_log_url(
    token: &str,
    url: &str,
    run: &WorkflowRun,
    pattern: &Regex,
    cb: FilterCallback,
) -> Result<()> {
    let mut success = true;
    let mut response = request_github(&token, &url)?;
    let mut buffer = Vec::<u8>::new();
    std::io::copy(&mut response, &mut buffer)?;
    let cursor = Cursor::new(buffer);
    let mut zip = ZipArchive::new(cursor)?;

    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        if pattern.is_match(file.name()) {
            let length = file.size() as usize;
            let mut buf = vec![0u8; length];
            std::io::copy(&mut file, &mut buf)?;

            let reader = BufReader::new(buf.as_slice());
            if let Err(err) = cb(reader) {
                success = false;
                println!(
                    "FAIL: {}\n  log: {}\n  date: {:?}\n  reason: {}\n",
                    run.html_url,
                    file.name(),
                    run.run_started_at,
                    err
                );
            }
        }
    }
    if success {
        println!("SUCCESS at {:?}", run.run_started_at);
    }
    Ok(())
}
