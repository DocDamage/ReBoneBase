//! Repeatable large-project performance checks.
//!
//! cargo run --release --example perf_check -- [--scale small|large] [--runs N]
//!     [--check perf/budgets.json] [--json report.json]
use anyhow::{Context, Result, bail};
use rebonebase::perf::{self, Scale};

fn main() -> Result<()> {
    let mut scale = Scale::LARGE;
    let mut runs = 3;
    let mut budgets = None;
    let mut json = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().with_context(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--scale" => {
                scale = match value()?.as_str() {
                    "small" => Scale::SMALL,
                    "large" => Scale::LARGE,
                    other => bail!("Unknown scale {other}; use small or large"),
                }
            }
            "--runs" => runs = value()?.parse().context("--runs must be a number")?,
            "--check" => budgets = Some(value()?),
            "--json" => json = Some(value()?),
            other => bail!("Unknown argument {other}"),
        }
    }
    if cfg!(debug_assertions) {
        eprintln!("warning: debug build; budgets assume --release");
    }
    let report = perf::run(scale, runs)?;
    println!(
        "{} clips × {} frames ({} rendered frames per full export), median of {} runs",
        scale.clips, scale.frames, report.frames, runs
    );
    for c in &report.checks {
        println!(
            "{:<24} {:>9.1} ms  (max {:>8.1})  {}",
            c.name, c.median_ms, c.max_ms, c.detail
        );
    }
    if let Some(path) = json {
        std::fs::write(&path, serde_json::to_vec_pretty(&report)?)?;
    }
    if let Some(path) = budgets {
        let text = std::fs::read_to_string(&path).with_context(|| format!("Reading {path}"))?;
        let budgets: serde_json::Value = serde_json::from_str(&text)?;
        let key = if scale.dense_keys { "large" } else { "small" };
        let failures = perf::over_budget(&report, &budgets[key]);
        if !failures.is_empty() {
            for f in &failures {
                eprintln!("over budget: {f}");
            }
            bail!(
                "{} performance checks exceeded their budgets",
                failures.len()
            );
        }
        println!("All checks within {path} ({key})");
    }
    Ok(())
}
