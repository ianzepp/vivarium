use super::*;

#[test]
fn scrollback_view_is_clamped_to_configured_limit() {
    let mut terminal = TerminalState::new(4, 20, 3);
    terminal.process_output(b"one\ntwo\nthree\nfour\nfive\nsix\n");
    terminal.parser.screen_mut().set_scrollback(usize::MAX);

    let snapshot = terminal.snapshot("scrollback-test");
    assert!(snapshot.scrollback <= 3);
    assert_eq!(snapshot.scrollback_limit, 3);
}
