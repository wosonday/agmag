//! Global screen state of the full-screen `agm tui`: focus, command line, popup, selections.

use std::cell::RefCell;
use std::collections::HashMap;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;

use super::cmd::NoteKind;
use super::theme::ThemeName;
use crate::dashboard::{Section, Snapshot, TermView, sanitize_one_line};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CmdKey {
    Edited,
    Submit(String),
    SubmitEmpty,
    Esc,
    Unhandled,
}

impl CmdLine {
    pub(crate) fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.hist_pos = None;
        self.draft = None;
    }

    pub(crate) fn on_key(&mut self, key: KeyEvent) -> CmdKey {
        match key.code {
            KeyCode::Enter => {
                let text = self.text.clone();
                if text.trim().is_empty() {
                    return CmdKey::SubmitEmpty;
                }
                self.push_history(text.clone());
                self.clear();
                CmdKey::Submit(text)
            }
            KeyCode::Esc => {
                self.clear();
                CmdKey::Esc
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    let idx = char_to_byte(&self.text, self.cursor - 1);
                    let next = char_to_byte(&self.text, self.cursor);
                    self.text.drain(idx..next);
                    self.cursor -= 1;
                }
                CmdKey::Edited
            }
            KeyCode::Delete => {
                let len = self.text.chars().count();
                if self.cursor < len {
                    let idx = char_to_byte(&self.text, self.cursor);
                    let next = char_to_byte(&self.text, self.cursor + 1);
                    self.text.drain(idx..next);
                }
                CmdKey::Edited
            }
            KeyCode::Left => {
                self.cursor = self.cursor.saturating_sub(1);
                CmdKey::Edited
            }
            KeyCode::Right => {
                let len = self.text.chars().count();
                if self.cursor < len {
                    self.cursor += 1;
                }
                CmdKey::Edited
            }
            KeyCode::Home => {
                self.cursor = 0;
                CmdKey::Edited
            }
            KeyCode::End => {
                self.cursor = self.text.chars().count();
                CmdKey::Edited
            }
            KeyCode::Up => {
                self.history_up();
                CmdKey::Edited
            }
            KeyCode::Down => {
                self.history_down();
                CmdKey::Edited
            }
            KeyCode::Char(c)
                if !c.is_control()
                    && !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                let idx = char_to_byte(&self.text, self.cursor);
                self.text.insert(idx, c);
                self.cursor += 1;
                CmdKey::Edited
            }
            _ => CmdKey::Unhandled,
        }
    }

    fn push_history(&mut self, text: String) {
        if self.history.last().is_some_and(|last| last == &text) {
            return;
        }
        self.history.push(text);
        if self.history.len() > 50 {
            let drop = self.history.len() - 50;
            self.history.drain(0..drop);
        }
    }

    fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let pos = match self.hist_pos {
            Some(p) => p.saturating_sub(1),
            None => {
                self.draft = Some(self.text.clone());
                self.history.len() - 1
            }
        };
        self.hist_pos = Some(pos);
        self.text = self.history[pos].clone();
        self.cursor = self.text.chars().count();
    }

    fn history_down(&mut self) {
        let Some(pos) = self.hist_pos else {
            return;
        };
        if pos + 1 < self.history.len() {
            let next = pos + 1;
            self.hist_pos = Some(next);
            self.text = self.history[next].clone();
            self.cursor = self.text.chars().count();
        } else {
            self.hist_pos = None;
            self.text = self.draft.take().unwrap_or_default();
            self.cursor = self.text.chars().count();
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Popup {
    Confirm { prompt: String, input: String },
}

/// What a drawn row points at. The board (T2) uses the Agent/Project variants; the overview
/// uses Terminal/Inbox/Message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowTarget {
    Terminal(String),
    Inbox(i64),
    Message(usize),
    Agent(String),
    Project(String),
}

/// A draggable boundary recorded by the renderer (mouse handling comes in T3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderId {
    /// Divider between the two top panels on wide screens.
    TopPairVertical,
    /// Boundary above the section with this index in `visible_sections(&data.view)`.
    Horizontal(usize),
    /// Top edge of the Output overlay, above the lower half of the section area.
    OutputTop,
}

/// A panel recorded by the renderer. Output is an overlay, not one of the view's `Section`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitPanel {
    Section(Section),
    Output,
}

impl From<Section> for HitPanel {
    fn from(section: Section) -> Self {
        HitPanel::Section(section)
    }
}

/// What the last frame drew where. Rebuilt on every draw and kept in `GlobalState` so the app
/// loop can hit-test after drawing without re-deriving the layout.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HitMap {
    pub panels: Vec<(Rect, HitPanel)>,
    pub rows: Vec<(Rect, RowTarget)>,
    pub borders: Vec<(Rect, BorderId)>,
}

impl HitMap {
    pub fn clear(&mut self) {
        self.panels.clear();
        self.rows.clear();
        self.borders.clear();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hit {
    Panel(HitPanel),
    Row(RowTarget),
    Border(BorderId),
}

/// Pure hit testing over a recorded map: the narrowest target wins (border, then row, then panel).
/// Rect edges are exclusive, like ratatui's own `Rect::contains`.
pub fn hit(map: &HitMap, x: u16, y: u16) -> Option<Hit> {
    let inside = |rect: &Rect| {
        x >= rect.x
            && y >= rect.y
            && x < rect.x.saturating_add(rect.width)
            && y < rect.y.saturating_add(rect.height)
    };
    if let Some((_, id)) = map.borders.iter().find(|(rect, _)| inside(rect)) {
        return Some(Hit::Border(*id));
    }
    if let Some((_, target)) = map.rows.iter().find(|(rect, _)| inside(rect)) {
        return Some(Hit::Row(target.clone()));
    }
    if let Some((_, section)) = map.panels.iter().find(|(rect, _)| inside(rect)) {
        return Some(Hit::Panel(*section));
    }
    None
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
    /// Index of `sel_terminal` in the last snapshot, so a removed row keeps its neighbour.
    prev_terminal_idx: Option<usize>,
    /// Index of `sel_inbox` in the last snapshot, so a removed row keeps its neighbour.
    prev_inbox_idx: Option<usize>,
    /// Rebuilt by `draw_global` every frame; read by the app loop after drawing.
    hit_map: RefCell<HitMap>,
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
            prev_terminal_idx: None,
            prev_inbox_idx: None,
            hit_map: RefCell::new(HitMap::default()),
        }
    }

    /// A snapshot of what the last draw put on screen.
    pub fn hit_map(&self) -> HitMap {
        self.hit_map.borrow().clone()
    }

    pub(crate) fn clear_hit_map(&self) {
        self.hit_map.borrow_mut().clear();
    }

    pub(crate) fn push_panel(&self, rect: Rect, panel: impl Into<HitPanel>) {
        self.hit_map.borrow_mut().panels.push((rect, panel.into()));
    }

    pub(crate) fn push_row(&self, rect: Rect, target: RowTarget) {
        self.hit_map.borrow_mut().rows.push((rect, target));
    }

    pub(crate) fn push_border(&self, rect: Rect, id: BorderId) {
        self.hit_map.borrow_mut().borders.push((rect, id));
    }

    /// Clip away entries hidden by an overlay drawn over part of the section area. The Output
    /// panel spans the full width, so removing the vertical band of each rect is enough: a rect
    /// starting above the overlay keeps its visible top part, one starting inside is dropped.
    pub(crate) fn clip_hit_map(&self, hidden: Rect) {
        fn clip(rect: Rect, hidden: Rect) -> Option<Rect> {
            if rect.width == 0
                || rect.height == 0
                || rect.bottom() <= hidden.y
                || rect.y >= hidden.bottom()
            {
                return Some(rect);
            }
            if rect.y < hidden.y {
                return Some(Rect {
                    height: hidden.y - rect.y,
                    ..rect
                });
            }
            let y = hidden.bottom();
            if y < rect.bottom() {
                Some(Rect {
                    y,
                    height: rect.bottom() - y,
                    ..rect
                })
            } else {
                None
            }
        }
        let mut map = self.hit_map.borrow_mut();
        map.panels = map
            .panels
            .iter()
            .filter_map(|(r, p)| clip(*r, hidden).map(|r| (r, *p)))
            .collect();
        map.rows = map
            .rows
            .iter()
            .filter_map(|(r, t)| clip(*r, hidden).map(|r| (r, t.clone())))
            .collect();
        map.borders = map
            .borders
            .iter()
            .filter_map(|(r, b)| clip(*r, hidden).map(|r| (r, *b)))
            .collect();
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
        // Keep terminal selection by address; when it disappears, take the row that slid into its
        // previous index (the nearest neighbour) instead of jumping to the last row.
        if let Some(snap) = data.snap.as_ref() {
            let terms = ordered_terminals(snap);
            if terms.is_empty() {
                self.sel_terminal = None;
                self.prev_terminal_idx = None;
            } else if let Some(addr) = self.sel_terminal.as_deref() {
                if let Some(i) = terms.iter().position(|t| t.address == addr) {
                    self.sel_terminal = Some(terms[i].address.clone());
                    self.prev_terminal_idx = Some(i);
                } else {
                    let idx = self
                        .prev_terminal_idx
                        .unwrap_or(0)
                        .min(terms.len().saturating_sub(1));
                    self.sel_terminal = Some(terms[idx].address.clone());
                    self.prev_terminal_idx = Some(idx);
                }
            } else {
                self.sel_terminal = Some(terms[0].address.clone());
                self.prev_terminal_idx = Some(0);
            }

            let inbox = &snap.inbox;
            if inbox.is_empty() {
                self.sel_inbox = None;
                self.prev_inbox_idx = None;
            } else if let Some(id) = self.sel_inbox {
                if let Some(i) = inbox.iter().position(|r| r.id == id) {
                    self.sel_inbox = Some(inbox[i].id);
                    self.prev_inbox_idx = Some(i);
                } else {
                    let idx = self
                        .prev_inbox_idx
                        .unwrap_or(0)
                        .min(inbox.len().saturating_sub(1));
                    self.sel_inbox = Some(inbox[idx].id);
                    self.prev_inbox_idx = Some(idx);
                }
            } else {
                self.sel_inbox = Some(inbox[0].id);
                self.prev_inbox_idx = Some(0);
            }
        } else {
            // No snapshot: no rows to select.
            self.sel_terminal = None;
            self.sel_inbox = None;
            self.prev_terminal_idx = None;
            self.prev_inbox_idx = None;
        }

        if let Focus::Panel(sec) = self.focus
            && !visible_panels(data).contains(&sec)
        {
            self.focus = Focus::CmdLine;
        }
    }

    pub(crate) fn handle_popup_key(&mut self, key: KeyEvent) -> GEffect {
        if let Some(popup) = self.popup.clone() {
            self.on_key_popup(key, popup)
        } else {
            GEffect::None
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
        match self.cmd.on_key(key) {
            CmdKey::Submit(text) => GEffect::Run(text),
            CmdKey::Edited | CmdKey::SubmitEmpty | CmdKey::Esc => GEffect::None,
            CmdKey::Unhandled => match key.code {
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
                _ => GEffect::None,
            },
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

    fn panel_up(&mut self, data: &GlobalData, sec: Section) {
        match sec {
            Section::Terminals => {
                let Some(snap) = data.snap.as_ref() else {
                    return;
                };
                let terms = ordered_terminals(snap);
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
                self.prev_terminal_idx = Some(next);
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
                self.prev_inbox_idx = Some(next);
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
                let terms = ordered_terminals(snap);
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
                self.prev_terminal_idx = Some(next);
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
                self.prev_inbox_idx = Some(next);
            }
            _ => {
                let entry = self.scroll.entry(sec).or_insert(0);
                *entry = entry.saturating_add(1);
            }
        }
    }
}

fn visible_panels(data: &GlobalData) -> Vec<Section> {
    visible_sections(&data.view)
}

/// The panels the global screen shows: the chosen sections in `view` (dropping `Messages`, which
/// is pinned and never counted) followed by `Messages`. An empty or messages-only view falls back
/// to `Section::DEFAULT`, exactly as the drawing does, so `Tab` and the screen never disagree.
pub(crate) fn visible_sections(view: &[Section]) -> Vec<Section> {
    let mut sections: Vec<Section> = view
        .iter()
        .copied()
        .filter(|s| *s != Section::Messages)
        .collect();
    if sections.is_empty() {
        sections = Section::DEFAULT.to_vec();
    }
    sections.push(Section::Messages);
    sections
}

/// Apps that count as a plain shell only when no session/model is linked.
const SHELL_APPS: [&str; 7] = ["pwsh", "powershell", "bash", "sh", "zsh", "cmd", "fish"];

pub(crate) fn is_plain_shell(t: &TermView) -> bool {
    let app = sanitize_one_line(&t.app).trim().to_ascii_lowercase();
    SHELL_APPS.contains(&app.as_str())
        && sanitize_one_line(&t.model).trim().is_empty()
        && sanitize_one_line(&t.effort).trim().is_empty()
}

/// The one visual order of the Terminals panel: agents in their existing order, then plain shells
/// in their existing order. Both the renderer and ↑↓ navigation use it, so selection by address
/// keeps working while the arrows follow what the user sees.
pub fn ordered_terminals(snap: &Snapshot) -> Vec<&TermView> {
    snap.terminals
        .iter()
        .filter(|t| !is_plain_shell(t))
        .chain(snap.terminals.iter().filter(|t| is_plain_shell(t)))
        .collect()
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
    fn hit_map_prefers_borders_then_rows_then_panels() {
        let panel = Rect::new(0, 0, 20, 10);
        // The row reaches x = 20 and shares x = 19 with the border, so the priority is real.
        let row = Rect::new(1, 1, 19, 1);
        let border = Rect::new(19, 0, 1, 10);
        let map = HitMap {
            panels: vec![(panel, HitPanel::Section(Section::Terminals))],
            rows: vec![(row, RowTarget::Terminal("t1".into()))],
            borders: vec![(border, BorderId::TopPairVertical)],
        };
        assert_eq!(
            hit(&map, 5, 5),
            Some(Hit::Panel(HitPanel::Section(Section::Terminals)))
        );
        assert_eq!(
            hit(&map, 5, 1),
            Some(Hit::Row(RowTarget::Terminal("t1".into())))
        );
        assert_eq!(
            hit(&map, 18, 1),
            Some(Hit::Row(RowTarget::Terminal("t1".into()))),
            "the row is still reachable just before the border"
        );
        assert_eq!(
            hit(&map, 19, 1),
            Some(Hit::Border(BorderId::TopPairVertical)),
            "border wins where the row and the border overlap"
        );
        assert_eq!(hit(&map, 20, 1), None, "the right edge is outside");
        assert_eq!(hit(&map, 0, 10), None, "the bottom edge is outside");
        assert_eq!(hit(&HitMap::default(), 0, 0), None);
    }

    #[test]
    fn ordered_terminals_puts_plain_shells_last() {
        let shell = |address: &str, app: &str| TermView {
            app: app.into(),
            model: String::new(),
            effort: String::new(),
            ..term(address, address)
        };
        let linked_shell = TermView {
            model: "opus".into(),
            ..shell("linked", "bash")
        };
        let snap = snap(
            vec![
                shell("s1", "bash"),
                term("a1", "agent-1"),
                shell("s2", "POWERSHELL"),
                linked_shell,
                term("a2", "agent-2"),
            ],
            vec![],
        );
        let addresses: Vec<&str> = ordered_terminals(&snap)
            .iter()
            .map(|t| t.address.as_str())
            .collect();
        assert_eq!(
            addresses,
            vec!["a1", "linked", "a2", "s1", "s2"],
            "agents first, plain shells last, original order kept inside each group"
        );
    }

    #[test]
    fn down_follows_the_visual_terminal_order() {
        let shell = TermView {
            app: "bash".into(),
            model: String::new(),
            effort: String::new(),
            ..term("s1", "shell-1")
        };
        let data = data_with(
            vec![Section::Terminals],
            vec![shell, term("a1", "agent-1"), term("a2", "agent-2")],
            vec![],
        );
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        st.focus = Focus::Panel(Section::Terminals);
        st.sel_terminal = Some("a1".into());
        st.sync(&data);

        st.panel_down(&data, Section::Terminals);
        assert_eq!(st.sel_terminal.as_deref(), Some("a2"));
        st.panel_down(&data, Section::Terminals);
        assert_eq!(st.sel_terminal.as_deref(), Some("s1"));
        st.panel_up(&data, Section::Terminals);
        assert_eq!(st.sel_terminal.as_deref(), Some("a2"));
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
    fn pgdn_scrolls_output_from_cmdline() {
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        let mut data = GlobalData {
            output: Some((0..40).map(|i| format!("line-{i:02}\n")).collect()),
            ..GlobalData::default()
        };
        assert_eq!(st.focus, Focus::CmdLine);

        st.on_key(key(KeyCode::PageDown), &data);
        assert_eq!(st.output_scroll, 5, "PgDn must scroll the Output panel");
        st.on_key(key(KeyCode::PageDown), &data);
        assert_eq!(st.output_scroll, 10);
        st.on_key(key(KeyCode::PageUp), &data);
        assert_eq!(st.output_scroll, 5);

        // No Output panel open: paging does nothing.
        data.output = None;
        st.on_key(key(KeyCode::PageDown), &data);
        assert_eq!(st.output_scroll, 5);
    }

    #[test]
    fn tab_matches_drawn_panels_for_empty_and_messages_only_view() {
        for view in [Vec::new(), vec![Section::Messages]] {
            let data = data_with(view.clone(), vec![], vec![]);
            let expected = visible_sections(&view);
            assert_eq!(expected.last(), Some(&Section::Messages));
            assert!(
                expected.len() > 1,
                "an empty or messages-only view must fall back to the drawn defaults"
            );

            let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
            let mut seen = Vec::new();
            loop {
                st.on_key(key(KeyCode::Tab), &data);
                match st.focus {
                    Focus::Panel(sec) => seen.push(sec),
                    Focus::CmdLine => break,
                }
            }
            assert_eq!(seen, expected, "Tab must visit exactly the drawn panels");
        }
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
    fn sync_selects_neighbour_after_middle_removal() {
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        let mut data = data_with(
            vec![Section::Inbox],
            vec![],
            (1..=5).map(inbox).collect::<Vec<_>>(),
        );
        st.sel_inbox = Some(3);
        st.sync(&data);
        // Rows 1..5 with 3 selected; 3 closes, the row that took its place (4) is selected.
        data.snap.as_mut().unwrap().inbox = vec![inbox(1), inbox(2), inbox(4), inbox(5)];
        st.sync(&data);
        assert_eq!(st.sel_inbox, Some(4));
    }

    #[test]
    fn sync_clears_closed_inbox_selection() {
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        let mut data = data_with(
            vec![Section::Inbox],
            vec![],
            vec![inbox(1), inbox(2), inbox(3)],
        );
        st.sel_inbox = Some(2);
        st.sync(&data);
        // The `d` command closed item 2: after the refresh it is gone and no longer selected.
        data.snap.as_mut().unwrap().inbox = vec![inbox(1), inbox(3)];
        st.sync(&data);
        assert_ne!(
            st.sel_inbox,
            Some(2),
            "selection still points at the closed item"
        );
        assert_eq!(st.sel_inbox, Some(3));
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
