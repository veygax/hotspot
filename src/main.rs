use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::collections::{HashMap, VecDeque};
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let mut path_arg = ".";
    let mut limit: Option<usize> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-n" | "--limit" => {
                if i + 1 < args.len() {
                    limit = args[i + 1].parse().ok();
                    i += 2;
                } else {
                    i += 1;
                }
            }
            arg => {
                path_arg = arg;
                i += 1;
            }
        }
    }

    let scan_path = path_arg;

    let scan_path_canonical = fs::canonicalize(scan_path)?;

    println!("Scanning path: {}", scan_path_canonical.display());

    let scan_spinner = ProgressBar::new_spinner();
    scan_spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );
    scan_spinner.set_message("Fetching git history...");
    scan_spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let (commit_files, commit_order) = get_introductions(&scan_path_canonical)?;

    scan_spinner.finish_with_message(format!("Found {} introductions", commit_files.len()));

    if commit_files.is_empty() {
        println!("No introductions found.");
        return Ok(());
    }

    println!("Finding modified files...");

    let progress = ProgressBar::new(commit_order.len() as u64);
    progress.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:20.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("=>-"),
    );
    progress.set_message("Processing commits...");

    let commit_to_modified_files: HashMap<String, Vec<String>> = commit_order
        .par_iter()
        .progress_with(progress.clone())
        .filter_map(|commit_hash| {
            get_modified_files_for_commit(&scan_path_canonical, commit_hash)
                .map(|files| (commit_hash.clone(), files))
        })
        .collect();

    progress.finish_with_message("Analysis complete!");

    println!("\nAggregating results...");

    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut file_commits: HashMap<String, String> = HashMap::new();

    for commit_hash in &commit_order {
        if let Some(modified_files) = commit_to_modified_files.get(commit_hash) {
            for file in modified_files {
                *counts.entry(file.clone()).or_insert(0) += 1;
                file_commits
                    .entry(file.clone())
                    .or_insert_with(|| commit_hash.clone());
            }
        }
    }

    let mut ranked: Vec<(String, usize, String)> = counts
        .into_iter()
        .map(|(file, count)| {
            let short_hash = file_commits
                .get(&file)
                .map(|h| &h[..8])
                .unwrap_or("--------");
            (file, count, short_hash.to_string())
        })
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1));

    let max_count = ranked.first().map(|(_, c, _)| *c).unwrap_or(0);
    let bar_width = 30;

    println!(
        "{:<6} | {:<9} | {:<50} | {}",
        "Count", "Modified", "File", "Hotness"
    );
    println!("{}", "-".repeat(120));

    let mut ranked_iter = ranked.into_iter();
    let take_count = limit.unwrap_or(usize::MAX);

    for (file, count_val, short_hash) in ranked_iter.by_ref().take(take_count) {
        let pct = if max_count > 0 {
            (count_val as f64 / max_count as f64) * 100.0
        } else {
            0.0
        };
        let bar_len = (pct / 100.0 * bar_width as f64) as usize;
        let bar = "█".repeat(bar_len);
        let truncated_file = if file.len() > 50 {
            if let Some(slash_pos) = file.rfind('/') {
                let filename = &file[slash_pos..];
                let prefix = &file[..8.min(file.len())];
                format!("{}...{}", prefix, filename)
            } else {
                file.clone()
            }
        } else {
            file.clone()
        };
        println!(
            "{:<6} | {:<9} | {:<50} | {}",
            count_val, short_hash, truncated_file, bar
        );
    }

    Ok(())
}

fn get_introductions(
    scan_path: &Path,
) -> Result<(HashMap<String, String>, Vec<String>), Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args([
            "log",
            "--all",
            "--format=COMMIT:%H",
            "--name-only",
            "--diff-filter=A",
        ])
        .current_dir(scan_path)
        .output()?;

    if !output.status.success() {
        return Err("git log failed".into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut file_to_commit: HashMap<String, String> = HashMap::new();
    let mut commit_order: VecDeque<String> = VecDeque::new();
    let mut current_commit: Option<String> = None;

    for line in stdout.lines() {
        if line.is_empty() {
            continue;
        }
        if let Some(hash) = line.strip_prefix("COMMIT:") {
            current_commit = Some(hash.to_string());
            commit_order.push_back(hash.to_string());
        } else if let Some(commit) = &current_commit {
            file_to_commit.insert(line.to_string(), commit.clone());
        }
    }

    Ok((file_to_commit, commit_order.into_iter().collect()))
}

fn get_modified_files_for_commit(scan_path: &Path, commit_hash: &str) -> Option<Vec<String>> {
    let output = Command::new("git")
        .args([
            "diff",
            "--name-only",
            "--diff-filter=AM",
            &format!("{}^..{}", commit_hash, commit_hash),
        ])
        .current_dir(scan_path)
        .output()
        .ok()?;

    if !output.status.success() {
        return Some(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let files: Vec<String> = stdout.lines().map(|line| line.to_string()).collect();

    Some(files)
}
