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
    println!("GET {url}");
    reqwest::blocking::Client::new()
        .get(url)
        .header("User-Agent", "gha-logs-miner")
        .header("Authorization", format!("bearer {token}"))
        .send()
}

fn request_github_json<Resp: serde::de::DeserializeOwned>(token: &str, url: &str) -> Resp {
    request_github(token, url).unwrap().json::<Resp>().unwrap()
}

#[derive(Deserialize)]
struct WorkflowRun {
    id: u64,
    name: String,
    run_started_at: Date,
    url: String,
}

#[derive(Deserialize)]
struct WorkflowRunsResponse {
    workflow_runs: Vec<WorkflowRun>,
}

fn get_workflow_runs(token: &str, api_url: &str, days_back: i64) -> Vec<WorkflowRun> {
    let mut workflow_runs = vec![];
    let age_limit = Utc::now() - Duration::days(days_back);
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
        println!("Run count collected so far: {}", workflow_runs.len());
    }
    workflow_runs
}

fn get_silent_failure_count_from_log_url(token: &str, url: &str, date: &Date) -> Result<u64> {
    let pattern = Regex::new(r"^Nightwatch.*Run Nightwatch.txt$").unwrap();
    let success_pattern = Regex::new(r"OK.*total assertions passed").unwrap();
    let mut silent_failure_count = 0;

    let mut response = request_github(&token, &url)?;
    let mut buffer = Vec::<u8>::new();
    std::io::copy(&mut response, &mut buffer)?;
    let cursor = Cursor::new(buffer);
    let mut zip = ZipArchive::new(cursor).unwrap();

    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        if pattern.is_match(file.name()) {
            let length = file.size() as usize;
            let mut buf = vec![0u8; length];
            std::io::copy(&mut file, &mut buf)?;
            let reader = BufReader::new(buf.as_slice());
            let mut last_line = "".to_string();
            let mut has_timeout = false;
            for line in reader.lines().flatten() {
                has_timeout |= line.contains("Timed out while waiting for element");
                last_line = line;
            }
            if has_timeout && success_pattern.is_match(&last_line) {
                silent_failure_count += 1;
                println!("FAILURE in log: {:?}, file: {}", date, file.name());
            }
        }
    }
    if silent_failure_count == 0 {
        println!("SUCCESS in log: {date:?}");
    }
    Ok(silent_failure_count)
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
    let api_url = format!("https://api.github.com/repos/{user}/{repo}");

    let mut workflow_runs = get_workflow_runs(&token, &api_url, 365);

    workflow_runs.sort_by_key(|run| run.run_started_at);

    let result = workflow_runs
        .par_iter_mut()
        .map(|run| {
            let url = format!("{}/actions/runs/{}/logs", api_url, run.id);
            let res = get_silent_failure_count_from_log_url(&token, &url, &run.run_started_at);
            (run.url.clone(), run.run_started_at.clone(), res)
        })
        .collect::<Vec<_>>();

    println!("Results: {result:#?}");
}
