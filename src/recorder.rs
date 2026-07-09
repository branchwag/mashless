//! Session state. The Lua shim forwards every keystroke (plus cursor and
//! buffer events) over msgpack-RPC; this module folds those events into the
//! session the analyzer later consumes. It is the Rust port of the old
//! `mashless.recorder` — with the crucial difference that key *capture* still
//! happens in Lua (`vim.on_key`), since that API has no RPC binding.

use std::collections::BTreeSet;

use chrono::{DateTime, Local};

/// Arrow keys, flagged both in normal mode and (separately) in insert mode.
pub const ARROWS: [&str; 4] = ["<Up>", "<Down>", "<Left>", "<Right>"];

pub fn is_arrow(tok: &str) -> bool {
    ARROWS.contains(&tok)
}

/// Tokens whose surrounding line text we keep, so the analyzer can suggest
/// `f`/`t`/search targets for horizontal motions.
pub fn keep_text(tok: &str) -> bool {
    matches!(
        tok,
        "l" | "h" | "<Right>" | "<Left>" | "w" | "b" | "e" | "W" | "B" | "E"
    )
}

/// Coarse mode bucket, derived from Neovim's mode string.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Normal,
    Visual,
    Insert,
    Other,
}

/// Reduce Neovim's mode string to a coarse bucket. Mirrors the Lua `classify`.
pub fn classify(mode: &str) -> Kind {
    match mode.chars().next() {
        Some('n') => Kind::Normal,
        // v, V, <C-v> (0x16), s, S, <C-s> (0x13)
        Some('v') | Some('V') | Some('\u{16}') | Some('s') | Some('S') | Some('\u{13}') => {
            Kind::Visual
        }
        Some('i') => Kind::Insert,
        _ => Kind::Other,
    }
}

/// One recorded normal/visual-mode keystroke, with the cursor position it
/// started from. `text` is the line under the cursor, kept only for the tokens
/// in [`keep_text`].
pub struct KeyEntry {
    pub tok: String,
    pub line: i64,
    pub col: i64,
    pub text: Option<String>,
}

/// Tunables, supplied by the Lua shim at setup time.
pub struct Config {
    pub output_dir: String,
    pub min_keys: i64,
    pub vmin: i64,
    pub hmin: i64,
    pub xmin: i64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            output_dir: ".".into(),
            min_keys: 20,
            vmin: 3,
            hmin: 5,
            xmin: 3,
        }
    }
}

/// Everything recorded for the current Neovim session.
pub struct Session {
    pub start: DateTime<Local>,
    /// Ordered stream of normal/visual keystrokes.
    pub keylog: Vec<KeyEntry>,
    /// Normal + visual keystrokes.
    pub total_keys: u64,
    /// Arrow keys pressed while in insert mode.
    pub insert_arrows: u64,
    /// Absolute paths of files visited this session.
    pub files: BTreeSet<String>,
    /// Most recent cursor position, kept fresh by CursorMoved.
    pub last_pos: (i64, i64),
}

impl Session {
    pub fn new() -> Self {
        Self {
            start: Local::now(),
            keylog: Vec::new(),
            total_keys: 0,
            insert_arrows: 0,
            files: BTreeSet::new(),
            last_pos: (1, 0),
        }
    }

    /// Fold one forwarded keystroke into the session. `mode` is Neovim's raw
    /// mode string at the moment the key was pressed; `text` is the current
    /// line (empty when the shim decided it wasn't worth sending).
    pub fn record_key(&mut self, tok: String, mode: &str, line: i64, col: i64, text: Option<String>) {
        match classify(mode) {
            Kind::Insert => {
                if is_arrow(&tok) {
                    self.insert_arrows += 1;
                }
            }
            Kind::Normal | Kind::Visual => {
                self.total_keys += 1;
                let text = if keep_text(&tok) {
                    text.filter(|t| !t.is_empty())
                } else {
                    None
                };
                self.keylog.push(KeyEntry {
                    tok,
                    line,
                    col,
                    text,
                });
            }
            Kind::Other => {}
        }
    }
}
