use crate::protocol::{SessionState, TerminalSnapshot};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Classification {
    pub state: SessionState,
    pub evidence: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerminalAction {
    WriteText(String),
    WriteBytes(Vec<u8>),
    Key(String),
    WaitForState(SessionState),
    WaitForScreenSettle,
}

pub trait HarnessDriver: Send + Sync {
    fn name(&self) -> &'static str;
    fn classify(&self, terminal: &TerminalSnapshot) -> Classification;
    fn submit(&self, message: &str) -> Vec<TerminalAction>;
}
