use crate::protocol::{Request, Response, read_frame, write_frame};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::os::unix::net::UnixStream;
use std::path::Path;

pub fn call(socket: &Path, method: &str, params: Value) -> Result<Value> {
    let mut stream = UnixStream::connect(socket)
        .with_context(|| format!("connect to daemon at {}", socket.display()))?;
    write_frame(&mut stream, &Request::new(1, method, params))?;
    let response: Response = read_frame(&mut stream)?;
    if let Some(error) = response.error {
        bail!("{} ({})", error.message, error.code);
    }
    response
        .result
        .context("daemon returned neither result nor error")
}
