use std::io;

pub trait PtySession: Send {
    fn write(&mut self, bytes: &[u8]) -> io::Result<()>;
    fn resize(&mut self, columns: u16, rows: u16) -> io::Result<()>;
    fn terminate(&mut self) -> io::Result<()>;
}

pub trait PtySupervisor: Send + Sync {
    type Session: PtySession;

    fn spawn(&self, program: &str, args: &[String], cwd: &str) -> io::Result<Self::Session>;
}
