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

The result looks like a list of this:

```
(
    "https://api.github.com/repos/AnonosDev/bigprivacy-engine/actions/runs/2496023109",
    2022-06-14T14:38:30Z,
    Ok(
        2,
    ),
),
```

That means there are 2 undetected failures in that run. If there are no undetected failures, it says `Ok(0)`. If the processing of a log fails, it displays an error instead of the count.