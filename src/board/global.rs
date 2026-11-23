//! Global screen state of the full-screen `agm tui`: focus, command line, popup, selections.

use std::collections::HashMap;

use ratatui::crossterm::event::KeyEvent;

use super::cmd::NoteKind;
use super::theme::ThemeName;
use crate::dashboard::{Section, Snapshot};
use crate::project::ProjectKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    CmdLine,
    /// A section panel (`Section::Messages` for the pinned messages panel).
    Panel(Section),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CmdLine {
    pub text: String,
    /// Cursor position in chars.
    pub cursor: usize,
    /// Last 50 commands, oldest first.
    pub history: Vec<String>,
    pub hist_pos: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Popup {
    Confirm { prompt: String, input: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum GEffect {
    None,
    /// A command line for the dispatcher.
    Run(String),
    /// Open the board of this terminal's project (terminal address).
    OpenBoard(String),
    SaveTheme(ThemeName),
    Quit,
}

#[derive(Debug, Clone, Default)]
pub struct GlobalData {
    pub snap: Option<Snapshot>,
    pub view: Vec<Section>,
    /// Terminal address → project.
    pub project_of: HashMap<String, ProjectKey>,
    /// `Outcome::Shown` text, shown in the Output panel until the next command or `esc`.
    pub output: Option<String>,
    pub now_ms: i64,
}

#[derive(Debug, Clone)]
pub struct GlobalState {
    pub focus: Focus,
    pub cmd: CmdLine,
    pub popup: Option<Popup>,
    /// Address of the selected terminal row.
    pub sel_terminal: Option<String>,
    /// Id of the selected inbox row.
    pub sel_inbox: Option<i64>,
    pub scroll: HashMap<Section, u16>,
    pub output_scroll: u16,
    pub theme: ThemeName,
    pub note: Option<(NoteKind, String)>,
}

impl GlobalState {
    pub fn new(theme: ThemeName) -> Self {
        Self {
            focus: Focus::CmdLine,
            cmd: CmdLine::default(),
            popup: None,
            sel_terminal: None,
            sel_inbox: None,
            scroll: HashMap::new(),
            output_scroll: 0,
            theme,
            note: None,
        }
    }

    pub fn on_key(&mut self, key: KeyEvent, data: &GlobalData) -> GEffect {
        let _ = (key, data);
        todo!("Task C")
    }

    pub fn sync(&mut self, data: &GlobalData) {
        let _ = data;
        todo!("Task C")
    }
}
