use super::*;

#[test]
fn expunge_requires_confirm_without_dry_run() {
    let inputs = vec!["abc123".to_string()];
    let err = validate_mutation_confirmation(
        &inputs,
        &|_| Ok(MutationAction::Expunge),
        MutationRunOptions {
            dry_run: false,
            json: false,
            confirm: false,
        },
    )
    .unwrap_err();

    assert!(err.to_string().contains("--confirm"));
}

#[test]
fn expunge_dry_run_does_not_require_confirm() {
    let inputs = vec!["abc123".to_string()];
    let result = validate_mutation_confirmation(
        &inputs,
        &|_| Ok(MutationAction::Expunge),
        MutationRunOptions {
            dry_run: true,
            json: true,
            confirm: false,
        },
    );

    assert!(result.is_ok());
}
