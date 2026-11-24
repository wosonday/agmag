//! Board state and key handling: `on_key` turns a key into an `Effect` without any I/O.

use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

use super::theme::ThemeName;
use super::{BoardData, Note};
use crate::actions::KINDS_EN;
use crate::project::{AgentView, ProjectKey};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Projects,
    Orchestration,
    Detail,
    Inbox,
}

/// Rects recorded during the last board draw, used for mouse hit testing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BoardHitMap {
    pub panels: Vec<(Rect, Panel)>,
    pub project_rows: Vec<(Rect, usize)>,
    pub agent_cards: Vec<(Rect, String)>,
    pub inbox_rows: Vec<(Rect, i64)>,
    pub picker_rows: Vec<(Rect, usize)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HitTarget {
    Panel(Panel),
    ProjectRow(usize),
    AgentCard(String),
    InboxRow(i64),
    PickerRow(usize),
}

/// Pure hit test: most specific target wins (rows/cards before panels).
/// Picker rows are checked first because the picker popup overlays the board.
pub fn hit(map: &BoardHitMap, x: u16, y: u16) -> Option<HitTarget> {
    for (rect, idx) in &map.picker_rows {
        if contains(*rect, x, y) {
            return Some(HitTarget::PickerRow(*idx));
        }
    }
    for (rect, idx) in &map.project_rows {
        if contains(*rect, x, y) {
            return Some(HitTarget::ProjectRow(*idx));
        }
    }
    for (rect, addr) in &map.agent_cards {
        if contains(*rect, x, y) {
            return Some(HitTarget::AgentCard(addr.clone()));
        }
    }
    for (rect, id) in &map.inbox_rows {
        if contains(*rect, x, y) {
            return Some(HitTarget::InboxRow(*id));
        }
    }
    for (rect, panel) in &map.panels {
        if contains(*rect, x, y) {
            return Some(HitTarget::Panel(*panel));
        }
    }
    None
}

fn contains(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
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
    agent_idx: usize,
    inbox_idx: usize,
    last_click: Option<(HitTarget, i64)>,
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
            agent_idx: 0,
            inbox_idx: 0,
            last_click: None,
        }
    }

    const DOUBLE_CLICK_MS: i64 = 400;

    pub fn on_mouse(
        &mut self,
        mouse: MouseEvent,
        map: &BoardHitMap,
        data: &BoardData,
        now_ms: i64,
    ) -> Effect {
        // Ignore all mouse events while any popup/confirm/prompt mode is open.
        if !matches!(self.mode, Mode::Normal | Mode::PickProject { .. }) {
            return Effect::None;
        }
        // Act only on Down/Drag/Up(Left) and Scroll; ignore others.
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left)
            | MouseEventKind::Drag(MouseButton::Left)
            | MouseEventKind::Up(MouseButton::Left)
            | MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown => {}
            _ => return Effect::None,
        }

        let Some(target) = hit(map, mouse.column, mouse.row) else {
            if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
                self.last_click = None;
            }
            return Effect::None;
        };

        match mouse.kind {
            MouseEventKind::ScrollUp => self.on_scroll(&target, -1, data),
            MouseEventKind::ScrollDown => self.on_scroll(&target, 1, data),
            MouseEventKind::Down(MouseButton::Left) => self.on_left_down(target, data, now_ms),
            // Drag/Up(Left): no action, but don't count as stale for unrelated reasons.
            MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left) => {
                Effect::None
            }
            _ => Effect::None,
        }
    }

    fn on_scroll(&mut self, target: &HitTarget, delta: i32, data: &BoardData) -> Effect {
        let panel = match target {
            HitTarget::Panel(p) => *p,
            HitTarget::ProjectRow(_) | HitTarget::PickerRow(_) => Panel::Projects,
            HitTarget::AgentCard(_) => Panel::Orchestration,
            HitTarget::InboxRow(_) => Panel::Inbox,
        };
        match panel {
            Panel::Projects => {
                if let Mode::PickProject { idx } = &mut self.mode {
                    let max = data.projects.len().saturating_sub(1);
                    if delta < 0 {
                        *idx = idx.saturating_sub(1);
                    } else if *idx < max {
                        *idx += 1;
                    }
                } else {
                    let max = data.projects.len().saturating_sub(1);
                    if delta < 0 {
                        self.sel_project = self.sel_project.saturating_sub(1);
                    } else if self.sel_project < max {
                        self.sel_project += 1;
                    }
                }
            }
            Panel::Orchestration => self.move_agent(delta, data),
            Panel::Detail => {
                if delta < 0 {
                    self.msg_scroll = self.msg_scroll.saturating_sub(1);
                } else {
                    self.msg_scroll = self.msg_scroll.saturating_add(1);
                }
            }
            Panel::Inbox => self.move_inbox(delta, data),
        }
        Effect::None
    }

    fn on_left_down(&mut self, target: HitTarget, data: &BoardData, now_ms: i64) -> Effect {
        let is_double = self.last_click.as_ref().is_some_and(|(prev, ts)| {
            *prev == target && now_ms.saturating_sub(*ts) <= Self::DOUBLE_CLICK_MS
        });
        self.last_click = Some((target.clone(), now_ms));

        if is_double {
            return self.on_double_click(target, data);
        }

        match target {
            HitTarget::Panel(panel) => {
                self.panel = panel;
                Effect::None
            }
            HitTarget::ProjectRow(idx) | HitTarget::PickerRow(idx) => {
                self.sel_project = idx.min(data.projects.len().saturating_sub(1));
                if let Mode::PickProject { idx: ref mut pidx } = self.mode {
                    *pidx = self.sel_project;
                } else {
                    self.panel = Panel::Projects;
                }
                Effect::None
            }
            HitTarget::AgentCard(address) => {
                self.panel = Panel::Orchestration;
                self.select_agent(&address, data);
                Effect::None
            }
            HitTarget::InboxRow(id) => {
                self.panel = Panel::Inbox;
                self.select_inbox(id, data);
                Effect::None
            }
        }
    }

    fn on_double_click(&mut self, target: HitTarget, data: &BoardData) -> Effect {
        // A double-click consumes the click pair.
        self.last_click = None;
        match target {
            HitTarget::ProjectRow(idx) | HitTarget::PickerRow(idx) => {
                self.sel_project = idx.min(data.projects.len().saturating_sub(1));
                if let Some(p) = data.projects.get(self.sel_project) {
                    let key = p.key.clone();
                    self.mode = Mode::Normal;
                    self.enter_project(key.clone());
                    return Effect::SwitchProject(key);
                }
                Effect::None
            }
            HitTarget::InboxRow(id) => {
                self.panel = Panel::Inbox;
                self.select_inbox(id, data);
                // Pre-fill the reply command exactly like the `r` key does (nothing is sent).
                self.mode = Mode::Reply {
                    id,
                    input: String::new(),
                };
                Effect::None
            }
            _ => Effect::None,
        }
    }

    fn select_agent(&mut self, address: &str, data: &BoardData) {
        let Some(view) = data.view.as_ref() else {
            return;
        };
        if let Some(i) = view.agents.iter().position(|a| a.address == address) {
            if self.sel_agent.as_deref() != Some(address) {
                self.msg_scroll = 0;
            }
            self.agent_idx = i;
            self.sel_agent = Some(address.to_string());
        }
    }

    fn select_inbox(&mut self, id: i64, data: &BoardData) {
        let Some(view) = data.view.as_ref() else {
            return;
        };
        if let Some(i) = view.inbox.iter().position(|r| r.id == id) {
            self.inbox_idx = i;
            self.sel_inbox = Some(id);
        }
    }

    pub fn on_key(&mut self, key: KeyEvent, data: &BoardData) -> Effect {
        if key.kind == KeyEventKind::Release {
            return Effect::None;
        }
        // Notes report the previous action or a failed refresh; any key the user presses in
        // normal mode dismisses it before the key itself is handled.
        if self.mode == Mode::Normal {
            self.note = None;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
        {
            return Effect::Quit;
        }

        match &self.mode.clone() {
            Mode::Normal => self.on_key_normal(key, data),
            Mode::Reply { id, input } => self.on_key_reply(key, *id, input.clone()),
            Mode::Send { to, kind, input } => {
                self.on_key_send(key, to.clone(), *kind, input.clone())
            }
            Mode::ConfirmDone { id } => self.on_key_confirm_done(key, *id),
            Mode::ShowItem { .. } | Mode::Help => {
                self.mode = Mode::Normal;
                Effect::None
            }
            Mode::Filter { input } => self.on_key_filter(key, input.clone()),
            Mode::PickProject { idx } => self.on_key_pick_project(key, *idx, data),
        }
    }

    fn on_key_normal(&mut self, key: KeyEvent, data: &BoardData) -> Effect {
        match key.code {
            KeyCode::Tab => {
                self.panel = next_panel(self.panel, false);
                Effect::None
            }
            KeyCode::BackTab => {
                self.panel = next_panel(self.panel, true);
                Effect::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_up(data);
                Effect::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_down(data);
                Effect::None
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if self.panel == Panel::Orchestration {
                    self.move_agent(-1, data);
                }
                Effect::None
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if self.panel == Panel::Orchestration {
                    self.move_agent(1, data);
                }
                Effect::None
            }
            KeyCode::Enter => self.enter_normal(data),
            KeyCode::Char('r') => {
                if let Some(id) = self.sel_inbox {
                    self.mode = Mode::Reply {
                        id,
                        input: String::new(),
                    };
                } else {
                    self.note = Some(Note {
                        ok: false,
                        text: "no inbox item selected".into(),
                    });
                }
                Effect::None
            }
            KeyCode::Char('d') => {
                if let Some(id) = self.sel_inbox {
                    self.mode = Mode::ConfirmDone { id };
                }
                Effect::None
            }
            KeyCode::Char('s') => {
                if let Some(to) = self.sel_agent.clone() {
                    self.mode = Mode::Send {
                        to,
                        kind: 0,
                        input: String::new(),
                    };
                }
                Effect::None
            }
            KeyCode::Char('/') => {
                self.mode = Mode::Filter {
                    input: self.filter.clone(),
                };
                Effect::None
            }
            KeyCode::Char('t') => {
                self.theme = self.theme.next();
                Effect::SaveTheme(self.theme)
            }
            KeyCode::Char('p') => {
                self.mode = Mode::PickProject {
                    idx: self.sel_project,
                };
                Effect::None
            }
            KeyCode::Char('?') => {
                self.mode = Mode::Help;
                Effect::None
            }
            KeyCode::Char('q') | KeyCode::Esc => Effect::Quit,
            _ => Effect::None,
        }
    }

    /// Switches project and forgets the previous project's selections.
    fn enter_project(&mut self, key: ProjectKey) {
        if self.project.as_ref() != Some(&key) {
            self.sel_agent = None;
            self.sel_inbox = None;
            self.agent_idx = 0;
            self.inbox_idx = 0;
            self.msg_scroll = 0;
        }
        self.project = Some(key);
        self.panel = Panel::Orchestration;
    }

    fn move_up(&mut self, data: &BoardData) {
        match self.panel {
            Panel::Projects => {
                self.sel_project = self.sel_project.saturating_sub(1);
            }
            Panel::Orchestration => self.move_agent(-1, data),
            Panel::Detail => {
                self.msg_scroll = self.msg_scroll.saturating_sub(1);
            }
            Panel::Inbox => self.move_inbox(-1, data),
        }
    }

    fn move_down(&mut self, data: &BoardData) {
        match self.panel {
            Panel::Projects => {
                let max = data.projects.len().saturating_sub(1);
                if self.sel_project < max {
                    self.sel_project += 1;
                }
            }
            Panel::Orchestration => self.move_agent(1, data),
            Panel::Detail => {
                self.msg_scroll = self.msg_scroll.saturating_add(1);
            }
            Panel::Inbox => self.move_inbox(1, data),
        }
    }

    fn move_agent(&mut self, delta: i32, data: &BoardData) {
        let Some(view) = data.view.as_ref() else {
            return;
        };
        let len = view.agents.len();
        if len == 0 {
            self.sel_agent = None;
            self.agent_idx = 0;
            return;
        }
        let cur = current_agent_idx(self, &view.agents);
        let next = if delta < 0 {
            cur.saturating_sub(1)
        } else {
            (cur + 1).min(len - 1)
        };
        if next != cur {
            self.msg_scroll = 0;
        }
        self.agent_idx = next;
        self.sel_agent = Some(view.agents[next].address.clone());
    }

    fn move_inbox(&mut self, delta: i32, data: &BoardData) {
        let Some(view) = data.view.as_ref() else {
            return;
        };
        let len = view.inbox.len();
        if len == 0 {
            self.sel_inbox = None;
            self.inbox_idx = 0;
            return;
        }
        let cur = current_inbox_idx(self, &view.inbox);
        let next = if delta < 0 {
            cur.saturating_sub(1)
        } else {
            (cur + 1).min(len - 1)
        };
        self.inbox_idx = next;
        self.sel_inbox = Some(view.inbox[next].id);
    }

    fn enter_normal(&mut self, data: &BoardData) -> Effect {
        match self.panel {
            Panel::Projects => {
                if let Some(p) = data.projects.get(self.sel_project) {
                    let key = p.key.clone();
                    self.enter_project(key.clone());
                    return Effect::SwitchProject(key);
                }
                Effect::None
            }
            Panel::Orchestration => {
                self.panel = Panel::Detail;
                Effect::None
            }
            Panel::Inbox => {
                if let Some(id) = self.sel_inbox {
                    self.mode = Mode::ShowItem { id };
                }
                Effect::None
            }
            Panel::Detail => Effect::None,
        }
    }

    fn on_key_reply(&mut self, key: KeyEvent, id: i64, mut input: String) -> Effect {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                Effect::None
            }
            KeyCode::Backspace => {
                input.pop();
                self.mode = Mode::Reply { id, input };
                Effect::None
            }
            KeyCode::Enter => {
                let text = input.trim().to_string();
                if text.is_empty() {
                    self.mode = Mode::Reply { id, input };
                    Effect::None
                } else {
                    self.mode = Mode::Normal;
                    Effect::Reply { id, text }
                }
            }
            KeyCode::Char(c) => {
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT)
                {
                    input.push(c);
                }
                self.mode = Mode::Reply { id, input };
                Effect::None
            }
            _ => {
                self.mode = Mode::Reply { id, input };
                Effect::None
            }
        }
    }

    fn on_key_send(
        &mut self,
        key: KeyEvent,
        to: String,
        mut kind: usize,
        mut input: String,
    ) -> Effect {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                Effect::None
            }
            KeyCode::Backspace => {
                input.pop();
                self.mode = Mode::Send { to, kind, input };
                Effect::None
            }
            KeyCode::Tab => {
                if !KINDS_EN.is_empty() {
                    kind = (kind + 1) % KINDS_EN.len();
                }
                self.mode = Mode::Send { to, kind, input };
                Effect::None
            }
            KeyCode::Enter => {
                let text = input.trim().to_string();
                if text.is_empty() {
                    self.mode = Mode::Send { to, kind, input };
                    Effect::None
                } else {
                    let stored = KINDS_EN
                        .get(kind)
                        .map(|(_, code)| (*code).to_string())
                        .unwrap_or_else(|| "GIAO".to_string());
                    self.mode = Mode::Normal;
                    Effect::Send {
                        to,
                        kind: stored,
                        text,
                    }
                }
            }
            KeyCode::Char(c) => {
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT)
                {
                    input.push(c);
                }
                self.mode = Mode::Send { to, kind, input };
                Effect::None
            }
            _ => {
                self.mode = Mode::Send { to, kind, input };
                Effect::None
            }
        }
    }

    fn on_key_confirm_done(&mut self, key: KeyEvent, id: i64) -> Effect {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.mode = Mode::Normal;
                Effect::Done { id }
            }
            _ => {
                self.mode = Mode::Normal;
                Effect::None
            }
        }
    }

    fn on_key_filter(&mut self, key: KeyEvent, mut input: String) -> Effect {
        match key.code {
            KeyCode::Esc => {
                self.filter.clear();
                self.mode = Mode::Normal;
                Effect::None
            }
            KeyCode::Backspace => {
                input.pop();
                self.mode = Mode::Filter { input };
                Effect::None
            }
            KeyCode::Enter => {
                self.filter = input.trim().to_string();
                self.mode = Mode::Normal;
                Effect::None
            }
            KeyCode::Char(c) => {
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT)
                {
                    input.push(c);
                }
                self.mode = Mode::Filter { input };
                Effect::None
            }
            _ => {
                self.mode = Mode::Filter { input };
                Effect::None
            }
        }
    }

    fn on_key_pick_project(&mut self, key: KeyEvent, mut idx: usize, data: &BoardData) -> Effect {
        let max = data.projects.len().saturating_sub(1);
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                Effect::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                idx = idx.saturating_sub(1);
                self.mode = Mode::PickProject { idx };
                Effect::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if idx < max {
                    idx += 1;
                }
                self.mode = Mode::PickProject { idx };
                Effect::None
            }
            KeyCode::Enter => {
                self.mode = Mode::Normal;
                if let Some(p) = data.projects.get(idx) {
                    let key = p.key.clone();
                    self.sel_project = idx;
                    self.enter_project(key.clone());
                    return Effect::SwitchProject(key);
                }
                Effect::None
            }
            _ => {
                self.mode = Mode::PickProject { idx };
                Effect::None
            }
        }
    }

    pub fn sync(&mut self, data: &BoardData) {
        let max_project = data.projects.len().saturating_sub(1);
        if data.projects.is_empty() {
            self.sel_project = 0;
        } else {
            self.sel_project = self.sel_project.min(max_project);
        }

        if let Some(view) = data.view.as_ref() {
            sync_agents(self, &view.agents);
            sync_inbox(self, &view.inbox);
            sync_popup(self, &view.inbox);
        } else {
            self.sel_agent = None;
            self.sel_inbox = None;
            self.agent_idx = 0;
            self.inbox_idx = 0;
            sync_popup_no_view(self);
        }
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

fn next_panel(panel: Panel, reverse: bool) -> Panel {
    const ORDER: [Panel; 3] = [Panel::Orchestration, Panel::Detail, Panel::Inbox];
    match ORDER.iter().position(|p| *p == panel) {
        Some(idx) => {
            let next = if reverse {
                (idx + ORDER.len() - 1) % ORDER.len()
            } else {
                (idx + 1) % ORDER.len()
            };
            ORDER[next]
        }
        None => {
            if reverse {
                Panel::Inbox
            } else {
                Panel::Orchestration
            }
        }
    }
}

fn current_agent_idx(state: &BoardState, agents: &[AgentView]) -> usize {
    if let Some(addr) = state.sel_agent.as_deref()
        && let Some(i) = agents.iter().position(|a| a.address == addr)
    {
        return i;
    }
    state.agent_idx.min(agents.len().saturating_sub(1))
}

fn current_inbox_idx(state: &BoardState, inbox: &[crate::store::InboxRow]) -> usize {
    if let Some(id) = state.sel_inbox
        && let Some(i) = inbox.iter().position(|r| r.id == id)
    {
        return i;
    }
    state.inbox_idx.min(inbox.len().saturating_sub(1))
}

fn sync_agents(state: &mut BoardState, agents: &[AgentView]) {
    if agents.is_empty() {
        state.sel_agent = None;
        state.agent_idx = 0;
        return;
    }
    if let Some(addr) = state.sel_agent.as_deref() {
        if let Some(i) = agents.iter().position(|a| a.address == addr) {
            state.agent_idx = i;
            state.sel_agent = Some(agents[i].address.clone());
            return;
        }
    } else {
        state.agent_idx = 0;
        state.sel_agent = Some(agents[0].address.clone());
        return;
    }
    let idx = state.agent_idx.min(agents.len() - 1);
    state.agent_idx = idx;
    state.sel_agent = Some(agents[idx].address.clone());
}

fn sync_inbox(state: &mut BoardState, inbox: &[crate::store::InboxRow]) {
    if inbox.is_empty() {
        state.sel_inbox = None;
        state.inbox_idx = 0;
        return;
    }
    if let Some(id) = state.sel_inbox {
        if let Some(i) = inbox.iter().position(|r| r.id == id) {
            state.inbox_idx = i;
            state.sel_inbox = Some(inbox[i].id);
            return;
        }
    } else {
        state.inbox_idx = 0;
        state.sel_inbox = Some(inbox[0].id);
        return;
    }
    let idx = state.inbox_idx.min(inbox.len() - 1);
    state.inbox_idx = idx;
    state.sel_inbox = Some(inbox[idx].id);
}

fn popup_item_id(mode: &Mode) -> Option<i64> {
    match mode {
        Mode::Reply { id, .. } | Mode::ConfirmDone { id } | Mode::ShowItem { id } => Some(*id),
        _ => None,
    }
}

fn sync_popup(state: &mut BoardState, inbox: &[crate::store::InboxRow]) {
    if let Some(id) = popup_item_id(&state.mode)
        && !inbox.iter().any(|r| r.id == id)
    {
        state.mode = Mode::Normal;
        state.note = Some(Note {
            ok: false,
            text: format!("item #{id} is closed"),
        });
    }
}

fn sync_popup_no_view(state: &mut BoardState) {
    if let Some(id) = popup_item_id(&state.mode) {
        state.mode = Mode::Normal;
        state.note = Some(Note {
            ok: false,
            text: format!("item #{id} is closed"),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::KINDS_EN;
    use crate::project::{AgentView, ProjectKey, ProjectSummary, ProjectView};
    use crate::store::InboxRow;
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

    fn pk(name: &str) -> ProjectKey {
        ProjectKey {
            root: format!("/repo/{name}"),
            name: name.into(),
        }
    }

    fn summary(name: &str) -> ProjectSummary {
        ProjectSummary {
            key: pk(name),
            live_agents: 1,
            open_inbox: 0,
        }
    }

    fn agent(address: &str, depth: usize) -> AgentView {
        AgentView {
            address: address.into(),
            label: address.into(),
            app: "claude".into(),
            model: "opus".into(),
            effort: String::new(),
            status: "-".into(),
            state: "live".into(),
            depth,
            last_sibling: false,
            doing: None,
            last_msg: None,
            context_pct: None,
            tokens: None,
            spark: None,
            stage: None,
            task: None,
            role: None,
        }
    }

    fn inbox(id: i64) -> InboxRow {
        InboxRow {
            id,
            ts_ms: 1,
            terminal: Some("t1".into()),
            source: "source".into(),
            kind: Some("blocked".into()),
            text: format!("item {id}"),
            resolved_ms: None,
            reply: None,
        }
    }

    fn view_with(agents: Vec<AgentView>, inbox_items: Vec<InboxRow>) -> ProjectView {
        ProjectView {
            key: pk("agmag"),
            agents,
            inbox: inbox_items,
            messages: Vec::new(),
        }
    }

    fn data() -> BoardData {
        BoardData {
            projects: vec![summary("agmag"), summary("shop")],
            view: Some(view_with(
                vec![agent("t1", 0), agent("t2", 1), agent("t3", 1)],
                vec![inbox(10), inbox(11)],
            )),
            ..BoardData::default()
        }
    }

    fn state_with_data() -> (BoardState, BoardData) {
        let d = data();
        let mut s = BoardState::new(Some(pk("agmag")), ThemeName::CatppuccinMocha);
        s.sel_agent = Some("t1".into());
        s.sel_inbox = Some(10);
        (s, d)
    }

    #[test]
    fn release_events_ignored() {
        let (mut s, d) = state_with_data();
        let before = s.clone();
        let e = s.on_key(key_kind(KeyCode::Char('q'), KeyEventKind::Release), &d);
        assert_eq!(e, Effect::None);
        assert_eq!(s.panel, before.panel);
        assert_eq!(s.mode, before.mode);
        assert_eq!(s.sel_agent, before.sel_agent);
    }

    #[test]
    fn tab_cycles_panels_both_ways() {
        let (mut s, d) = state_with_data();
        s.panel = Panel::Projects;
        s.on_key(key(KeyCode::Tab), &d);
        assert_eq!(s.panel, Panel::Orchestration);
        s.on_key(key(KeyCode::Tab), &d);
        assert_eq!(s.panel, Panel::Detail);
        s.on_key(key(KeyCode::Tab), &d);
        assert_eq!(s.panel, Panel::Inbox);
        s.on_key(key(KeyCode::Tab), &d);
        assert_eq!(s.panel, Panel::Orchestration);
        s.on_key(key(KeyCode::BackTab), &d);
        assert_eq!(s.panel, Panel::Inbox);
    }

    #[test]
    fn tab_skips_projects_panel() {
        let (mut s, d) = state_with_data();
        s.panel = Panel::Inbox;
        s.on_key(key(KeyCode::Tab), &d);
        assert_eq!(s.panel, Panel::Orchestration);
        s.on_key(key(KeyCode::BackTab), &d);
        assert_eq!(s.panel, Panel::Inbox);
        s.panel = Panel::Projects;
        s.on_key(key(KeyCode::Tab), &d);
        assert_ne!(s.panel, Panel::Projects);
    }

    #[test]
    fn navigation_clamps_without_wrap() {
        let (mut s, d) = state_with_data();
        s.panel = Panel::Projects;
        s.sel_project = 0;
        s.on_key(key(KeyCode::Up), &d);
        assert_eq!(s.sel_project, 0);
        s.sel_project = 1;
        s.on_key(key(KeyCode::Down), &d);
        assert_eq!(s.sel_project, 1);

        s.panel = Panel::Orchestration;
        s.sel_agent = Some("t1".into());
        s.on_key(key(KeyCode::Up), &d);
        assert_eq!(s.sel_agent.as_deref(), Some("t1"));
        s.sel_agent = Some("t3".into());
        s.on_key(key(KeyCode::Down), &d);
        assert_eq!(s.sel_agent.as_deref(), Some("t3"));

        s.panel = Panel::Detail;
        s.msg_scroll = 0;
        s.on_key(key(KeyCode::Up), &d);
        assert_eq!(s.msg_scroll, 0);
        s.msg_scroll = 3;
        s.on_key(key(KeyCode::Down), &d);
        assert_eq!(s.msg_scroll, 4);

        s.panel = Panel::Inbox;
        s.sel_inbox = Some(10);
        s.on_key(key(KeyCode::Up), &d);
        assert_eq!(s.sel_inbox, Some(10));
        s.sel_inbox = Some(11);
        s.on_key(key(KeyCode::Down), &d);
        assert_eq!(s.sel_inbox, Some(11));
    }

    #[test]
    fn enter_on_project_switches() {
        let (mut s, d) = state_with_data();
        s.panel = Panel::Projects;
        s.sel_project = 1;
        let e = s.on_key(key(KeyCode::Enter), &d);
        assert_eq!(e, Effect::SwitchProject(pk("shop")));
        assert_eq!(s.project, Some(pk("shop")));
        assert_eq!(s.panel, Panel::Orchestration);
    }

    #[test]
    fn reply_popup_types_and_sends() {
        let (mut s, d) = state_with_data();
        s.panel = Panel::Inbox;
        s.sel_inbox = Some(10);
        s.on_key(char_key('r'), &d);
        assert_eq!(
            s.mode,
            Mode::Reply {
                id: 10,
                input: String::new()
            }
        );
        s.on_key(char_key('o'), &d);
        s.on_key(char_key('k'), &d);
        let e = s.on_key(key(KeyCode::Enter), &d);
        assert_eq!(
            e,
            Effect::Reply {
                id: 10,
                text: "ok".into()
            }
        );
        assert_eq!(s.mode, Mode::Normal);
    }

    #[test]
    fn reply_blank_does_not_send() {
        let (mut s, d) = state_with_data();
        s.mode = Mode::Reply {
            id: 10,
            input: "   ".into(),
        };
        let e = s.on_key(key(KeyCode::Enter), &d);
        assert_eq!(e, Effect::None);
        assert!(matches!(s.mode, Mode::Reply { id: 10, .. }));
    }

    #[test]
    fn send_popup_tab_cycles_kind_and_emits_stored_code() {
        let (mut s, d) = state_with_data();
        s.panel = Panel::Orchestration;
        s.sel_agent = Some("t2".into());
        s.on_key(char_key('s'), &d);
        assert_eq!(
            s.mode,
            Mode::Send {
                to: "t2".into(),
                kind: 0,
                input: String::new()
            }
        );
        s.on_key(key(KeyCode::Tab), &d);
        assert!(matches!(s.mode, Mode::Send { kind: 1, .. }));
        s.on_key(char_key('h'), &d);
        s.on_key(char_key('i'), &d);
        let e = s.on_key(key(KeyCode::Enter), &d);
        assert_eq!(
            e,
            Effect::Send {
                to: "t2".into(),
                kind: KINDS_EN[1].1.to_string(),
                text: "hi".into(),
            }
        );
        assert_eq!(KINDS_EN[1].1, "BÁO CÁO");
    }

    #[test]
    fn done_requires_y() {
        let (mut s, d) = state_with_data();
        s.panel = Panel::Inbox;
        s.sel_inbox = Some(11);
        s.on_key(char_key('d'), &d);
        assert_eq!(s.mode, Mode::ConfirmDone { id: 11 });
        let e = s.on_key(char_key('n'), &d);
        assert_eq!(e, Effect::None);
        assert_eq!(s.mode, Mode::Normal);

        s.mode = Mode::ConfirmDone { id: 11 };
        let e = s.on_key(char_key('y'), &d);
        assert_eq!(e, Effect::Done { id: 11 });
        assert_eq!(s.mode, Mode::Normal);
    }

    #[test]
    fn esc_cancels_popup_without_effect() {
        let (mut s, d) = state_with_data();
        s.mode = Mode::Reply {
            id: 10,
            input: "draft".into(),
        };
        let e = s.on_key(key(KeyCode::Esc), &d);
        assert_eq!(e, Effect::None);
        assert_eq!(s.mode, Mode::Normal);
    }

    #[test]
    fn q_quits_in_normal_mode_only() {
        let (mut s, d) = state_with_data();
        let e = s.on_key(char_key('q'), &d);
        assert_eq!(e, Effect::Quit);

        let (mut s, d) = state_with_data();
        s.mode = Mode::Reply {
            id: 10,
            input: String::new(),
        };
        let e = s.on_key(char_key('q'), &d);
        assert_eq!(e, Effect::None);
        assert_eq!(
            s.mode,
            Mode::Reply {
                id: 10,
                input: "q".into()
            }
        );
    }

    #[test]
    fn theme_key_cycles_and_saves() {
        let (mut s, d) = state_with_data();
        assert_eq!(s.theme, ThemeName::CatppuccinMocha);
        let e = s.on_key(char_key('t'), &d);
        assert_eq!(s.theme, ThemeName::TokyoNight);
        assert_eq!(e, Effect::SaveTheme(ThemeName::TokyoNight));
    }

    #[test]
    fn filter_set_and_clear() {
        let (mut s, d) = state_with_data();
        s.on_key(char_key('/'), &d);
        assert!(matches!(s.mode, Mode::Filter { .. }));
        s.on_key(char_key('f'), &d);
        s.on_key(char_key('o'), &d);
        s.on_key(char_key('o'), &d);
        let e = s.on_key(key(KeyCode::Enter), &d);
        assert_eq!(e, Effect::None);
        assert_eq!(s.filter, "foo");
        assert_eq!(s.mode, Mode::Normal);

        s.on_key(char_key('/'), &d);
        let e = s.on_key(key(KeyCode::Esc), &d);
        assert_eq!(e, Effect::None);
        assert_eq!(s.filter, "");
        assert_eq!(s.mode, Mode::Normal);
    }

    #[test]
    fn sync_keeps_selection_by_address_after_reorder() {
        let (mut s, mut d) = state_with_data();
        s.sel_agent = Some("t2".into());
        if let Some(v) = d.view.as_mut() {
            v.agents = vec![agent("t3", 0), agent("t2", 1), agent("t1", 1)];
        }
        s.sync(&d);
        assert_eq!(s.sel_agent.as_deref(), Some("t2"));
    }

    #[test]
    fn sync_moves_to_neighbor_when_agent_gone() {
        let (mut s, mut d) = state_with_data();
        s.sel_agent = Some("t2".into());
        s.sync(&d);
        if let Some(v) = d.view.as_mut() {
            v.agents = vec![agent("t1", 0), agent("t3", 1)];
        }
        s.sync(&d);
        assert_eq!(s.sel_agent.as_deref(), Some("t3"));
    }

    #[test]
    fn sync_closes_popup_for_closed_item() {
        let (mut s, mut d) = state_with_data();
        s.mode = Mode::Reply {
            id: 10,
            input: "x".into(),
        };
        if let Some(v) = d.view.as_mut() {
            v.inbox.retain(|i| i.id != 10);
        }
        s.sync(&d);
        assert_eq!(s.mode, Mode::Normal);
        assert_eq!(
            s.note.as_ref().map(|n| n.text.as_str()),
            Some("item #10 is closed")
        );
    }

    #[test]
    fn switching_project_forgets_old_selection() {
        let (mut st, mut d) = state_with_data();
        st.sel_agent = Some("old-agent".into());
        st.sel_inbox = Some(999);
        st.msg_scroll = 7;
        d.projects = vec![summary("agmag"), summary("shop")];
        st.panel = Panel::Projects;
        st.sel_project = 1;
        assert_eq!(
            st.on_key(key(KeyCode::Enter), &d),
            Effect::SwitchProject(pk("shop"))
        );
        assert_eq!(st.sel_agent, None);
        assert_eq!(st.sel_inbox, None);
        assert_eq!(st.msg_scroll, 0);
        assert_eq!(st.panel, Panel::Orchestration);
    }

    #[test]
    fn selecting_another_agent_resets_message_scroll() {
        let (mut st, d) = state_with_data();
        st.sync(&d);
        st.panel = Panel::Orchestration;
        st.msg_scroll = 5;
        st.on_key(key(KeyCode::Down), &d);
        assert_eq!(st.msg_scroll, 0);
    }

    #[test]
    fn normal_key_clears_note() {
        let (mut s, d) = state_with_data();
        s.note = Some(crate::board::Note {
            ok: true,
            text: "replied to #1".into(),
        });
        s.on_key(key(KeyCode::Up), &d);
        assert_eq!(s.note, None, "a normal-mode key must clear the note");

        // Popup modes are not Normal: their keys leave the note alone.
        s.note = Some(crate::board::Note {
            ok: false,
            text: "no inbox item selected".into(),
        });
        s.mode = Mode::Filter {
            input: String::new(),
        };
        s.on_key(char_key('x'), &d);
        assert!(s.note.is_some(), "popup keys must not clear the note");
    }

    #[test]
    fn sync_on_empty_data_is_none() {
        let mut s = BoardState::new(None, ThemeName::CatppuccinMocha);
        s.sel_agent = Some("ghost".into());
        s.sel_inbox = Some(99);
        let d = BoardData::default();
        s.sync(&d);
        assert_eq!(s.sel_agent, None);
        assert_eq!(s.sel_inbox, None);

        let mut s = BoardState::new(Some(pk("p")), ThemeName::Nord);
        s.sel_agent = Some("ghost".into());
        let d = BoardData {
            view: Some(view_with(Vec::new(), Vec::new())),
            ..BoardData::default()
        };
        s.sync(&d);
        assert_eq!(s.sel_agent, None);
        assert_eq!(s.sel_inbox, None);
    }

    fn mouse(kind: MouseEventKind, col: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn hit_map() -> BoardHitMap {
        BoardHitMap {
            panels: vec![
                (Rect::new(0, 0, 20, 10), Panel::Orchestration),
                (Rect::new(20, 0, 20, 10), Panel::Detail),
                (Rect::new(0, 10, 40, 5), Panel::Inbox),
            ],
            agent_cards: vec![
                (Rect::new(1, 1, 8, 3), "t1".into()),
                (Rect::new(1, 4, 8, 3), "t2".into()),
            ],
            inbox_rows: vec![(Rect::new(1, 11, 38, 1), 10), (Rect::new(1, 12, 38, 1), 11)],
            ..BoardHitMap::default()
        }
    }

    #[test]
    fn mouse_down_focuses_panel_and_selects_targets() {
        let (mut s, d) = state_with_data();
        let map = hit_map();
        let e = s.on_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), 25, 2),
            &map,
            &d,
            1,
        );
        assert_eq!(e, Effect::None);
        assert_eq!(s.panel, Panel::Detail);

        let e = s.on_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), 2, 5),
            &map,
            &d,
            2,
        );
        assert_eq!(e, Effect::None);
        assert_eq!(s.panel, Panel::Orchestration);
        assert_eq!(s.sel_agent.as_deref(), Some("t2"));

        let e = s.on_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), 2, 12),
            &map,
            &d,
            3,
        );
        assert_eq!(e, Effect::None);
        assert_eq!(s.panel, Panel::Inbox);
        assert_eq!(s.sel_inbox, Some(11));
    }

    #[test]
    fn mouse_scroll_moves_selection_or_detail_scroll() {
        let (mut s, d) = state_with_data();
        let map = hit_map();
        s.panel = Panel::Orchestration;
        s.sel_agent = Some("t1".into());
        s.sync(&d);
        s.on_mouse(mouse(MouseEventKind::ScrollDown, 2, 2), &map, &d, 1);
        assert_eq!(s.sel_agent.as_deref(), Some("t2"));

        s.on_mouse(mouse(MouseEventKind::ScrollDown, 25, 2), &map, &d, 2);
        assert_eq!(s.msg_scroll, 1);
        s.on_mouse(mouse(MouseEventKind::ScrollUp, 25, 2), &map, &d, 3);
        assert_eq!(s.msg_scroll, 0);

        s.sel_inbox = Some(10);
        s.sync(&d);
        s.on_mouse(mouse(MouseEventKind::ScrollDown, 2, 11), &map, &d, 4);
        assert_eq!(s.sel_inbox, Some(11));
    }

    #[test]
    fn mouse_double_click_opens_project_and_replies_inbox() {
        let (mut s, d) = state_with_data();
        let map = BoardHitMap {
            panels: vec![(Rect::new(0, 0, 20, 10), Panel::Projects)],
            project_rows: vec![(Rect::new(1, 1, 18, 1), 1)],
            picker_rows: vec![(Rect::new(5, 5, 10, 1), 0)],
            inbox_rows: vec![(Rect::new(1, 11, 38, 1), 10)],
            ..BoardHitMap::default()
        };

        let e = s.on_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), 2, 1),
            &map,
            &d,
            100,
        );
        assert_eq!(e, Effect::None);
        let e = s.on_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), 2, 1),
            &map,
            &d,
            300,
        );
        assert_eq!(e, Effect::SwitchProject(pk("shop")));
        assert_eq!(s.project, Some(pk("shop")));

        let (mut s, d) = state_with_data();
        s.mode = Mode::PickProject { idx: 0 };
        let e = s.on_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), 6, 5),
            &map,
            &d,
            100,
        );
        assert_eq!(e, Effect::None);
        let e = s.on_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), 6, 5),
            &map,
            &d,
            400,
        );
        assert_eq!(e, Effect::SwitchProject(pk("agmag")));
        assert_eq!(s.mode, Mode::Normal);

        let (mut s, d) = state_with_data();
        let e = s.on_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), 2, 11),
            &map,
            &d,
            100,
        );
        assert_eq!(e, Effect::None);
        let e = s.on_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), 2, 11),
            &map,
            &d,
            450,
        );
        assert_eq!(e, Effect::None);
        assert_eq!(
            s.mode,
            Mode::Reply {
                id: 10,
                input: String::new()
            }
        );
    }

    #[test]
    fn mouse_ignored_in_popup_modes_and_non_left_actions() {
        let (mut s, d) = state_with_data();
        let map = hit_map();
        s.mode = Mode::ConfirmDone { id: 10 };
        let e = s.on_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), 2, 2),
            &map,
            &d,
            1,
        );
        assert_eq!(e, Effect::None);
        assert_eq!(s.mode, Mode::ConfirmDone { id: 10 });

        s.mode = Mode::Normal;
        let e = s.on_mouse(
            mouse(MouseEventKind::Down(MouseButton::Right), 2, 2),
            &map,
            &d,
            2,
        );
        assert_eq!(e, Effect::None);
        assert_eq!(s.sel_agent.as_deref(), Some("t1"));
        let e = s.on_mouse(mouse(MouseEventKind::Moved, 2, 2), &map, &d, 3);
        assert_eq!(e, Effect::None);
    }

    #[test]
    fn mouse_cannot_reach_destructive_or_send_actions() {
        let (mut s, d) = state_with_data();
        let map = BoardHitMap {
            panels: vec![
                (Rect::new(0, 0, 20, 10), Panel::Orchestration),
                (Rect::new(20, 0, 20, 10), Panel::Detail),
                (Rect::new(0, 10, 40, 5), Panel::Inbox),
                (Rect::new(0, 15, 40, 5), Panel::Projects),
            ],
            project_rows: vec![(Rect::new(1, 16, 10, 1), 0)],
            agent_cards: vec![(Rect::new(1, 1, 8, 3), "t1".into())],
            inbox_rows: vec![(Rect::new(1, 11, 38, 1), 10)],
            picker_rows: vec![(Rect::new(1, 17, 10, 1), 1)],
        };
        let kinds = [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Down(MouseButton::Right),
            MouseEventKind::Down(MouseButton::Middle),
            MouseEventKind::Up(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Right),
            MouseEventKind::Drag(MouseButton::Left),
            MouseEventKind::Drag(MouseButton::Right),
            MouseEventKind::Moved,
            MouseEventKind::ScrollDown,
            MouseEventKind::ScrollUp,
        ];
        let coords = [(1u16, 1u16), (25, 2), (2, 11), (2, 16), (2, 17), (99, 99)];
        let mut t = 0i64;
        for kind in kinds {
            for (c, r) in coords {
                for _ in 0..3 {
                    t += 10;
                    let e = s.on_mouse(mouse(kind, c, r), &map, &d, t);
                    assert!(
                        matches!(e, Effect::None | Effect::SwitchProject(_)),
                        "mouse produced unsafe effect: {e:?}"
                    );
                    assert!(
                        !matches!(s.mode, Mode::ConfirmDone { .. }),
                        "mouse entered confirm mode"
                    );
                    // Reply mode is allowed only as prefill (no send effect produced).
                }
            }
        }
    }
}
