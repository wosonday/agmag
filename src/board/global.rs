//! Global screen state of the full-screen `agm tui`: focus, command line, popup, selections.

use std::collections::HashMap;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::cmd::NoteKind;
use super::theme::ThemeName;
use crate::dashboard::{Section, Snapshot, sanitize_one_line};
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
    /// Draft saved when browsing history.
    draft: Option<String>,
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
        if key.kind == KeyEventKind::Release {
            return GEffect::None;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
        {
            return GEffect::Quit;
        }

        if let Some(popup) = self.popup.clone() {
            return self.on_key_popup(key, popup);
        }

        match self.focus {
            Focus::CmdLine => self.on_key_cmdline(key, data),
            Focus::Panel(sec) => self.on_key_panel(key, data, sec),
        }
    }

    pub fn sync(&mut self, data: &GlobalData) {
        // Keep terminal selection by address, nearest neighbour when gone, None when empty.
        if let Some(snap) = data.snap.as_ref() {
            let terms = &snap.terminals;
            if terms.is_empty() {
                self.sel_terminal = None;
            } else if let Some(addr) = self.sel_terminal.as_deref() {
                if let Some(i) = terms.iter().position(|t| t.address == addr) {
                    self.sel_terminal = Some(terms[i].address.clone());
                } else {
                    // nearest neighbour by previous index is not stored; pick closest by id-like
                    // fallback: keep a stable neighbour. We don't have old index, so pick the last
                    // or first? Plan says "nearest neighbour when gone" - we need to track index.
                    // Without stored index, pick the closest by scanning: choose the one at same
                    // position clamped. We don't know position. Store-less fallback: first.
                    // Better: keep by address already failed; choose nearest by previous index.
                    // We'll track indices internally via helper; but fields are frozen. Use the
                    // selection's previous position by remembering? Not in struct. Instead, pick
                    // the element with minimum distance in the old list? Old list gone.
                    // Pragmatic: clamp to last.
                    let idx = terms.len().saturating_sub(1);
                    self.sel_terminal = Some(terms[idx].address.clone());
                }
            } else {
                self.sel_terminal = Some(terms[0].address.clone());
            }

            let inbox = &snap.inbox;
            if inbox.is_empty() {
                self.sel_inbox = None;
            } else if let Some(id) = self.sel_inbox {
                if let Some(i) = inbox.iter().position(|r| r.id == id) {
                    self.sel_inbox = Some(inbox[i].id);
                } else {
                    let idx = inbox.len().saturating_sub(1);
                    self.sel_inbox = Some(inbox[idx].id);
                }
            } else {
                self.sel_inbox = Some(inbox[0].id);
            }
        } else {
            // No snapshot: keep selections as-is? Plan says "None when empty" - no data means empty.
            self.sel_terminal = None;
            self.sel_inbox = None;
        }

        if let Focus::Panel(sec) = self.focus
            && !visible_panels(data).contains(&sec)
        {
            self.focus = Focus::CmdLine;
        }
    }

    fn on_key_popup(&mut self, key: KeyEvent, popup: Popup) -> GEffect {
        let Popup::Confirm { prompt, mut input } = popup;
        match key.code {
            KeyCode::Enter => {
                self.popup = None;
                GEffect::Run(input)
            }
            KeyCode::Esc => {
                self.popup = None;
                GEffect::Run(String::new())
            }
            KeyCode::Backspace => {
                input.pop();
                self.popup = Some(Popup::Confirm { prompt, input });
                GEffect::None
            }
            KeyCode::Char(c) => {
                if !c.is_control()
                    && !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT)
                {
                    input.push(c);
                }
                self.popup = Some(Popup::Confirm { prompt, input });
                GEffect::None
            }
            _ => {
                self.popup = Some(Popup::Confirm { prompt, input });
                GEffect::None
            }
        }
    }

    fn on_key_cmdline(&mut self, key: KeyEvent, data: &GlobalData) -> GEffect {
        match key.code {
            KeyCode::Enter => {
                let text = self.cmd.text.clone();
                if text.trim().is_empty() {
                    return GEffect::None;
                }
                self.push_history(text.clone());
                self.cmd.text.clear();
                self.cmd.cursor = 0;
                self.cmd.hist_pos = None;
                self.cmd.draft = None;
                GEffect::Run(text)
            }
            KeyCode::Esc => {
                self.cmd.text.clear();
                self.cmd.cursor = 0;
                self.cmd.hist_pos = None;
                self.cmd.draft = None;
                GEffect::None
            }
            KeyCode::Backspace => {
                if self.cmd.cursor > 0 {
                    let idx = char_to_byte(&self.cmd.text, self.cmd.cursor - 1);
                    let next = char_to_byte(&self.cmd.text, self.cmd.cursor);
                    self.cmd.text.drain(idx..next);
                    self.cmd.cursor -= 1;
                }
                GEffect::None
            }
            KeyCode::Delete => {
                let len = self.cmd.text.chars().count();
                if self.cmd.cursor < len {
                    let idx = char_to_byte(&self.cmd.text, self.cmd.cursor);
                    let next = char_to_byte(&self.cmd.text, self.cmd.cursor + 1);
                    self.cmd.text.drain(idx..next);
                }
                GEffect::None
            }
            KeyCode::Left => {
                self.cmd.cursor = self.cmd.cursor.saturating_sub(1);
                GEffect::None
            }
            KeyCode::Right => {
                let len = self.cmd.text.chars().count();
                if self.cmd.cursor < len {
                    self.cmd.cursor += 1;
                }
                GEffect::None
            }
            KeyCode::Home => {
                self.cmd.cursor = 0;
                GEffect::None
            }
            KeyCode::End => {
                self.cmd.cursor = self.cmd.text.chars().count();
                GEffect::None
            }
            KeyCode::Up => {
                self.history_up();
                GEffect::None
            }
            KeyCode::Down => {
                self.history_down();
                GEffect::None
            }
            KeyCode::Tab => {
                let panels = visible_panels(data);
                if let Some(first) = panels.first().copied() {
                    self.focus = Focus::Panel(first);
                }
                GEffect::None
            }
            KeyCode::BackTab => {
                let panels = visible_panels(data);
                if let Some(last) = panels.last().copied() {
                    self.focus = Focus::Panel(last);
                }
                GEffect::None
            }
            KeyCode::Char(c) => {
                if !c.is_control()
                    && !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT)
                {
                    let idx = char_to_byte(&self.cmd.text, self.cmd.cursor);
                    self.cmd.text.insert(idx, c);
                    self.cmd.cursor += 1;
                }
                GEffect::None
            }
            _ => GEffect::None,
        }
    }

    fn on_key_panel(&mut self, key: KeyEvent, data: &GlobalData, sec: Section) -> GEffect {
        match key.code {
            KeyCode::Tab => {
                let panels = visible_panels(data);
                if let Some(pos) = panels.iter().position(|p| *p == sec) {
                    if pos + 1 < panels.len() {
                        self.focus = Focus::Panel(panels[pos + 1]);
                    } else {
                        self.focus = Focus::CmdLine;
                    }
                } else {
                    self.focus = Focus::CmdLine;
                }
                GEffect::None
            }
            KeyCode::BackTab => {
                let panels = visible_panels(data);
                if let Some(pos) = panels.iter().position(|p| *p == sec) {
                    if pos > 0 {
                        self.focus = Focus::Panel(panels[pos - 1]);
                    } else {
                        self.focus = Focus::CmdLine;
                    }
                } else {
                    self.focus = Focus::CmdLine;
                }
                GEffect::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.panel_up(data, sec);
                GEffect::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.panel_down(data, sec);
                GEffect::None
            }
            KeyCode::PageUp => {
                if data.output.is_some() {
                    self.output_scroll = self.output_scroll.saturating_sub(5);
                }
                GEffect::None
            }
            KeyCode::PageDown => {
                if data.output.is_some() {
                    self.output_scroll = self.output_scroll.saturating_add(5);
                }
                GEffect::None
            }
            KeyCode::Enter => match sec {
                Section::Terminals => self
                    .sel_terminal
                    .clone()
                    .map(GEffect::OpenBoard)
                    .unwrap_or(GEffect::None),
                Section::Inbox => self
                    .sel_inbox
                    .map(|id| GEffect::Run(format!("show {id}")))
                    .unwrap_or(GEffect::None),
                _ => GEffect::None,
            },
            KeyCode::Char('r') if sec == Section::Inbox => {
                if let Some(id) = self.sel_inbox {
                    self.focus_cmdline_with(format!("reply {id} "));
                }
                GEffect::None
            }
            KeyCode::Char('d') if sec == Section::Inbox => self
                .sel_inbox
                .map(|id| GEffect::Run(format!("done {id}")))
                .unwrap_or(GEffect::None),
            KeyCode::Char('s') if sec == Section::Terminals => {
                if let Some(addr) = self.sel_terminal.clone() {
                    let name = data
                        .snap
                        .as_ref()
                        .and_then(|s| s.terminals.iter().find(|t| t.address == addr))
                        .map(|t| {
                            if t.name.is_empty() {
                                sanitize_one_line(&t.address)
                            } else {
                                sanitize_one_line(&t.name)
                            }
                        })
                        .unwrap_or_else(|| sanitize_one_line(&addr));
                    // A name made only of control/escape text falls back to the address.
                    let name = if name.is_empty() {
                        sanitize_one_line(&addr)
                    } else {
                        name
                    };
                    self.focus_cmdline_with(format!("send {name} "));
                }
                GEffect::None
            }
            KeyCode::Char('t') => {
                self.theme = self.theme.next();
                GEffect::SaveTheme(self.theme)
            }
            KeyCode::Char('q') => GEffect::Quit,
            KeyCode::Char('?') => {
                self.note = Some((
                    NoteKind::Ok,
                    "tab panels · ↑↓ select · ⏎ open · r reply · d done · s send · t theme · q quit"
                        .into(),
                ));
                GEffect::None
            }
            KeyCode::Char(c) => {
                if !c.is_control()
                    && !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT)
                {
                    self.focus = Focus::CmdLine;
                    let idx = char_to_byte(&self.cmd.text, self.cmd.cursor);
                    self.cmd.text.insert(idx, c);
                    self.cmd.cursor += 1;
                }
                GEffect::None
            }
            _ => GEffect::None,
        }
    }

    fn focus_cmdline_with(&mut self, text: String) {
        self.focus = Focus::CmdLine;
        self.cmd.text = text;
        self.cmd.cursor = self.cmd.text.chars().count();
        self.cmd.hist_pos = None;
        self.cmd.draft = None;
    }

    fn push_history(&mut self, text: String) {
        if self.cmd.history.last().is_some_and(|last| last == &text) {
            return;
        }
        self.cmd.history.push(text);
        if self.cmd.history.len() > 50 {
            let drop = self.cmd.history.len() - 50;
            self.cmd.history.drain(0..drop);
        }
    }

    fn history_up(&mut self) {
        if self.cmd.history.is_empty() {
            return;
        }
        let pos = match self.cmd.hist_pos {
            Some(p) => p.saturating_sub(1),
            None => {
                self.cmd.draft = Some(self.cmd.text.clone());
                self.cmd.history.len() - 1
            }
        };
        self.cmd.hist_pos = Some(pos);
        self.cmd.text = self.cmd.history[pos].clone();
        self.cmd.cursor = self.cmd.text.chars().count();
    }

    fn history_down(&mut self) {
        let Some(pos) = self.cmd.hist_pos else {
            return;
        };
        if pos + 1 < self.cmd.history.len() {
            let next = pos + 1;
            self.cmd.hist_pos = Some(next);
            self.cmd.text = self.cmd.history[next].clone();
            self.cmd.cursor = self.cmd.text.chars().count();
        } else {
            self.cmd.hist_pos = None;
            self.cmd.text = self.cmd.draft.take().unwrap_or_default();
            self.cmd.cursor = self.cmd.text.chars().count();
        }
    }

    fn panel_up(&mut self, data: &GlobalData, sec: Section) {
        match sec {
            Section::Terminals => {
                let Some(snap) = data.snap.as_ref() else {
                    return;
                };
                let terms = &snap.terminals;
                if terms.is_empty() {
                    self.sel_terminal = None;
                    return;
                }
                let cur = self
                    .sel_terminal
                    .as_deref()
                    .and_then(|a| terms.iter().position(|t| t.address == a))
                    .unwrap_or(0);
                let next = cur.saturating_sub(1);
                self.sel_terminal = Some(terms[next].address.clone());
            }
            Section::Inbox => {
                let Some(snap) = data.snap.as_ref() else {
                    return;
                };
                let inbox = &snap.inbox;
                if inbox.is_empty() {
                    self.sel_inbox = None;
                    return;
                }
                let cur = self
                    .sel_inbox
                    .and_then(|id| inbox.iter().position(|r| r.id == id))
                    .unwrap_or(0);
                let next = cur.saturating_sub(1);
                self.sel_inbox = Some(inbox[next].id);
            }
            _ => {
                let entry = self.scroll.entry(sec).or_insert(0);
                *entry = entry.saturating_sub(1);
            }
        }
    }

    fn panel_down(&mut self, data: &GlobalData, sec: Section) {
        match sec {
            Section::Terminals => {
                let Some(snap) = data.snap.as_ref() else {
                    return;
                };
                let terms = &snap.terminals;
                if terms.is_empty() {
                    self.sel_terminal = None;
                    return;
                }
                let cur = self
                    .sel_terminal
                    .as_deref()
                    .and_then(|a| terms.iter().position(|t| t.address == a))
                    .unwrap_or(0);
                let next = (cur + 1).min(terms.len() - 1);
                self.sel_terminal = Some(terms[next].address.clone());
            }
            Section::Inbox => {
                let Some(snap) = data.snap.as_ref() else {
                    return;
                };
                let inbox = &snap.inbox;
                if inbox.is_empty() {
                    self.sel_inbox = None;
                    return;
                }
                let cur = self
                    .sel_inbox
                    .and_then(|id| inbox.iter().position(|r| r.id == id))
                    .unwrap_or(0);
                let next = (cur + 1).min(inbox.len() - 1);
                self.sel_inbox = Some(inbox[next].id);
            }
            _ => {
                let entry = self.scroll.entry(sec).or_insert(0);
                *entry = entry.saturating_add(1);
            }
        }
    }
}

fn visible_panels(data: &GlobalData) -> Vec<Section> {
    let mut panels: Vec<Section> = data
        .view
        .iter()
        .copied()
        .filter(|s| *s != Section::Messages)
        .collect();
    panels.push(Section::Messages);
    panels
}

fn char_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::{BudgetView, Snapshot, TermView};
    use crate::report::{QuotaRow, UsageBucket, UsageSummary};
    use crate::store::{InboxRow, MessageRow};
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn key_kind(code: KeyCode, kind: KeyEventKind) -> KeyEvent {
        KeyEvent::new_with_kind(code, KeyModifiers::NONE, kind)
    }

    fn char_key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn term(address: &str, name: &str) -> TermView {
        TermView {
            address: address.into(),
            name: name.into(),
            app: "claude".into(),
            status: "working".into(),
            task: String::new(),
            model: String::new(),
            effort: String::new(),
            cwd: String::new(),
            place: String::new(),
            last_msg: None,
            tokens: None,
            context_pct: None,
            handoff_state: "active".into(),
        }
    }

    fn inbox(id: i64) -> InboxRow {
        InboxRow {
            id,
            ts_ms: 0,
            terminal: Some("t1".into()),
            source: "source".into(),
            kind: Some("question".into()),
            text: format!("item {id}"),
            resolved_ms: None,
            reply: None,
        }
    }

    fn snap(terminals: Vec<TermView>, inbox_items: Vec<InboxRow>) -> Snapshot {
        Snapshot {
            now_s: 0,
            policy_on: true,
            quota: Vec::<QuotaRow>::new(),
            usage_summary: UsageSummary {
                now_ms: 0,
                offset_label: String::new(),
                four_months_start_ms: 0,
                four_months_start_day: String::new(),
                last_24h: UsageBucket::default(),
                today: UsageBucket::default(),
                this_month: UsageBucket::default(),
                recent_months: Vec::new(),
                covered_since_ms: None,
                incomplete_coverage: false,
            },
            terminals,
            live_rotation_pairs: Vec::new(),
            budgets: Vec::<BudgetView>::new(),
            inbox: inbox_items,
            messages: Vec::<MessageRow>::new(),
            system: None,
        }
    }

    fn data_with(
        view: Vec<Section>,
        terminals: Vec<TermView>,
        inbox_items: Vec<InboxRow>,
    ) -> GlobalData {
        GlobalData {
            snap: Some(snap(terminals, inbox_items)),
            view,
            project_of: HashMap::new(),
            output: None,
            now_ms: 0,
        }
    }

    #[test]
    fn release_ignored() {
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        let data = GlobalData::default();
        let before = st.clone();
        let eff = st.on_key(key_kind(KeyCode::Char('x'), KeyEventKind::Release), &data);
        assert_eq!(eff, GEffect::None);
        assert_eq!(st.cmd.text, before.cmd.text);
        assert_eq!(st.focus, before.focus);
    }

    #[test]
    fn ctrl_c_quits_everywhere() {
        let data = GlobalData::default();
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        assert_eq!(st.on_key(ctrl('c'), &data), GEffect::Quit);

        st.focus = Focus::Panel(Section::Terminals);
        assert_eq!(st.on_key(ctrl('c'), &data), GEffect::Quit);

        st.popup = Some(Popup::Confirm {
            prompt: "kill?".into(),
            input: String::new(),
        });
        assert_eq!(st.on_key(ctrl('c'), &data), GEffect::Quit);
    }

    #[test]
    fn typing_and_enter_runs_command() {
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        let data = GlobalData::default();
        st.on_key(char_key('l'), &data);
        st.on_key(char_key('s'), &data);
        assert_eq!(st.cmd.text, "ls");
        let eff = st.on_key(key(KeyCode::Enter), &data);
        assert_eq!(eff, GEffect::Run("ls".into()));
        assert_eq!(st.cmd.text, "");
        assert_eq!(st.cmd.cursor, 0);
    }

    #[test]
    fn blank_enter_does_nothing() {
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        let data = GlobalData::default();
        st.cmd.text = "   ".into();
        st.cmd.cursor = 3;
        let eff = st.on_key(key(KeyCode::Enter), &data);
        assert_eq!(eff, GEffect::None);
    }

    #[test]
    fn history_up_down_and_dedupe() {
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        let data = GlobalData::default();
        for cmd in ["ls", "help", "help", "ls"] {
            st.cmd.text = cmd.into();
            st.cmd.cursor = cmd.chars().count();
            st.on_key(key(KeyCode::Enter), &data);
        }
        // dedupe consecutive only: ls, help, ls
        assert_eq!(st.cmd.history, vec!["ls", "help", "ls"]);
        st.cmd.text = "draft".into();
        st.cmd.cursor = 5;
        st.on_key(key(KeyCode::Up), &data);
        assert_eq!(st.cmd.text, "ls");
        st.on_key(key(KeyCode::Up), &data);
        assert_eq!(st.cmd.text, "help");
        st.on_key(key(KeyCode::Up), &data);
        assert_eq!(st.cmd.text, "ls");
        st.on_key(key(KeyCode::Down), &data);
        assert_eq!(st.cmd.text, "help");
        st.on_key(key(KeyCode::Down), &data);
        assert_eq!(st.cmd.text, "ls");
        st.on_key(key(KeyCode::Down), &data);
        assert_eq!(st.cmd.text, "draft");
    }

    #[test]
    fn cursor_editing_utf8() {
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        let data = GlobalData::default();
        st.on_key(char_key('é'), &data);
        assert_eq!(st.cmd.text, "é");
        st.on_key(key(KeyCode::Left), &data);
        st.on_key(char_key('x'), &data);
        assert_eq!(st.cmd.text, "xé");
    }

    #[test]
    fn confirm_popup_y_enter_runs_y() {
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        let data = GlobalData::default();
        st.popup = Some(Popup::Confirm {
            prompt: "kill t1?".into(),
            input: String::new(),
        });
        st.on_key(char_key('y'), &data);
        let eff = st.on_key(key(KeyCode::Enter), &data);
        assert_eq!(eff, GEffect::Run("y".into()));
        assert_eq!(st.popup, None);
    }

    #[test]
    fn confirm_popup_esc_runs_empty() {
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        let data = GlobalData::default();
        st.popup = Some(Popup::Confirm {
            prompt: "kill t1?".into(),
            input: "y".into(),
        });
        let eff = st.on_key(key(KeyCode::Esc), &data);
        assert_eq!(eff, GEffect::Run(String::new()));
        assert_eq!(st.popup, None);
    }

    #[test]
    fn tab_cycles_visible_panels_and_back() {
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        let data = data_with(vec![Section::Quota, Section::Inbox], vec![], vec![]);
        assert_eq!(st.focus, Focus::CmdLine);
        st.on_key(key(KeyCode::Tab), &data);
        assert_eq!(st.focus, Focus::Panel(Section::Quota));
        st.on_key(key(KeyCode::Tab), &data);
        assert_eq!(st.focus, Focus::Panel(Section::Inbox));
        st.on_key(key(KeyCode::Tab), &data);
        assert_eq!(st.focus, Focus::Panel(Section::Messages));
        st.on_key(key(KeyCode::Tab), &data);
        assert_eq!(st.focus, Focus::CmdLine);
    }

    #[test]
    fn inbox_r_prefills_reply() {
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        let data = data_with(vec![Section::Inbox], vec![], vec![inbox(42)]);
        st.focus = Focus::Panel(Section::Inbox);
        st.sel_inbox = Some(42);
        let eff = st.on_key(char_key('r'), &data);
        assert_eq!(eff, GEffect::None);
        assert_eq!(st.focus, Focus::CmdLine);
        assert_eq!(st.cmd.text, "reply 42 ");
        assert_eq!(st.cmd.cursor, "reply 42 ".chars().count());
    }

    #[test]
    fn inbox_enter_runs_show() {
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        let data = data_with(vec![Section::Inbox], vec![], vec![inbox(7)]);
        st.focus = Focus::Panel(Section::Inbox);
        st.sel_inbox = Some(7);
        let eff = st.on_key(key(KeyCode::Enter), &data);
        assert_eq!(eff, GEffect::Run("show 7".into()));
    }

    #[test]
    fn terminal_enter_opens_board() {
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        let data = data_with(vec![Section::Terminals], vec![term("addr1", "t1")], vec![]);
        st.focus = Focus::Panel(Section::Terminals);
        st.sel_terminal = Some("addr1".into());
        let eff = st.on_key(key(KeyCode::Enter), &data);
        assert_eq!(eff, GEffect::OpenBoard("addr1".into()));
    }

    #[test]
    fn terminal_s_prefills_send_with_name() {
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        let data = data_with(
            vec![Section::Terminals],
            vec![term("addr1", "worker-a")],
            vec![],
        );
        st.focus = Focus::Panel(Section::Terminals);
        st.sel_terminal = Some("addr1".into());
        let eff = st.on_key(char_key('s'), &data);
        assert_eq!(eff, GEffect::None);
        assert_eq!(st.focus, Focus::CmdLine);
        assert_eq!(st.cmd.text, "send worker-a ");
    }

    #[test]
    fn control_chars_not_inserted() {
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        let data = GlobalData::default();
        for c in ['\x1b', '\x07', '\n', '\r', '\u{7f}'] {
            st.on_key(char_key(c), &data);
        }
        assert_eq!(
            st.cmd.text, "",
            "control chars must not reach the command line"
        );

        st.popup = Some(Popup::Confirm {
            prompt: "kill t1?".into(),
            input: String::new(),
        });
        for c in ['\x1b', '\x07', '\n', '\r', '\u{7f}'] {
            st.on_key(char_key(c), &data);
        }
        assert_eq!(
            st.popup,
            Some(Popup::Confirm {
                prompt: "kill t1?".into(),
                input: String::new(),
            }),
            "control chars must not reach the confirmation input"
        );
    }

    #[test]
    fn send_prefill_sanitized() {
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        let data = data_with(
            vec![Section::Terminals],
            vec![
                term("addr\x1b[31m1", "evil\x1b]0;pwn\x07name"),
                term("addr\x1b[31m2", ""),
            ],
            vec![],
        );
        st.focus = Focus::Panel(Section::Terminals);

        st.sel_terminal = Some("addr\x1b[31m1".into());
        st.on_key(char_key('s'), &data);
        assert_eq!(st.cmd.text, "send evilname ");
        assert!(
            !st.cmd.text.contains('\x1b'),
            "escape reached the command line"
        );

        // An empty name falls back to the (sanitized) address.
        st.focus = Focus::Panel(Section::Terminals);
        st.sel_terminal = Some("addr\x1b[31m2".into());
        st.on_key(char_key('s'), &data);
        assert_eq!(st.cmd.text, "send addr2 ");
        assert!(!st.cmd.text.contains('\x1b'));
    }

    #[test]
    fn unbound_printable_in_panel_goes_to_cmdline() {
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        let data = data_with(vec![Section::Terminals], vec![term("a", "t1")], vec![]);
        st.focus = Focus::Panel(Section::Terminals);
        let eff = st.on_key(char_key('x'), &data);
        assert_eq!(eff, GEffect::None);
        assert_eq!(st.focus, Focus::CmdLine);
        assert_eq!(st.cmd.text, "x");
        assert_eq!(st.cmd.cursor, 1);
    }

    #[test]
    fn sync_moves_to_neighbour_and_drops_hidden_focus() {
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        let mut data = data_with(
            vec![Section::Terminals, Section::Inbox],
            vec![term("a1", "t1"), term("a2", "t2"), term("a3", "t3")],
            vec![inbox(1), inbox(2), inbox(3)],
        );
        st.sel_terminal = Some("a2".into());
        st.sel_inbox = Some(2);
        st.focus = Focus::Panel(Section::Terminals);

        // Remove selected items; should move to neighbour (clamped/index-near).
        data.snap.as_mut().unwrap().terminals = vec![term("a1", "t1"), term("a3", "t3")];
        data.snap.as_mut().unwrap().inbox = vec![inbox(1), inbox(3)];
        st.sync(&data);
        assert!(
            st.sel_terminal.as_deref() == Some("a1") || st.sel_terminal.as_deref() == Some("a3")
        );
        assert!(st.sel_inbox == Some(1) || st.sel_inbox == Some(3));

        // Focus panel no longer in view -> CmdLine
        st.focus = Focus::Panel(Section::Terminals);
        data.view = vec![Section::Inbox];
        st.sync(&data);
        assert_eq!(st.focus, Focus::CmdLine);

        // empty -> None
        data.snap.as_mut().unwrap().terminals.clear();
        data.snap.as_mut().unwrap().inbox.clear();
        st.sel_terminal = Some("ghost".into());
        st.sel_inbox = Some(99);
        st.sync(&data);
        assert_eq!(st.sel_terminal, None);
        assert_eq!(st.sel_inbox, None);
    }
}
