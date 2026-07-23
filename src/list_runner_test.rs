use super::*;

#[test]
fn broken_pipe_output_is_not_an_error() {
    let result = handle_output_result(Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed")));

    assert!(result.is_ok());
}

#[test]
fn other_output_errors_are_reported() {
    let result = handle_output_result(Err(io::Error::other("disk sad")));

    assert!(result.is_err());
}
