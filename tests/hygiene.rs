//! Production-source hygiene ratchet.
//!
//! Scans `src/**/*.rs` excluding dedicated test companions. Budgets are fixed
//! at current production-only totals and may only go down.

#![allow(clippy::absurd_extreme_comparisons)]

use std::fs;
use std::path::Path;

// Size budgets (production only).
const MAX_TOTAL_LINES: usize = 32_524;
const MAX_TOTAL_FUNCTIONS: usize = 1_178;
const MAX_TOTAL_IMPLS: usize = 106;
const MAX_FILE_LINES: usize = 1_000;
const MAX_FN_LINES: usize = 60;

// Banned-pattern budgets (production only). Monotonic — lower, never raise.
const MAX_UNWRAP: usize = 8;
const MAX_EXPECT: usize = 1;
const MAX_PANIC: usize = 0;
const MAX_UNREACHABLE: usize = 14;
const MAX_TODO: usize = 0;
const MAX_UNIMPLEMENTED: usize = 0;
const MAX_LET_UNDERSCORE: usize = 23;
const MAX_OK_DROP: usize = 34;

// Structural test-boundary budgets.
const MAX_INLINE_TEST_MODULES: usize = 0;
const MAX_TEST_ATTR_IN_PRODUCTION: usize = 0;

struct SourceFile {
    path: String,
    content: String,
}

fn production_files() -> Vec<SourceFile> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_production_files(&src, &mut files);
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files
}

fn collect_production_files(dir: &Path, files: &mut Vec<SourceFile>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_production_files(&path, files);
            continue;
        }
        if !is_production_rust_file(&path) {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        files.push(SourceFile {
            path: path.to_string_lossy().into_owned(),
            content,
        });
    }
}

fn is_production_rust_file(path: &Path) -> bool {
    if !path.is_file() || path.extension().is_none_or(|e| e != "rs") {
        return false;
    }
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if name == "lib.rs" {
        return false;
    }
    if name.ends_with("_test.rs") || name.ends_with(".test.rs") {
        return false;
    }
    if name == "tests.rs" || name.ends_with("_tests.rs") {
        return false;
    }
    // Known dedicated test module filenames (not `*_test.rs` companions).
    if matches!(
        name,
        "filter_tests.rs" | "option_tests.rs" | "send_tests.rs"
    ) {
        return false;
    }
    true
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
        .filter(|line| line.trim().starts_with("impl "))
        .count()
}

fn count_pattern(files: &[SourceFile], pattern: &str) -> usize {
    files
        .iter()
        .map(|file| {
            file.content
                .lines()
                .filter(|line| line.contains(pattern))
                .count()
        })
        .sum()
}

fn assert_budget(name: &str, count: usize, max: usize) {
    assert!(
        count <= max,
        "{name} budget exceeded: found {count}, max {max}"
    );
}

#[test]
fn hygiene_no_file_over_line_limit() {
    for file in production_files() {
        let lines = count_lines(&file.content);
        assert!(
            lines <= MAX_FILE_LINES,
            "{} has {} lines, budget is {}",
            file.path,
            lines,
            MAX_FILE_LINES
        );
    }
}

#[test]
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
fn hygiene_no_function_over_line_limit() {
    for file in production_files() {
        let lines: Vec<&str> = file.content.lines().collect();
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
            }

            if fn_depth > 0 {
                for ch in trimmed.chars() {
                    if ch == '{' {
                        depth += 1;
                    } else if ch == '}' {
                        depth -= 1;
                        if depth == fn_depth - 1 {
                            assert!(
                                fn_lines <= MAX_FN_LINES,
                                "{}: function has {} lines, budget is {}",
                                file.path,
                                fn_lines,
                                MAX_FN_LINES
                            );
                            fn_depth = 0;
                            fn_lines = 0;
                            break;
                        }
                    }
                }
                if fn_depth > 0 {
                    fn_lines += 1;
                }
            } else if !is_fn {
                depth += trimmed.chars().filter(|&c| c == '{').count() as i32;
                depth -= trimmed.chars().filter(|&c| c == '}').count() as i32;
            }
        }
    }
}

#[test]
fn hygiene_total_lines_within_budget() {
    let actual: usize = production_files()
        .iter()
        .map(|f| count_lines(&f.content))
        .sum();
    assert_budget("total lines", actual, MAX_TOTAL_LINES);
}

#[test]
fn hygiene_total_functions_within_budget() {
    let actual: usize = production_files()
        .iter()
        .map(|f| count_functions(&f.content))
        .sum();
    assert_budget("total functions", actual, MAX_TOTAL_FUNCTIONS);
}

#[test]
fn hygiene_total_impls_within_budget() {
    let actual: usize = production_files()
        .iter()
        .map(|f| count_impls(&f.content))
        .sum();
    assert_budget("total impls", actual, MAX_TOTAL_IMPLS);
}

#[test]
fn hygiene_unwrap_budget() {
    assert_budget(
        ".unwrap()",
        count_pattern(&production_files(), ".unwrap()"),
        MAX_UNWRAP,
    );
}

#[test]
fn hygiene_expect_budget() {
    assert_budget(
        ".expect(",
        count_pattern(&production_files(), ".expect("),
        MAX_EXPECT,
    );
}

#[test]
fn hygiene_panic_budget() {
    assert_budget(
        "panic!(",
        count_pattern(&production_files(), "panic!("),
        MAX_PANIC,
    );
}

#[test]
fn hygiene_unreachable_budget() {
    assert_budget(
        "unreachable!(",
        count_pattern(&production_files(), "unreachable!("),
        MAX_UNREACHABLE,
    );
}

#[test]
fn hygiene_todo_budget() {
    assert_budget(
        "todo!(",
        count_pattern(&production_files(), "todo!("),
        MAX_TODO,
    );
}

#[test]
fn hygiene_unimplemented_budget() {
    assert_budget(
        "unimplemented!(",
        count_pattern(&production_files(), "unimplemented!("),
        MAX_UNIMPLEMENTED,
    );
}

#[test]
fn hygiene_let_underscore_budget() {
    assert_budget(
        "let _ =",
        count_pattern(&production_files(), "let _ ="),
        MAX_LET_UNDERSCORE,
    );
}

#[test]
fn hygiene_ok_drop_budget() {
    assert_budget(
        ".ok()",
        count_pattern(&production_files(), ".ok()"),
        MAX_OK_DROP,
    );
}

#[test]
fn hygiene_no_inline_test_modules() {
    let mut hits = Vec::new();
    for file in production_files() {
        let lines: Vec<&str> = file.content.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if !trimmed.starts_with("mod ") || !trimmed.contains('{') {
                continue;
            }
            // Look back a few lines for #[cfg(test)].
            let window_start = idx.saturating_sub(3);
            let prelude = lines[window_start..=idx].join("\n");
            if prelude.contains("#[cfg(test)]") {
                hits.push(format!(
                    "{}:{}: inline cfg(test) mod body",
                    file.path,
                    idx + 1
                ));
            }
        }
    }
    assert!(
        hits.len() <= MAX_INLINE_TEST_MODULES,
        "inline test modules budget exceeded: found {}, max {}\n{}",
        hits.len(),
        MAX_INLINE_TEST_MODULES,
        hits.join("\n")
    );
}

#[test]
fn hygiene_no_test_attrs_in_production() {
    let mut hits = Vec::new();
    for file in production_files() {
        for (idx, line) in file.content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("#[test]")
                || trimmed.starts_with("#[tokio::test]")
                || trimmed.starts_with("#[rstest]")
            {
                hits.push(format!("{}:{}: {trimmed}", file.path, idx + 1));
            }
        }
    }
    assert!(
        hits.len() <= MAX_TEST_ATTR_IN_PRODUCTION,
        "test attributes in production budget exceeded: found {}, max {}\n{}",
        hits.len(),
        MAX_TEST_ATTR_IN_PRODUCTION,
        hits.join("\n")
    );
}
