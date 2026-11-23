//! Board state and key handling: `on_key` turns a key into an `Effect` without any I/O.

use ratatui::crossterm::event::KeyEvent;

use super::theme::ThemeName;
use super::{BoardData, Note};
use crate::project::{AgentView, ProjectKey};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Projects,
    Orchestration,
    Detail,
    Inbox,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Normal,
    Reply {
        id: i64,
        input: String,
    },
    Send {
        to: String,
        /// Index into `actions::KINDS_EN`.
        kind: usize,
        input: String,
    },
    ConfirmDone {
        id: i64,
    },
    ShowItem {
        id: i64,
    },
    Filter {
        input: String,
    },
    PickProject {
        idx: usize,
    },
    Help,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    None,
    Reply {
        id: i64,
        text: String,
    },
    Done {
        id: i64,
    },
    Send {
        to: String,
        /// Stored code, e.g. `GIAO`.
        kind: String,
        text: String,
    },
    SwitchProject(ProjectKey),
    SaveTheme(ThemeName),
    Quit,
}

#[derive(Debug, Clone)]
pub struct BoardState {
    pub panel: Panel,
    pub mode: Mode,
    pub project: Option<ProjectKey>,
    pub sel_project: usize,
    /// Address of the selected agent.
    pub sel_agent: Option<String>,
    /// Id of the selected inbox item.
    pub sel_inbox: Option<i64>,
    pub msg_scroll: usize,
    pub filter: String,
    pub theme: ThemeName,
    pub note: Option<Note>,
}

impl BoardState {
    pub fn new(project: Option<ProjectKey>, theme: ThemeName) -> Self {
        Self {
            panel: if project.is_some() {
                Panel::Orchestration
            } else {
                Panel::Projects
            },
            mode: Mode::Normal,
            project,
            sel_project: 0,
            sel_agent: None,
            sel_inbox: None,
            msg_scroll: 0,
            filter: String::new(),
            theme,
            note: None,
        }
    }

    pub fn on_key(&mut self, key: KeyEvent, data: &BoardData) -> Effect {
        let _ = (key, data);
        todo!("Task 2")
    }

    pub fn sync(&mut self, data: &BoardData) {
        let _ = data;
        todo!("Task 2")
    }

    pub fn selected_agent<'a>(&self, data: &'a BoardData) -> Option<&'a AgentView> {
        let address = self.sel_agent.as_deref()?;
        data.view
            .as_ref()?
            .agents
            .iter()
            .find(|a| a.address == address)
    }
}
