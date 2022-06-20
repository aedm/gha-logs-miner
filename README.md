# Github Actions logs miner

Finds Nightwatch errors in historical Github Actions logs.

It downloads zipped logs using the Github API into memory, unzips them, extracts Nightwatch logs, checks for unhandled timeouts, then writes a summary to stdout. It doesn't write any files.

The code processes logs in parallel. Adjust the thread pool size if there too many failures. Github API is sometimes a bit unreliable.

## Requirements

1. Install Rust.
2. Make `.env` from `.env.example`, fill it in.

## Usage

`cargo run`

## Output

The result is a list of silently failed tests:

```
FAILURE: https://github.com/AnonosDev/bigprivacy-engine/actions/runs/2527438344
  date: 2022-06-20T07:54:12Z
  file: Nightwatch (jJkKlL)/11_Run Nightwatch.txt
    Data Transformers - Filters Popup - Cancel filtering
    Data Transformers - Filters Popup - Clear filters
    Data Transformers - Filter by Jurisdiction
    Data Transformers - Filter by Use Case
    Data Transformers - Filter by Relinking - No results
    Data Transformers - Filters Popup - Closes when creating Data Transformer
```
