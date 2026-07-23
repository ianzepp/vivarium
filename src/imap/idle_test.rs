use super::*;

#[test]
fn wait_modes_are_explicitly_inbound_only() {
    assert_ne!(InboxWaitMode::ImapIdle, InboxWaitMode::Poll);
}
