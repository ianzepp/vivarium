#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Cursor {
    pub column: u16,
    pub row: u16,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TerminalSnapshot {
    pub columns: u16,
    pub rows: u16,
    pub cursor: Cursor,
    pub visible_text: String,
    pub alternate_screen: bool,
}
