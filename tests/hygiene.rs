use std::fs;
use std::path::{Path, PathBuf};

fn src_files() -> Vec<String> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_src_files(&src, &mut files);
    files.sort();
    files
}

fn collect_src_files(dir: &Path, files: &mut Vec<String>) {
    for entry in fs::read_dir(dir).expect("src directory should exist") {
        let entry = entry.expect("read dir entry");
        let path = entry.path();
        if path.is_dir() {
            collect_src_files(&path, files);
            continue;
        }
        if !is_checked_rust_file(&path) {
            continue;
        }
        files.push(path.to_string_lossy().into_owned())
    }
}

fn is_checked_rust_file(path: &PathBuf) -> bool {
    path.is_file()
        && path.extension().is_some_and(|e| e == "rs")
        && path.file_name().is_none_or(|n| n != "lib.rs")
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

        let mut depth = 0i32;
        let mut fn_depth = 0i32;
        let mut fn_lines = 0;

        for line in &lines {
            let trimmed = line.trim();
            let is_fn = trimmed.starts_with("fn ")
                || trimmed.starts_with("pub fn ")
                || trimmed.starts_with("async fn ")
                || trimmed.starts_with("pub async fn ");

            if fn_depth == 0 && is_fn {
                fn_depth = depth + 1;
                fn_lines = 1;
            } else if fn_depth > 0 && is_fn {
                // Nested function — don't count, just track depth
            }

            if fn_depth > 0 {
                for ch in trimmed.chars() {
                    if ch == '{' {
                        depth += 1;
                    } else if ch == '}' {
                        depth -= 1;
                        if depth == fn_depth - 1 {
                            assert!(
                                fn_lines <= budgets.max_fn_lines,
                                "function has {} lines, budget is {}",
                                fn_lines,
                                budgets.max_fn_lines
                            );
                            fn_depth = 0;
                            fn_lines = 0;
                            break;
                        }
                    }
                }
                fn_lines += 1;
            } else if !is_fn {
                depth += trimmed.chars().filter(|&c| c == '{').count() as i32;
                depth -= trimmed.chars().filter(|&c| c == '}').count() as i32;
            }
        }
    }
}

#[test]
fn hygiene_total_lines_within_budget() {
    let budgets = budgets();
    let actual = src_files()
        .iter()
        .map(|f| count_lines(&fs::read_to_string(f).expect("read file")))
        .sum::<usize>();
    assert!(
        actual <= budgets.total_lines,
        "total lines {} equals budget {}",
        actual,
        budgets.total_lines
    );
}

#[test]
fn hygiene_total_functions_within_budget() {
    let budgets = budgets();
    let actual = src_files()
        .iter()
        .map(|f| count_functions(&fs::read_to_string(f).expect("read file")))
        .sum::<usize>();
    assert!(
        actual <= budgets.total_functions,
        "total functions {} equals budget {}",
        actual,
        budgets.total_functions
    );
}

#[test]
fn hygiene_total_impls_within_budget() {
    let budgets = budgets();
    let actual = src_files()
        .iter()
        .map(|f| count_impls(&fs::read_to_string(f).expect("read file")))
        .sum::<usize>();
    assert!(
        actual <= budgets.total_impls,
        "total impls {} equals budget {}",
        actual,
        budgets.total_impls
    );
}
