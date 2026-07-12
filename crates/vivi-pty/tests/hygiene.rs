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

fn production_files() -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    collect_rust_files(Path::new("src"), &mut files);
    files
}

fn collect_rust_files(directory: &Path, files: &mut Vec<(PathBuf, String)>) {
    let entries = fs::read_dir(directory).unwrap();
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_rust_files(&path, files);
            continue;
        }
        let is_rust = path.extension().is_some_and(|extension| extension == "rs");
        let name = path.file_name().unwrap().to_string_lossy();
        if is_rust && !name.ends_with("_test.rs") && !name.ends_with(".test.rs") {
            files.push((path.clone(), fs::read_to_string(path).unwrap()));
        }
    }
}

fn count_lines(files: &[(PathBuf, String)], pattern: &str) -> usize {
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
        let count = count_lines(&files, pattern);
        assert!(
            count <= budget,
            "{pattern} budget exceeded: found {count}, max {budget}"
        );
    }
}

#[test]
fn production_files_have_no_inline_tests() {
    let files = production_files();
    let inline_modules = count_lines(&files, "mod tests {");
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
