use chrono::prelude::*;
use chrono::Duration;
use dotenv::dotenv;
use itertools::Itertools;
use rayon::prelude::*;
use reqwest::blocking::Response;
use serde::Deserialize;
use std::fs;
use std::fs::File;
use std::path::Path;

const WORKFLOW_NAME: &'static str = "CD";
type Date = DateTime<Utc>;

fn request_github(token: &str, url: &str) -> Response {
    println!("GET {url}");
    reqwest::blocking::Client::new()
        .get(url)
        .header("User-Agent", "gha-logs-miner")
        .header("Authorization", format!("bearer {token}"))
        .send()
        .unwrap()
}

fn request_github_json<Resp: serde::de::DeserializeOwned>(token: &str, url: &str) -> Resp {
    request_github(token, url).json::<Resp>().unwrap()
}

#[derive(Deserialize, Debug)]
struct WorkflowRun {
    id: u64,
    name: String,
    head_branch: String,
    run_attempt: u64,
    run_started_at: Date,
    logs_url: String,
}

#[derive(Deserialize, Debug)]
struct WorkflowRunsResponse {
    workflow_runs: Vec<WorkflowRun>,
}

fn main() {
    dotenv().unwrap();
    rayon::ThreadPoolBuilder::new()
        .num_threads(8)
        .build_global()
        .unwrap();

    let token = std::env::var("GITHUB_TOKEN").unwrap();
    let user = std::env::var("GITHUB_USER").unwrap();
    let repo = std::env::var("GITHUB_REPO").unwrap();

    let api_url = format!("https://api.github.com/repos/{user}/{repo}");
    let age_limit = Utc::now() - Duration::days(365);
    let mut workflow_runs = vec![];
    for page in 0.. {
        let url = format!("{api_url}/actions/runs?per_page=100&page={page}&branch=develop");
        let mut runs = request_github_json::<WorkflowRunsResponse>(&token, &url);
        println!("response.workflow_runs: {}", runs.workflow_runs.len());
        let mut runs = runs
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
        println!("Total count: {}", workflow_runs.len());
    }

    workflow_runs.sort_by_key(|run| run.run_started_at);

    workflow_runs.par_iter_mut().for_each(|run| {
        let file_path = format!(
            "logs/{}-{}.zip",
            run.run_started_at.format("%Y%m%d-%H%M%S"),
            run.run_attempt
        );
        if Path::new(&file_path).exists() {
            println!("Already downloaded: '{file_path}'");
            return;
        }
        let url = format!("{}/actions/runs/{}/logs", api_url, run.id);
        let mut response = request_github(&token, &url);
        println!("Downloading '{file_path}'");
        std::io::copy(&mut response, &mut File::create(file_path).unwrap()).unwrap();
    });

    println!("Runs: {}", workflow_runs.len());
}
