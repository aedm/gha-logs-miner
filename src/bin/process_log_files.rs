use itertools::Itertools;
use regex::Regex;
use std::fs::File;
use std::io::{BufRead, BufReader};
use zip::ZipArchive;

fn find_log_files() -> Vec<String> {
    let pattern = Regex::new(r"/.*\.zip$").unwrap();
    std::fs::read_dir("./logs")
        .unwrap()
        .flatten()
        .map(|d| d.path().to_str().unwrap().to_string())
        .filter(|f| pattern.is_match(f))
        .collect_vec()
}

fn get_silent_failure_count_from_log_file(path: &str) -> u64 {
    let pattern = Regex::new(r"^Nightwatch.*Run Nightwatch.txt$").unwrap();
    let success_pattern = Regex::new(r"OK.*total assertions passed").unwrap();
    let mut silent_failure_count = 0;
    let file = File::open(path).unwrap();
    let mut zip = ZipArchive::new(file).unwrap();
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).unwrap();
        if pattern.is_match(file.name()) {
            let length = file.size() as usize;
            let mut buf = vec![0u8; length];
            std::io::copy(&mut file, &mut buf).unwrap();
            let mut reader = BufReader::new(buf.as_slice());
            let mut last_line = "".to_string();
            let mut has_timeout = false;
            for line in reader.lines().flatten() {
                if line.contains("Timed out while waiting for element") {
                    has_timeout = true;
                }
                last_line = line;
            }
            let success = success_pattern.is_match(&last_line);
            if has_timeout && success {
                silent_failure_count += 1;
                println!("FAILURE in log: {}, file: {}", path, file.name());
            }
        }
    }
    silent_failure_count
}

fn main() {
    let mut logs = find_log_files();
    logs.sort();
    let result = logs
        .iter()
        .map(|f| get_silent_failure_count_from_log_file(f))
        .collect_vec();
    println!("Result: {result:?}");
}
