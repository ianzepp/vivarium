use std::fs;
use std::path::Path;

fn src_files() -> Vec<String> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    for entry in fs::read_dir(&src).expect("src/ should exist") {
        let entry = entry.expect("read dir entry");
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().map_or(true, |e| e != "rs") {
            continue;
        }
        if path.file_name().map_or(false, |n| n == "lib.rs") {
            continue;
        }
        files.push(path.to_string_lossy().into_owned());
    }
    files.sort();
    files
}

fn count_functions(content: &str) -> usize {
    content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed.starts_with("fn ")
                || trimmed.starts_with("pub fn ")
                || trimmed.starts_with("async fn ")
                || trimmed.starts_with("pub async fn ")
        })
        .count()
}

fn count_lines(content: &str) -> usize {
    content.lines().count()
}

fn count_impls(content: &str) -> usize {
    content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed.starts_with("impl ")
        })
        .count()
}

/// Current hygiene budget — set to the worst-case production-code total at time of creation.
struct Budgets {
    total_lines: usize,
    total_functions: usize,
    total_impls: usize,
    max_file_lines: usize,
    max_fn_lines: usize,
}

fn budgets() -> Budgets {
    let mut total_lines = 0;
    let mut total_functions = 0;
    let mut total_impls = 0;

    for file in src_files() {
        let content = fs::read_to_string(&file).expect("read file");
        total_lines += count_lines(&content);
        total_functions += count_functions(&content);
        total_impls += count_impls(&content);
    }

    Budgets {
        total_lines,
        total_functions,
        total_impls,
        max_file_lines: 400,
        max_fn_lines: 60,
    }
}

#[test]
fn hygiene_no_file_over_line_limit() {
    let budgets = budgets();
    for file in src_files() {
        let content = fs::read_to_string(&file).expect("read file");
        let lines = count_lines(&content);
        assert!(
            lines <= budgets.max_file_lines,
            "{} has {} lines, budget is {}",
            file,
            lines,
            budgets.max_file_lines
        );
    }
}

#[test]
fn hygiene_no_function_over_line_limit() {
    let budgets = budgets();
    for file in src_files() {
        let content = fs::read_to_string(&file).expect("read file");
        let lines: Vec<&str> = content.lines().collect();

        let mut in_fn = false;
        let mut fn_lines = 0;

        for line in &lines {
            let trimmed = line.trim();
            if trimmed.starts_with("fn ")
                || trimmed.starts_with("pub fn ")
                || trimmed.starts_with("async fn ")
                || trimmed.starts_with("pub async fn ")
            {
                // Close previous function
                if in_fn && fn_lines > 0 {
                    assert!(
                        fn_lines <= budgets.max_fn_lines,
                        "{} function has {} lines, budget is {}",
                        "??? (unknown fn name)",
                        fn_lines,
                        budgets.max_fn_lines
                    );
                }
                in_fn = true;
                fn_lines = 1;
            } else if in_fn {
                fn_lines += 1;
            }
        }
    }
}

#[test]
fn hygiene_total_lines_within_budget() {
    let budgets = budgets();
    let actual = src_files().iter().map(|f| {
        count_lines(&fs::read_to_string(f).expect("read file"))
    }).sum::<usize>();
    assert!(
        actual <= budgets.total_lines,
        "total lines {} equals budget {}",
        actual,
        budgets.total_lines
    );
}
