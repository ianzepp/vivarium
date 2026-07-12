use crate::protocol::{TerminalModes, TerminalSnapshot};

pub(crate) struct TerminalState {
    parser: vt100::Parser,
    screen_revision: u64,
    output_sequence: u64,
    scrollback_limit: usize,
}

impl TerminalState {
    pub(crate) fn new(rows: u16, columns: u16, scrollback_limit: usize) -> Self {
        Self {
            parser: vt100::Parser::new(rows, columns, scrollback_limit),
            screen_revision: 0,
            output_sequence: 0,
            scrollback_limit,
        }
    }

    pub(crate) fn process_output(&mut self, bytes: &[u8]) -> Option<(u64, u64)> {
        if bytes.is_empty() {
            return None;
        }
        self.output_sequence = self.output_sequence.saturating_add(1);
        self.parser.process(bytes);
        self.screen_revision = self.screen_revision.saturating_add(1);
        Some((self.screen_revision, self.output_sequence))
    }

    pub(crate) fn resize(&mut self, rows: u16, columns: u16) {
        self.parser.screen_mut().set_size(rows, columns);
    }

    pub(crate) fn snapshot(&self, session_id: &str) -> TerminalSnapshot {
        let screen = self.parser.screen();
        let (rows, columns) = screen.size();
        let (cursor_row, cursor_column) = screen.cursor_position();
        TerminalSnapshot {
            session_id: session_id.into(),
            columns,
            rows,
            cursor_column,
            cursor_row,
            contents: screen.contents(),
            formatted_contents: screen.contents_formatted(),
            scrollback: screen.scrollback(),
            scrollback_limit: self.scrollback_limit,
            modes: TerminalModes {
                alternate_screen: screen.alternate_screen(),
                application_keypad: screen.application_keypad(),
                application_cursor: screen.application_cursor(),
                cursor_hidden: screen.hide_cursor(),
                bracketed_paste: screen.bracketed_paste(),
                mouse_protocol: mouse_protocol_name(screen.mouse_protocol_mode()),
                mouse_encoding: mouse_encoding_name(screen.mouse_protocol_encoding()),
            },
            screen_revision: self.screen_revision,
            output_sequence: self.output_sequence,
        }
    }
}

fn mouse_protocol_name(mode: vt100::MouseProtocolMode) -> String {
    match mode {
        vt100::MouseProtocolMode::None => "none",
        vt100::MouseProtocolMode::Press => "press",
        vt100::MouseProtocolMode::PressRelease => "press_release",
        vt100::MouseProtocolMode::ButtonMotion => "button_motion",
        vt100::MouseProtocolMode::AnyMotion => "any_motion",
    }
    .into()
}

fn mouse_encoding_name(encoding: vt100::MouseProtocolEncoding) -> String {
    match encoding {
        vt100::MouseProtocolEncoding::Default => "default",
        vt100::MouseProtocolEncoding::Utf8 => "utf8",
        vt100::MouseProtocolEncoding::Sgr => "sgr",
    }
    .into()
}

#[cfg(test)]
#[path = "terminal_test.rs"]
mod tests;
