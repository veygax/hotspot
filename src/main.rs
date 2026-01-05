use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Serialize, Clone)]
struct TreemapNode {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    children: Option<Vec<TreemapNode>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
}

fn build_treemap(files: &[(String, usize, String)]) -> TreemapNode {
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct BuildNode {
        name: String,
        value: usize,
        first_commit: Option<String>,
        path: Option<String>,
        is_file: bool,
        children: BTreeMap<String, BuildNode>,
    }

    let mut root = BuildNode {
        name: "root".to_string(),
        ..Default::default()
    };

    for (path, count, commit) in files {
        let parts: Vec<&str> = path.split('/').collect();

        let mut current = &mut root;
        for (i, part) in parts.iter().enumerate() {
            let is_file = i == parts.len() - 1;
            current = current.children.entry(part.to_string()).or_insert_with(|| BuildNode {
                name: part.to_string(),
                is_file,
                ..Default::default()
            });
        }

        current.value = *count;
        current.first_commit = Some(commit.clone());
        current.path = Some(path.clone());
        current.is_file = true;
    }

    fn convert_node(node: &BuildNode) -> TreemapNode {
        let children: Vec<TreemapNode> = node
            .children
            .values()
            .map(convert_node)
            .collect();

        if children.is_empty() {
            TreemapNode {
                name: node.name.clone(),
                children: None,
                value: Some(node.value),
                first_commit: node.first_commit.clone(),
                path: node.path.clone(),
            }
        } else {
            let total_value: usize = children.iter().filter_map(|c| c.value).sum();
            TreemapNode {
                name: node.name.clone(),
                children: Some(children),
                value: Some(total_value),
                first_commit: None,
                path: None,
            }
        }
    }

    let children: Vec<TreemapNode> = root
        .children
        .values()
        .map(convert_node)
        .collect();

    TreemapNode {
        name: "root".to_string(),
        children: Some(children),
        value: None,
        first_commit: None,
        path: None,
    }
}

fn generate_html(treemap_data: &TreemapNode, max_value: usize, min_value: usize) -> String {
    let json_data = serde_json::to_string(treemap_data).unwrap();

    format!(r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Hotspot</title>
    <script src="https://cdn.jsdelivr.net/npm/echarts@5.4.3/dist/echarts.min.js"></script>
    <style>
        body {{
            margin: 0;
            padding: 0;
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
            background: #f5f5f5;
        }}
        #container {{
            width: 100vw;
            height: 100vh;
        }}
        #info {{
            position: absolute;
            top: 10px;
            left: 10px;
            background: rgba(255, 255, 255, 0.9);
            padding: 8px 12px;
            border-radius: 6px;
            z-index: 1000;
            font-size: 12px;
        }}
        #info h1 {{
            margin: 0 0 2px 0;
            font-size: 14px;
            font-weight: 600;
            color: #333;
        }}
        #info .credit {{
            margin: 0 0 6px 0;
            font-size: 11px;
            color: #888;
        }}
        #info .credit a {{
            color: #3b82f6;
            text-decoration: none;
        }}
        #info p {{
            margin: 2px 0;
            color: #666;
        }}
        #info .legend {{
            display: flex;
            align-items: center;
            gap: 8px;
        }}
        #info .label {{
            font-size: 11px;
            color: #666;
        }}
        #info input[type="range"] {{
            flex: 1;
            width: 100px;
            cursor: pointer;
        }}
        #info .slider-value {{
            font-size: 11px;
            color: #333;
            min-width: 35px;
        }}
    </style>
</head>
<body>
    <div id="info">
        <h1>Hotspot</h1>
        <p class="credit">by <a href="https://veygax.dev" target="_blank">veygax</a></p>
        <div class="legend">
            <span class="label">Show:</span>
            <input type="range" id="minSlider" min="1" max="100" value="1" step="1">
            <span class="slider-value" id="minValue">1%</span>
        </div>
    </div>
    <div id="container"></div>
    <script>
        const data = {json_data};
        const chart = echarts.init(document.getElementById('container'));
        const maxValue = {max_value};
        const minValue = {min_value};

        function createOption(percentage) {{
            const visibleMin = Math.max(minValue, Math.floor(maxValue - (maxValue * percentage / 100)));
            return {{
                visualMap: {{
                    min: minValue,
                    max: maxValue,
                    inRange: {{
                        color: ['#22c55e', '#f97316', '#ef4444']
                    }},
                    show: false
                }},
                tooltip: {{
                    trigger: 'item',
                    formatter: function(params) {{
                        return '<div style="padding: 4px;">' +
                            '<strong>' + params.name + '</strong><br/>' +
                            '<span style="color: #666;">Modifications: ' + params.value + '</span><br/>' +
                            '<span style="color: #999; font-size: 11px;">First commit: ' + (params.data.firstCommit || 'N/A') + '</span>' +
                            (params.data.path ? '<br/><span style="color: #999; font-size: 11px;">' + params.data.path + '</span>' : '') +
                            '</div>';
                    }}
                }},
                series: [{{
                    type: 'treemap',
                    visibleMin: visibleMin,
                    animation: false,
                    label: {{
                        show: true,
                        formatter: '{{b}}',
                        fontSize: 13,
                        color: '#fff'
                    }},
                    itemStyle: {{
                        gapWidth: 2,
                        borderColor: '#fff',
                        borderRadius: 2
                    }},
                    upperLabel: {{
                        show: true,
                        height: 24,
                        formatter: '{{b}}',
                        fontSize: 14,
                        color: '#333',
                        backgroundColor: 'rgba(255,255,255,0.9)',
                        borderRadius: 2
                    }},
                    data: [data],
                    breadcrumb: {{
                        show: true,
                        height: 28,
                        itemStyle: {{
                            fontSize: 12,
                            color: '#333'
                        }}
                    }}
                }}]
            }};
        }}

        chart.setOption(createOption(1));

        const minSlider = document.getElementById('minSlider');
        const minValueDisplay = document.getElementById('minValue');

        minSlider.addEventListener('input', function() {{
            const value = parseInt(this.value);
            minValueDisplay.textContent = value + '%';
            chart.setOption(createOption(value));
        }});

        window.addEventListener('resize', () => chart.resize());
    </script>
</body>
</html>"#)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let mut path_arg = ".";
    let mut limit: Option<usize> = None;
    let mut html_output: Option<String> = None;

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
            "-h" | "--html" => {
                if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                    html_output = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    html_output = Some("hotspot.html".to_string());
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

    let git_root = get_git_root(&scan_path_canonical)?;
    let relative_scan_path = scan_path_canonical
        .strip_prefix(&git_root)
        .unwrap_or(&scan_path_canonical);
    let relevant_commits = get_commits_touching_path(&git_root, relative_scan_path)?;

    scan_spinner.finish_with_message(format!("Found {} relevant commits", relevant_commits.len()));

    if relevant_commits.is_empty() {
        println!("No commits found touching this path.");
        return Ok(());
    }

    println!("Finding modified files...");

    let progress = ProgressBar::new(relevant_commits.len() as u64);
    progress.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:20.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("=>-"),
    );
    progress.set_message("Processing commits...");

    let commit_to_modified_files: HashMap<String, Vec<String>> = relevant_commits
        .par_iter()
        .progress_with(progress.clone())
        .filter_map(|commit_hash| {
            get_modified_files_for_commit(&git_root, commit_hash)
                .map(|files| (commit_hash.clone(), files))
        })
        .collect();

    progress.finish_with_message("Analysis complete!");

    println!("\nAggregating results...");

    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut file_commits: HashMap<String, String> = HashMap::new();

    for commit_hash in &relevant_commits {
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
        .filter_map(|(file, count)| {
            let full_path = git_root.join(&file);
            if full_path.exists() {
                let short_hash = file_commits
                    .get(&file)
                    .map(|h| &h[..8])
                    .unwrap_or("--------");
                Some((file, count, short_hash.to_string()))
            } else {
                None
            }
        })
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1));

    let take_count = limit.unwrap_or(usize::MAX);
    let ranked: Vec<(String, usize, String)> = ranked.into_iter().take(take_count).collect();

    if let Some(output_path) = html_output {
        let max_val = ranked.first().map(|(_, c, _)| *c).unwrap_or(0);
        let min_val = ranked.last().map(|(_, c, _)| *c).unwrap_or(0);
        let treemap = build_treemap(&ranked);
        let html = generate_html(&treemap, max_val, min_val);
        fs::write(&output_path, html)?;
        println!("HTML report written to: {}", output_path);
        return Ok(());
    }

    let max_count = ranked.first().map(|(_, c, _)| *c).unwrap_or(0);
    let bar_width = 30;

    println!(
        "{:<6} | {:<9} | {:<50} | {}",
        "Count", "Modified", "File", "Hotness"
    );
    println!("{}", "-".repeat(120));

    for (file, count_val, short_hash) in &ranked {
        let pct = if max_count > 0 {
            (*count_val as f64 / max_count as f64) * 100.0
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

fn get_commits_touching_path(
    git_root: &Path,
    path: &Path,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args([
            "log",
            "--all",
            "--format=%H",
            "--diff-filter=A",
            "--",
            &path.to_string_lossy(),
        ])
        .current_dir(git_root)
        .output()?;

    if !output.status.success() {
        return Err("git log failed".into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let commits: Vec<String> = stdout.lines().map(|line| line.to_string()).collect();

    Ok(commits)
}

fn get_modified_files_for_commit(git_root: &Path, commit_hash: &str) -> Option<Vec<String>> {
    let output = Command::new("git")
        .args([
            "diff",
            "--name-only",
            "--diff-filter=AM",
            &format!("{}^..{}", commit_hash, commit_hash),
        ])
        .current_dir(git_root)
        .output()
        .ok()?;

    if !output.status.success() {
        return Some(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let files: Vec<String> = stdout.lines().map(|line| line.to_string()).collect();

    Some(files)
}

fn get_git_root(scan_path: &Path) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(scan_path)
        .output()?;

    if !output.status.success() {
        return Err("git rev-parse failed".into());
    }

    let git_root = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(PathBuf::from(git_root))
}
