use crate::driver::ActionQueue;
use crate::events::EventHub;
use crate::protocol::{
    SessionEventKind, SessionInfo, SessionState, StartSession, TerminalResize, TerminalSnapshot,
};
use crate::terminal::TerminalState;
use anyhow::{Context, Result, anyhow};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{FromRawFd, RawFd};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::{
    MAX_SCROLLBACK_ROWS, POLL_INTERVAL, SessionError, TERMINATION_GRACE, TERMINATION_SETTLE,
};
pub(super) struct ManagedSession {
    pub(super) info: SessionInfo,
    child: Box<dyn Child + Send + Sync>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    terminal: Arc<Mutex<TerminalState>>,
    pub(crate) process_group: libc::pid_t,
    pub(super) actions: ActionQueue,
    reader: Option<Box<dyn Read + Send>>,
    _drain: Option<OutputDrain>,
}

impl ManagedSession {
    pub(super) fn spawn(request: StartSession) -> Result<Self> {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: request.rows,
                cols: request.columns,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("open PTY")?;

        let mut command = CommandBuilder::new(&request.command[0]);
        command.args(&request.command[1..]);
        command.cwd(&request.cwd);
        let mut child = pair.slave.spawn_command(command).context("spawn command")?;
        let pid = child.process_id();
        let process_group = match pair.master.process_group_leader() {
            Some(process_group) => process_group,
            None => {
                abort_spawned_child(&mut child, None);
                return Err(anyhow!("PTY did not expose a process-group leader"));
            }
        };
        let reader = match duplicate_reader(pair.master.as_raw_fd()) {
            Ok(reader) => reader,
            Err(error) => {
                abort_spawned_child(&mut child, Some(process_group));
                return Err(error);
            }
        };
        let writer = match pair.master.take_writer() {
            Ok(writer) => writer,
            Err(error) => {
                abort_spawned_child(&mut child, Some(process_group));
                return Err(error);
            }
        };
        drop(pair.slave);

        let terminal = Arc::new(Mutex::new(TerminalState::new(
            request.rows,
            request.columns,
            MAX_SCROLLBACK_ROWS,
        )));
        Ok(Self {
            info: SessionInfo {
                session_id: request.session_id,
                driver: request.driver,
                command: request.command,
                cwd: request.cwd,
                pid,
                state: SessionState::Running,
                exit_code: None,
            },
            child,
            master: pair.master,
            writer,
            terminal,
            process_group,
            actions: ActionQueue::default(),
            reader: Some(reader),
            _drain: None,
        })
    }

    pub(super) fn start_output_drain(&mut self, events: Arc<EventHub>) -> Result<()> {
        let reader = self
            .reader
            .take()
            .ok_or_else(|| anyhow!("session output drain already started"))?;
        self._drain = Some(drain_output(
            reader,
            Arc::clone(&self.terminal),
            events,
            self.info.session_id.clone(),
        ));
        Ok(())
    }

    pub(super) fn refresh(&mut self) -> Result<bool> {
        if !matches!(self.info.state, SessionState::Running) {
            return Ok(false);
        }
        if let Some(status) = self.child.try_wait().context("inspect child")? {
            self.cleanup_process_group()?;
            self.info.state = SessionState::Exited;
            self.info.exit_code = Some(status.exit_code());
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) fn stop(&mut self) -> Result<bool> {
        self.refresh()?;
        if !matches!(self.info.state, SessionState::Running) {
            // The session was already exited; the caller still needs to record
            // a tombstone and release resources, so treat it as a transition.
            return Ok(true);
        }

        signal_process_group(self.process_group, libc::SIGTERM)
            .context("send SIGTERM to session process group")?;
        let deadline = Instant::now() + TERMINATION_GRACE;
        let mut status = None;
        while Instant::now() < deadline {
            if let Some(exit_status) = self.child.try_wait().context("inspect stopped child")? {
                status = Some(exit_status);
                break;
            }
            thread::sleep(POLL_INTERVAL);
        }
        if process_group_exists(self.process_group) {
            signal_process_group(self.process_group, libc::SIGKILL)
                .context("send SIGKILL to session process group")?;
        }
        let status = match status {
            Some(status) => status,
            None => self.child.wait().context("wait for stopped child")?,
        };
        wait_for_process_group_exit(self.process_group, TERMINATION_SETTLE);
        self.info.exit_code = Some(status.exit_code());
        self.info.state = SessionState::Stopped;
        Ok(true)
    }

    pub(super) fn write_running(&mut self, bytes: &[u8]) -> Result<usize, SessionError> {
        if !matches!(self.info.state, SessionState::Running) {
            return Err(SessionError::InvalidState(format!(
                "session is not running: {}",
                self.info.session_id
            )));
        }
        self.writer
            .write_all(bytes)
            .map_err(|error| SessionError::Internal(error.into()))?;
        self.writer
            .flush()
            .map_err(|error| SessionError::Internal(error.into()))?;
        Ok(bytes.len())
    }

    pub(super) fn resize(
        &mut self,
        request: TerminalResize,
    ) -> Result<TerminalSnapshot, SessionError> {
        if !matches!(self.info.state, SessionState::Running) {
            return Err(SessionError::InvalidState(format!(
                "session is not running: {}",
                self.info.session_id
            )));
        }
        self.master
            .resize(PtySize {
                rows: request.rows,
                cols: request.columns,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(SessionError::Internal)?;
        self.terminal
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .resize(request.rows, request.columns);
        Ok(self.snapshot())
    }

    #[cfg(test)]
    pub(super) fn pty_size(&self) -> Result<PtySize> {
        self.master.get_size().context("inspect PTY size")
    }

    pub(super) fn snapshot(&self) -> TerminalSnapshot {
        self.terminal
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .snapshot(&self.info.session_id)
    }

    fn cleanup_process_group(&mut self) -> Result<()> {
        if !process_group_exists(self.process_group) {
            return Ok(());
        }
        signal_process_group(self.process_group, libc::SIGTERM)
            .context("clean up exited session process group")?;
        let deadline = Instant::now() + TERMINATION_GRACE;
        while process_group_exists(self.process_group) && Instant::now() < deadline {
            thread::sleep(POLL_INTERVAL);
        }
        if process_group_exists(self.process_group) {
            signal_process_group(self.process_group, libc::SIGKILL)
                .context("force cleanup of exited session process group")?;
            wait_for_process_group_exit(self.process_group, TERMINATION_SETTLE);
        }
        Ok(())
    }

    pub(super) fn kill_group_best_effort(&mut self) {
        let _ = signal_process_group(self.process_group, libc::SIGKILL);
    }
}

impl Drop for ManagedSession {
    fn drop(&mut self) {
        if matches!(self.info.state, SessionState::Running)
            && let Err(error) = self.stop()
        {
            eprintln!(
                "failed to drop session {} cleanly: {error:#}",
                self.info.session_id
            );
            self.kill_group_best_effort();
        }
    }
}

struct OutputDrain {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Drop for OutputDrain {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn duplicate_reader(fd: Option<RawFd>) -> Result<Box<dyn Read + Send>> {
    let fd = fd.ok_or_else(|| anyhow!("PTY did not expose a readable file descriptor"))?;
    let duplicate = unsafe { libc::dup(fd) };
    if duplicate < 0 {
        return Err(io::Error::last_os_error()).context("duplicate PTY reader");
    }
    set_nonblocking(duplicate)?;
    let reader = unsafe { File::from_raw_fd(duplicate) };
    Ok(Box::new(reader))
}

fn abort_spawned_child(
    child: &mut Box<dyn Child + Send + Sync>,
    process_group: Option<libc::pid_t>,
) {
    if let Some(process_group) = process_group {
        let _ = signal_process_group(process_group, libc::SIGKILL);
    } else {
        let _ = child.kill();
    }
    let _ = child.wait();
}

fn set_nonblocking(fd: RawFd) -> Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(io::Error::last_os_error()).context("read PTY flags");
    }
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error()).context("set PTY reader nonblocking");
    }
    Ok(())
}

fn drain_output(
    mut reader: Box<dyn Read + Send>,
    terminal: Arc<Mutex<TerminalState>>,
    events: Arc<EventHub>,
    session_id: String,
) -> OutputDrain {
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let handle = thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        while !thread_stop.load(Ordering::Acquire) {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    let revision = terminal
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .process_output(&buffer[..count]);
                    if let Some((screen_revision, output_sequence)) = revision {
                        events.publish(
                            session_id.clone(),
                            SessionEventKind::Screen {
                                screen_revision,
                                output_sequence,
                            },
                        );
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(POLL_INTERVAL);
                }
                Err(_) => break,
            }
        }
    });
    OutputDrain {
        stop,
        handle: Some(handle),
    }
}

fn signal_process_group(process_group: libc::pid_t, signal: libc::c_int) -> io::Result<()> {
    if process_group <= 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid process-group identifier",
        ));
    }
    let result = unsafe { libc::kill(-process_group, signal) };
    if result == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        Ok(())
    } else {
        Err(error)
    }
}

pub(crate) fn process_group_exists(process_group: libc::pid_t) -> bool {
    if process_group <= 0 {
        return false;
    }
    let result = unsafe { libc::kill(-process_group, 0) };
    result == 0 || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

fn wait_for_process_group_exit(process_group: libc::pid_t, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while process_group_exists(process_group) && Instant::now() < deadline {
        thread::sleep(POLL_INTERVAL);
    }
}
