use anyhow::Result;
use chrono::prelude::*;
use chrono::Duration;
use dotenv::dotenv;
use itertools::Itertools;
use rayon::prelude::*;
use regex::Regex;
use reqwest::blocking::Response;
use serde::Deserialize;
use std::io::{BufRead, BufReader, Cursor};
use zip::ZipArchive;

const WORKFLOW_NAME: &'static str = "CD";
type Date = DateTime<Utc>;

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

#[derive(Deserialize, Clone, Debug)]
struct WorkflowRun {
    id: u64,
    name: String,
    run_started_at: Date,
    html_url: String,
}

#[derive(Deserialize)]
struct WorkflowRunsResponse {
    workflow_runs: Vec<WorkflowRun>,
}

fn get_workflow_runs(token: &str, api_url: &str, days_back: i64) -> Vec<WorkflowRun> {
    let mut workflow_runs = vec![];
    let age_limit = Utc::now() - Duration::days(days_back);
    println!("Getting the last {days_back} days worth of workflow runs.");
    for page in 0.. {
        let url = format!("{api_url}/actions/runs?per_page=100&page={page}&branch=develop");
        let runs = request_github_json::<WorkflowRunsResponse>(&token, &url)
            .workflow_runs
            .into_iter()
            .filter(|run| run.run_started_at > age_limit)
            .collect_vec();
        if runs.len() == 0 {
            break;
        }
        let mut runs = runs
            .into_iter()
            .filter(|run| run.name == WORKFLOW_NAME)
            .collect_vec();
        workflow_runs.append(&mut runs);
        println!("  ...page {}, got {} so far", page, workflow_runs.len());
    }
    workflow_runs
}

fn get_silent_failure_count_from_log_url(token: &str, url: &str, run: &WorkflowRun) -> Result<()> {
    let pattern = Regex::new(r"^Nightwatch.*Run Nightwatch.txt$").unwrap();
    let current_test_pattern = Regex::new(r" Running:  (.*)$").unwrap();
    let success_pattern = Regex::new(r"OK.*total assertions passed").unwrap();
    let fail_pattern = Regex::new(r"FAILED.* assertions failed").unwrap();
    let stack_dump_pattern = Regex::new(r"    at ").unwrap();
    let mut silent_failure_count = 0;

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
            let mut failed_tests = vec![];
            let mut current_test = "[unknown]".to_string();
            let mut failed = false;
            for line in reader.lines().flatten() {
                if let Some(cap) = current_test_pattern.captures(&line) {
                    if failed {
                        failed_tests.push(current_test);
                        failed = false;
                    }
                    current_test = cap.get(1).unwrap().as_str().to_string();
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
                silent_failure_count += 1;
                println!(
                    "FAILURE: {}\n  date: {:?}\n  file: {}",
                    run.html_url,
                    run.run_started_at,
                    file.name()
                );
                for line in failed_tests {
                    println!("    {line}");
                }
            }
        }
    }
    if silent_failure_count == 0 {
        println!("SUCCESS at {:?}", run.run_started_at);
    }
    Ok(())
}

fn main() {
    dotenv().unwrap();
    rayon::ThreadPoolBuilder::new()
        .num_threads(8) // larger parallelism seems to break the Github API somehow
        .build_global()
        .unwrap();

    let token = std::env::var("GITHUB_TOKEN").unwrap();
    let user = std::env::var("GITHUB_USER").unwrap();
    let repo = std::env::var("GITHUB_REPO").unwrap();
    let days_back = std::env::var("SEARCH_RANGE_IN_DAYS")
        .unwrap()
        .parse()
        .unwrap();
    let api_url = format!("https://api.github.com/repos/{user}/{repo}");

    let mut workflow_runs = get_workflow_runs(&token, &api_url, days_back);
    println!(
        "Got {} workflow runs in range, downloading logs...",
        workflow_runs.len()
    );

    workflow_runs.sort_by_key(|run| run.run_started_at);
    workflow_runs.par_iter_mut().for_each(|run| {
        let url = format!("{}/actions/runs/{}/logs", api_url, run.id);
        get_silent_failure_count_from_log_url(&token, &url, &run).unwrap();
    });
}
