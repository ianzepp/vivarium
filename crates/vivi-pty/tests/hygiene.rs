#![allow(clippy::absurd_extreme_comparisons)]

use std::fs;
use std::path::{Path, PathBuf};

const MAX_UNWRAP: usize = 0;
const MAX_EXPECT: usize = 12;
const MAX_PANIC: usize = 0;
const MAX_UNREACHABLE: usize = 0;
const MAX_TODO: usize = 0;
const MAX_UNIMPLEMENTED: usize = 0;
const MAX_INLINE_TEST_MODULES: usize = 0;
const MAX_TEST_ATTR_IN_PRODUCTION: usize = 0;

// --- Size budget infrastructure ---

/// Ratchet budgets set to the worst-case production-code total at creation.
/// Numbers may only decrease; growth triggers a test failure.
struct Budgets {
    total_lines: usize,
    total_functions: usize,
    total_impls: usize,
    max_file_lines: usize,
    max_fn_lines: usize,
}

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

fn is_checked_rust_file(path: &Path) -> bool {
    path.is_file()
        && path.extension().is_some_and(|e| e == "rs")
        && path.file_name().is_none_or(|n| n != "lib.rs")
}

fn count_lines(content: &str) -> usize {
    content.lines().count()
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

fn count_impls(content: &str) -> usize {
    content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed.starts_with("impl ")
        })
        .count()
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
        // AGENTS.md ceiling is 1000 lines; no file currently exceeds it.
        max_file_lines: 1000,
        max_fn_lines: 120,
    }
}

// --- Banned pattern infrastructure ---

fn production_files() -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    collect_production_rust_files(Path::new("src"), &mut files);
    files
}

fn collect_production_rust_files(directory: &Path, files: &mut Vec<(PathBuf, String)>) {
    let entries = fs::read_dir(directory).expect("src directory should exist");
    for entry in entries {
        let path = entry.expect("read dir entry").path();
        if path.is_dir() {
            collect_production_rust_files(&path, files);
            continue;
        }
        let is_rust = path.extension().is_some_and(|extension| extension == "rs");
        let name = path.file_name().expect("file name").to_string_lossy();
        if is_rust && !name.ends_with("_test.rs") && !name.ends_with(".test.rs") {
            files.push((path.clone(), fs::read_to_string(path).expect("read file")));
        }
    }
}

fn count_pattern(files: &[(PathBuf, String)], pattern: &str) -> usize {
    files
        .iter()
        .map(|(_, content)| {
            content
                .lines()
                .filter(|line| line.contains(pattern))
                .count()
        })
        .sum()
}

// --- Size budget tests ---

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

// --- Banned pattern tests ---

#[test]
fn production_banned_pattern_budgets_hold() {
    let files = production_files();
    let budgets = [
        (".unwrap()", MAX_UNWRAP),
        (".expect(", MAX_EXPECT),
        ("panic!(", MAX_PANIC),
        ("unreachable!(", MAX_UNREACHABLE),
        ("todo!(", MAX_TODO),
        ("unimplemented!(", MAX_UNIMPLEMENTED),
    ];
    for (pattern, budget) in budgets {
        let count = count_pattern(&files, pattern);
        assert!(
            count <= budget,
            "{pattern} budget exceeded: found {count}, max {budget}"
        );
    }
}

#[test]
fn production_files_have_no_inline_tests() {
    let files = production_files();
    let inline_modules = count_pattern(&files, "mod tests {");
    let test_attributes = files
        .iter()
        .map(|(_, content)| {
            content
                .lines()
                .filter(|line| {
                    let line = line.trim();
                    line == "#[test]" || line == "#[tokio::test]" || line == "#[rstest]"
                })
                .count()
        })
        .sum::<usize>();
    assert!(
        inline_modules <= MAX_INLINE_TEST_MODULES,
        "inline test module budget exceeded: found {inline_modules}, max {MAX_INLINE_TEST_MODULES}"
    );
    assert!(
        test_attributes <= MAX_TEST_ATTR_IN_PRODUCTION,
        "production test attribute budget exceeded: found {test_attributes}, max {MAX_TEST_ATTR_IN_PRODUCTION}"
    );
}
