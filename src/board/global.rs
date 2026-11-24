//! Global screen state of the full-screen `agm tui`: focus, command line, popup, selections.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fmt;

use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

use super::cmd::NoteKind;
use super::global_view::{MESSAGES_MIN, MIN_PANEL_W, MIN_SECTION_H, WIDE_MIN_COLS};
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

/// Settings key the overview's panel proportions are saved under.
pub const LAYOUT_SETTING_KEY: &str = "tui_layout";

/// The overview's panel proportions, as ratios so a window resize recomputes the layout instead of
/// replaying stale row counts. Empty means "everything content-fitted".
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LayoutOverride {
    /// Fraction of the section area width given to the left panel of the top pair.
    pub top_pair_width: Option<f32>,
    /// `BorderId::Horizontal(i)` → fraction of the section area height given to the group
    /// directly above that boundary.
    pub heights: BTreeMap<usize, f32>,
}

impl LayoutOverride {
    pub fn is_empty(&self) -> bool {
        self.top_pair_width.is_none() && self.heights.is_empty()
    }

    pub fn clear(&mut self) {
        self.top_pair_width = None;
        self.heights.clear();
    }

    /// Removes the override of one boundary; the other boundaries stay as they are.
    pub fn reset(&mut self, border: BorderId) {
        match border {
            BorderId::TopPairVertical => self.top_pair_width = None,
            BorderId::Horizontal(i) => {
                self.heights.remove(&i);
            }
            BorderId::OutputTop => {}
        }
    }

    pub fn set_top_pair_width(&mut self, fraction: f32) {
        if fraction.is_finite() {
            self.top_pair_width = Some(fraction.clamp(0.0, 1.0));
        }
    }

    pub fn set_height(&mut self, boundary: usize, fraction: f32) {
        if fraction.is_finite() {
            self.heights.insert(boundary, fraction.clamp(0.0, 1.0));
        }
    }

    pub fn height(&self, boundary: usize) -> Option<f32> {
        self.heights.get(&boundary).copied()
    }
}

/// Compact persisted form: `w=<fraction>` plus `h<boundary>=<fraction>` entries, `;` separated.
/// Four decimals are enough for a drag and keep the setting short.
impl fmt::Display for LayoutOverride {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        if let Some(width) = self.top_pair_width {
            write!(f, "w={width:.4}")?;
            first = false;
        }
        for (&boundary, &fraction) in &self.heights {
            if !first {
                write!(f, ";")?;
            }
            write!(f, "h{boundary}={fraction:.4}")?;
            first = false;
        }
        Ok(())
    }
}

impl LayoutOverride {
    /// Reads the compact form back; anything malformed (unknown key, NaN, out of range, duplicate)
    /// is ignored as a whole so a corrupt setting can never produce a broken layout.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            return Some(Self::default());
        }
        let mut layout = Self::default();
        for field in text.split(';') {
            let (key, value) = field.split_once('=')?;
            let key = key.trim();
            let value: f32 = value.trim().parse().ok()?;
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return None;
            }
            if key == "w" {
                if layout.top_pair_width.is_some() {
                    return None;
                }
                layout.top_pair_width = Some(value);
            } else {
                let index: usize = key.strip_prefix('h')?.parse().ok()?;
                if layout.heights.insert(index, value).is_some() {
                    return None;
                }
            }
        }
        Some(layout)
    }
}

/// A panel recorded by the renderer. Output is an overlay, not one of the view's `Section`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitPanel {
    Section(Section),
    Output,
    /// The command line below the sections.
    CmdLine,
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

/// The section area (main content without the command bar and footer) as the last frame drew it:
/// the union of its section panels. Output and the command line are not part of it.
fn content_area(map: &HitMap) -> Option<Rect> {
    let mut rects = map
        .panels
        .iter()
        .filter(|(rect, panel)| {
            matches!(panel, HitPanel::Section(_)) && rect.width > 0 && rect.height > 0
        })
        .map(|(rect, _)| *rect);
    let first = rects.next()?;
    Some(rects.fold(first, |a, b| {
        let right = a.right().max(b.right());
        let bottom = a.bottom().max(b.bottom());
        Rect {
            x: a.x.min(b.x),
            y: a.y.min(b.y),
            width: right.saturating_sub(a.x.min(b.x)),
            height: bottom.saturating_sub(a.y.min(b.y)),
        }
    }))
}

#[derive(Debug, Clone, PartialEq)]
pub enum GEffect {
    None,
    /// A command line for the dispatcher.
    Run(String),
    /// Open the board of this terminal's project (terminal address).
    OpenBoard(String),
    SaveTheme(ThemeName),
    /// Persist these panel proportions under `LAYOUT_SETTING_KEY` (empty = reset).
    SaveLayout(String),
    /// Show the overview's keyboard/mouse help in the Output panel.
    ShowHelp,
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
    /// Panel proportions chosen by dragging; empty means content-fitted defaults.
    pub layout: LayoutOverride,
    /// Index of `sel_terminal` in the last snapshot, so a removed row keeps its neighbour.
    prev_terminal_idx: Option<usize>,
    /// Index of `sel_inbox` in the last snapshot, so a removed row keeps its neighbour.
    prev_inbox_idx: Option<usize>,
    /// Last left-button down: the target and its clock, so the next down on the same target within
    /// `DOUBLE_CLICK_MS` is the second half of a double-click.
    last_click: Option<(Hit, i64)>,
    /// The boundary drag in progress, if any.
    drag: Option<DragState>,
    /// Rebuilt by `draw_global` every frame; read by the app loop after drawing.
    hit_map: RefCell<HitMap>,
}

/// An armed (or in-progress) drag of one boundary. `area` is the section area of the last frame,
/// `start` the first row/column of the group that boundary resizes, so every drag recomputes the
/// ratio from the pointer instead of replaying row counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DragState {
    border: BorderId,
    area: Rect,
    start: u16,
    /// Whether this drag moved the boundary at least once (so Up has something to persist).
    changed: bool,
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
            layout: LayoutOverride::default(),
            prev_terminal_idx: None,
            prev_inbox_idx: None,
            last_click: None,
            drag: None,
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
        // A zero-width column (e.g. a long Quota title taking the whole top pair) or a dropped
        // zero-height section is not on screen, so it must not be hit-testable either.
        if rect.width == 0 || rect.height == 0 {
            return;
        }
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

    const DOUBLE_CLICK_MS: i64 = 400;

    /// One mouse event on the overview: (did the visible state change, effect for the app loop).
    ///
    /// The hit map is the one the last frame drew, so the app tests without a terminal. Only the
    /// left button and the wheel act; everything else is ignored, and so is every event while a
    /// confirmation popup is open or the Output overlay covers the sections.
    pub fn on_mouse(&mut self, ev: MouseEvent, data: &GlobalData, now_ms: i64) -> (bool, GEffect) {
        if self.popup.is_some() {
            // A popup is keyboard-only: forget earlier clicks so closing it cannot pair with one,
            // and drop any half-finished drag so it cannot resume underneath the popup.
            self.last_click = None;
            self.drag = None;
            return (false, GEffect::None);
        }
        match ev.kind {
            MouseEventKind::Down(MouseButton::Left)
            | MouseEventKind::Drag(MouseButton::Left)
            | MouseEventKind::Up(MouseButton::Left)
            | MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown => {}
            _ => return (false, GEffect::None),
        }

        let map = self.hit_map();
        let target = hit(&map, ev.column, ev.row);

        if data.output.is_some() {
            // The overlay is modal: only its own panel scrolls (its top border is not resizable).
            // A click or drag armed under it must not come back when the panel closes.
            self.last_click = None;
            self.drag = None;
            return match (ev.kind, target) {
                (MouseEventKind::ScrollUp, Some(Hit::Panel(HitPanel::Output))) => {
                    let before = self.output_scroll;
                    self.output_scroll = self.output_scroll.saturating_sub(1);
                    (self.output_scroll != before, GEffect::None)
                }
                (MouseEventKind::ScrollDown, Some(Hit::Panel(HitPanel::Output))) => {
                    let before = self.output_scroll;
                    self.output_scroll = self.output_scroll.saturating_add(1);
                    (self.output_scroll != before, GEffect::None)
                }
                _ => (false, GEffect::None),
            };
        }

        match ev.kind {
            MouseEventKind::ScrollUp => (self.on_wheel(target, -1, data), GEffect::None),
            MouseEventKind::ScrollDown => (self.on_wheel(target, 1, data), GEffect::None),
            MouseEventKind::Down(MouseButton::Left) => {
                self.on_left_down(target, &map, data, now_ms)
            }
            MouseEventKind::Drag(MouseButton::Left) => self.on_drag(ev.column, ev.row),
            MouseEventKind::Up(MouseButton::Left) => self.end_drag(),
            _ => (false, GEffect::None),
        }
    }

    /// The panel under the pointer takes the wheel: the two tables move their selection and every
    /// other section (Messages included) scrolls. A border, the command line and the footer do
    /// nothing.
    fn on_wheel(&mut self, target: Option<Hit>, delta: i32, data: &GlobalData) -> bool {
        let Some(target) = target else {
            return false;
        };
        let sec = match target {
            Hit::Panel(HitPanel::Section(sec)) => sec,
            Hit::Row(RowTarget::Terminal(_)) => Section::Terminals,
            Hit::Row(RowTarget::Inbox(_)) => Section::Inbox,
            Hit::Row(RowTarget::Message(_)) => Section::Messages,
            // The board's row targets never appear on the overview map.
            Hit::Row(RowTarget::Agent(_) | RowTarget::Project(_)) => return false,
            Hit::Panel(HitPanel::Output | HitPanel::CmdLine) | Hit::Border(_) => return false,
        };
        match sec {
            Section::Terminals => {
                let before = self.sel_terminal.clone();
                if delta < 0 {
                    self.panel_up(data, sec);
                } else {
                    self.panel_down(data, sec);
                }
                self.sel_terminal != before
            }
            Section::Inbox => {
                let before = self.sel_inbox;
                if delta < 0 {
                    self.panel_up(data, sec);
                } else {
                    self.panel_down(data, sec);
                }
                self.sel_inbox != before
            }
            _ => {
                let entry = self.scroll.entry(sec).or_insert(0);
                let before = *entry;
                *entry = if delta < 0 {
                    entry.saturating_sub(1)
                } else {
                    entry.saturating_add(1)
                };
                *entry != before
            }
        }
    }

    fn on_left_down(
        &mut self,
        target: Option<Hit>,
        map: &HitMap,
        data: &GlobalData,
        now_ms: i64,
    ) -> (bool, GEffect) {
        let Some(target) = target else {
            self.last_click = None;
            self.drag = None;
            return (false, GEffect::None);
        };
        let is_double = self.last_click.as_ref().is_some_and(|(prev, ts)| {
            *prev == target && now_ms.saturating_sub(*ts) <= Self::DOUBLE_CLICK_MS
        });
        self.last_click = Some((target.clone(), now_ms));
        if is_double {
            // A double-click consumes the pair.
            self.last_click = None;
            self.drag = None;
            return self.on_double_click(target, data);
        }
        match target {
            Hit::Panel(HitPanel::Section(sec)) => {
                self.drag = None;
                (self.set_focus(Focus::Panel(sec)), GEffect::None)
            }
            Hit::Panel(HitPanel::CmdLine) => {
                self.drag = None;
                (self.set_focus(Focus::CmdLine), GEffect::None)
            }
            Hit::Panel(HitPanel::Output) => {
                self.drag = None;
                (false, GEffect::None)
            }
            Hit::Row(row) => {
                self.drag = None;
                self.click_row(row, data)
            }
            Hit::Border(border) => {
                // A press on a boundary arms a drag; the frame only changes on the first Drag.
                self.start_drag(border, map, data);
                (false, GEffect::None)
            }
        }
    }

    /// Arms a drag on a boundary that can move. `OutputTop` is not resizable and boundaries in a
    /// window too small for their floors are ignored instead of starting a no-op drag.
    fn start_drag(&mut self, border: BorderId, map: &HitMap, data: &GlobalData) {
        self.drag = None;
        if border == BorderId::OutputTop {
            return;
        }
        let Some(area) = content_area(map) else {
            return;
        };
        let Some(border_rect) = map
            .borders
            .iter()
            .find_map(|(rect, id)| (*id == border).then_some(*rect))
        else {
            return;
        };
        // Mirror `section_layout`: the Messages slot is not a resizable group, and the top pair
        // folds the first two sections into one group only on a wide frame.
        let mut chosen = visible_sections(&data.view);
        chosen.pop();
        let wide = area.width >= WIDE_MIN_COLS && chosen.len() >= 2;
        let groups = chosen.len().saturating_sub(usize::from(wide));
        let start = match border {
            BorderId::TopPairVertical => {
                if area.width < MIN_PANEL_W.saturating_mul(2) {
                    return;
                }
                area.x
            }
            BorderId::Horizontal(_) => {
                // The exact floor `apply_height_overrides` uses: a window too short for every
                // group's minimum plus Messages ignores the boundary instead of storing a ratio
                // the renderer would never apply.
                let budget = area.height.saturating_sub(MESSAGES_MIN);
                if budget < MIN_SECTION_H.saturating_mul(groups as u16) {
                    return;
                }
                // The group above ends on the border's row; its first panel row is the anchor.
                let top = map
                    .panels
                    .iter()
                    .filter(|(rect, panel)| {
                        matches!(panel, HitPanel::Section(_))
                            && rect.y < border_rect.y
                            && rect.bottom() == border_rect.y
                    })
                    .map(|(rect, _)| rect.y)
                    .min();
                match top {
                    Some(top) => top,
                    None => return,
                }
            }
            BorderId::OutputTop => return,
        };
        self.drag = Some(DragState {
            border,
            area,
            start,
            changed: false,
        });
    }

    /// One drag event: the boundary follows the pointer as a ratio of the section area. Returns
    /// whether this event visibly moved anything.
    fn on_drag(&mut self, column: u16, row: u16) -> (bool, GEffect) {
        let Some(drag) = self.drag.as_mut() else {
            return (false, GEffect::None);
        };
        // A drag is not a click: the next Down must start a fresh press instead of pairing with
        // the Down that armed this drag (which would reset the boundary just resized).
        self.last_click = None;
        let fraction = match drag.border {
            BorderId::TopPairVertical => {
                let left = column
                    .saturating_sub(drag.area.x)
                    .clamp(MIN_PANEL_W, drag.area.width - MIN_PANEL_W);
                f32::from(left) / f32::from(drag.area.width)
            }
            BorderId::Horizontal(_) => {
                let height = row
                    .saturating_sub(drag.start)
                    .max(MIN_SECTION_H)
                    .min(drag.area.height.saturating_sub(MESSAGES_MIN));
                f32::from(height) / f32::from(drag.area.height)
            }
            BorderId::OutputTop => return (false, GEffect::None),
        };
        let changed = match drag.border {
            BorderId::TopPairVertical => self.layout.top_pair_width != Some(fraction),
            BorderId::Horizontal(i) => self.layout.height(i) != Some(fraction),
            BorderId::OutputTop => false,
        };
        if changed {
            match drag.border {
                BorderId::TopPairVertical => self.layout.set_top_pair_width(fraction),
                BorderId::Horizontal(i) => self.layout.set_height(i, fraction),
                BorderId::OutputTop => {}
            }
            drag.changed = true;
        }
        (changed, GEffect::None)
    }

    /// Ends a drag; if it moved the boundary, the app persists the new ratios.
    fn end_drag(&mut self) -> (bool, GEffect) {
        match self.drag.take() {
            Some(drag) if drag.changed => (false, GEffect::SaveLayout(self.layout.to_string())),
            _ => (false, GEffect::None),
        }
    }

    fn set_focus(&mut self, focus: Focus) -> bool {
        if self.focus == focus {
            return false;
        }
        self.focus = focus;
        true
    }

    fn click_row(&mut self, row: RowTarget, data: &GlobalData) -> (bool, GEffect) {
        let before = (self.focus, self.sel_terminal.clone(), self.sel_inbox);
        match row {
            RowTarget::Terminal(address) => {
                self.prev_terminal_idx = data.snap.as_ref().and_then(|snap| {
                    ordered_terminals(snap)
                        .iter()
                        .position(|t| t.address == address)
                });
                self.sel_terminal = Some(address);
                self.focus = Focus::Panel(Section::Terminals);
            }
            RowTarget::Inbox(id) => {
                self.prev_inbox_idx = data
                    .snap
                    .as_ref()
                    .and_then(|snap| snap.inbox.iter().position(|r| r.id == id));
                self.sel_inbox = Some(id);
                self.focus = Focus::Panel(Section::Inbox);
            }
            RowTarget::Message(_) => self.focus = Focus::Panel(Section::Messages),
            RowTarget::Agent(_) | RowTarget::Project(_) => return (false, GEffect::None),
        }
        (
            before != (self.focus, self.sel_terminal.clone(), self.sel_inbox),
            GEffect::None,
        )
    }

    fn on_double_click(&mut self, target: Hit, data: &GlobalData) -> (bool, GEffect) {
        self.drag = None;
        match target {
            Hit::Row(RowTarget::Terminal(address)) => {
                // Same effect as ⏎ on the Terminals panel.
                self.click_row(RowTarget::Terminal(address.clone()), data);
                (true, GEffect::OpenBoard(address))
            }
            Hit::Row(RowTarget::Inbox(id)) => {
                // Same pre-fill as `r`: nothing is dispatched.
                self.click_row(RowTarget::Inbox(id), data);
                self.focus_cmdline_with(format!("reply {id} "));
                (true, GEffect::None)
            }
            Hit::Border(border) => {
                let stored = match border {
                    BorderId::TopPairVertical => self.layout.top_pair_width.is_some(),
                    BorderId::Horizontal(i) => self.layout.heights.contains_key(&i),
                    BorderId::OutputTop => false,
                };
                if !stored {
                    return (false, GEffect::None);
                }
                self.layout.reset(border);
                (true, GEffect::SaveLayout(self.layout.to_string()))
            }
            _ => (false, GEffect::None),
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
            KeyCode::Char('?') => GEffect::ShowHelp,
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

    use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn down(column: u16, row: u16) -> MouseEvent {
        mouse(MouseEventKind::Down(MouseButton::Left), column, row)
    }

    fn drag(column: u16, row: u16) -> MouseEvent {
        mouse(MouseEventKind::Drag(MouseButton::Left), column, row)
    }

    fn up(column: u16, row: u16) -> MouseEvent {
        mouse(MouseEventKind::Up(MouseButton::Left), column, row)
    }

    /// A manual overview map: Terminals | Inbox on top, Messages, then the command line.
    fn overview_map(st: &GlobalState) {
        st.push_panel(Rect::new(0, 0, 40, 10), Section::Terminals);
        st.push_row(Rect::new(1, 1, 30, 1), RowTarget::Terminal("t1".into()));
        st.push_row(Rect::new(1, 2, 30, 1), RowTarget::Terminal("t2".into()));
        st.push_panel(Rect::new(40, 0, 20, 10), Section::Inbox);
        st.push_row(Rect::new(41, 1, 10, 1), RowTarget::Inbox(7));
        st.push_row(Rect::new(41, 2, 10, 1), RowTarget::Inbox(8));
        st.push_panel(Rect::new(0, 10, 60, 5), Section::Messages);
        st.push_row(Rect::new(1, 10, 50, 2), RowTarget::Message(0));
        st.push_panel(Rect::new(0, 15, 60, 1), HitPanel::CmdLine);
    }

    fn overview_state() -> (GlobalState, GlobalData) {
        let data = data_with(
            vec![Section::Terminals, Section::Inbox],
            vec![term("t1", "worker-1"), term("t2", "worker-2")],
            vec![inbox(7), inbox(8)],
        );
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        st.sync(&data);
        overview_map(&st);
        (st, data)
    }

    #[test]
    fn mouse_click_focuses_panel_rows_and_cmdline() {
        let (mut st, data) = overview_state();
        assert_eq!(st.focus, Focus::CmdLine);

        let (changed, eff) = st.on_mouse(down(5, 5), &data, 1_000);
        assert!(changed);
        assert_eq!(eff, GEffect::None);
        assert_eq!(st.focus, Focus::Panel(Section::Terminals));

        // Clicking the already focused panel is not a change.
        let (changed, _) = st.on_mouse(down(5, 5), &data, 2_000);
        assert!(!changed);

        // A Terminals row focuses the panel and selects exactly that row.
        let (changed, eff) = st.on_mouse(down(5, 2), &data, 3_000);
        assert!(changed);
        assert_eq!(eff, GEffect::None);
        assert_eq!(st.focus, Focus::Panel(Section::Terminals));
        assert_eq!(st.sel_terminal.as_deref(), Some("t2"));

        // An Inbox row does the same for the inbox.
        let (changed, _) = st.on_mouse(down(45, 1), &data, 4_000);
        assert!(changed);
        assert_eq!(st.focus, Focus::Panel(Section::Inbox));
        assert_eq!(st.sel_inbox, Some(7));

        // A Messages row only focuses Messages (no row selection is kept there).
        let (changed, _) = st.on_mouse(down(5, 10), &data, 5_000);
        assert!(changed);
        assert_eq!(st.focus, Focus::Panel(Section::Messages));

        // The command line below the sections takes the focus back.
        let (changed, _) = st.on_mouse(down(5, 15), &data, 6_000);
        assert!(changed);
        assert_eq!(st.focus, Focus::CmdLine);
    }

    #[test]
    fn mouse_wheel_moves_selection_and_scrolls() {
        let (mut st, data) = overview_state();

        // Terminals: one row per notch, clamped at the ends.
        let (changed, eff) = st.on_mouse(mouse(MouseEventKind::ScrollDown, 5, 1), &data, 1_000);
        assert!(changed);
        assert_eq!(eff, GEffect::None);
        assert_eq!(st.sel_terminal.as_deref(), Some("t2"));
        let (changed, _) = st.on_mouse(mouse(MouseEventKind::ScrollDown, 5, 1), &data, 2_000);
        assert!(!changed, "the selection is already on the last row");
        let (changed, _) = st.on_mouse(mouse(MouseEventKind::ScrollUp, 5, 1), &data, 3_000);
        assert!(changed);
        assert_eq!(st.sel_terminal.as_deref(), Some("t1"));

        // Inbox rows move the inbox selection, whichever row the pointer sits on.
        let (changed, _) = st.on_mouse(mouse(MouseEventKind::ScrollDown, 45, 2), &data, 4_000);
        assert!(changed);
        assert_eq!(st.sel_inbox, Some(8));

        // Messages scrolls by lines and stops at the top.
        let (changed, _) = st.on_mouse(mouse(MouseEventKind::ScrollDown, 5, 11), &data, 5_000);
        assert!(changed);
        assert_eq!(st.scroll.get(&Section::Messages).copied(), Some(1));
        let (changed, _) = st.on_mouse(mouse(MouseEventKind::ScrollUp, 5, 11), &data, 6_000);
        assert!(changed);
        assert_eq!(st.scroll.get(&Section::Messages).copied(), Some(0));
        let (changed, _) = st.on_mouse(mouse(MouseEventKind::ScrollUp, 5, 11), &data, 7_000);
        assert!(!changed, "scrolling past the top is not a change");

        // A wheel outside every panel (footer) or on the command line changes nothing.
        let (changed, _) = st.on_mouse(mouse(MouseEventKind::ScrollDown, 59, 17), &data, 8_000);
        assert!(!changed);
        let (changed, _) = st.on_mouse(mouse(MouseEventKind::ScrollDown, 5, 15), &data, 9_000);
        assert!(!changed);

        // A wheel on a border is not a panel either.
        st.push_border(Rect::new(19, 0, 2, 10), BorderId::TopPairVertical);
        let (changed, _) = st.on_mouse(mouse(MouseEventKind::ScrollDown, 19, 5), &data, 10_000);
        assert!(!changed);
    }

    #[test]
    fn mouse_double_click_opens_board_and_prefills_reply() {
        // Two downs on the same terminal row within 400 ms open its board.
        let (mut st, data) = overview_state();
        let (changed, eff) = st.on_mouse(down(5, 1), &data, 1_000);
        assert!(changed);
        assert_eq!(eff, GEffect::None);
        let (changed, eff) = st.on_mouse(down(5, 1), &data, 1_300);
        assert!(changed);
        assert_eq!(eff, GEffect::OpenBoard("t1".into()));

        // An inbox row pre-fills `reply <id> ` and focuses the command line; nothing runs.
        let (mut st, data) = overview_state();
        st.on_mouse(down(45, 1), &data, 1_000);
        let (changed, eff) = st.on_mouse(down(45, 1), &data, 1_200);
        assert!(changed);
        assert_eq!(eff, GEffect::None);
        assert_eq!(st.focus, Focus::CmdLine);
        assert_eq!(st.cmd.text, "reply 7 ");
        assert_eq!(st.cmd.cursor, "reply 7 ".chars().count());
    }

    #[test]
    fn mouse_double_click_window_edges() {
        // 401 ms apart is two plain clicks, never a double-click.
        let (mut st, data) = overview_state();
        st.on_mouse(down(5, 1), &data, 1_000);
        let (_, eff) = st.on_mouse(down(5, 1), &data, 1_401);
        assert_eq!(eff, GEffect::None);

        // Two different targets inside the window stay two plain clicks.
        let (mut st, data) = overview_state();
        st.on_mouse(down(5, 1), &data, 1_000);
        let (changed, eff) = st.on_mouse(down(5, 2), &data, 1_100);
        assert!(changed);
        assert_eq!(eff, GEffect::None);
        assert_eq!(st.sel_terminal.as_deref(), Some("t2"));

        // A double-click consumes the pair: the third down is a plain click again.
        let (mut st, data) = overview_state();
        st.on_mouse(down(5, 1), &data, 1_000);
        st.on_mouse(down(5, 1), &data, 1_100);
        let (_, eff) = st.on_mouse(down(5, 1), &data, 1_150);
        assert_eq!(eff, GEffect::None);
    }

    #[test]
    fn mouse_ignored_while_popup_or_output_overlay() {
        // Every event is ignored while a confirmation popup is open.
        let (mut st, data) = overview_state();
        st.popup = Some(Popup::Confirm {
            prompt: "kill t1?".into(),
            input: "y".into(),
        });
        let before = (
            st.focus,
            st.sel_terminal.clone(),
            st.sel_inbox,
            st.cmd.text.clone(),
            st.output_scroll,
            st.popup.clone(),
        );
        for kind in [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Down(MouseButton::Right),
            MouseEventKind::Up(MouseButton::Left),
            MouseEventKind::Drag(MouseButton::Left),
            MouseEventKind::Moved,
            MouseEventKind::ScrollUp,
            MouseEventKind::ScrollDown,
        ] {
            for (column, row) in [(5, 5), (5, 1), (45, 1), (5, 10), (5, 15)] {
                let (changed, eff) = st.on_mouse(mouse(kind, column, row), &data, 1_000);
                assert!(
                    !changed,
                    "{kind:?} at {column},{row} changed the popup screen"
                );
                assert_eq!(eff, GEffect::None);
            }
        }
        assert_eq!(
            (
                st.focus,
                st.sel_terminal.clone(),
                st.sel_inbox,
                st.cmd.text.clone(),
                st.output_scroll,
                st.popup.clone(),
            ),
            before
        );

        // While the Output overlay is shown only that panel and its border react.
        let data = data_with(
            vec![Section::Terminals],
            vec![term("t1", "worker-1")],
            vec![],
        );
        let mut data = data;
        data.output = Some("cleanup 1 t2".into());
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        st.sync(&data);
        st.push_panel(Rect::new(0, 0, 60, 10), Section::Terminals);
        st.push_row(Rect::new(1, 1, 50, 1), RowTarget::Terminal("t1".into()));
        st.push_panel(Rect::new(0, 10, 60, 5), HitPanel::Output);
        st.push_border(Rect::new(0, 10, 60, 1), BorderId::OutputTop);
        st.push_panel(Rect::new(0, 15, 60, 1), HitPanel::CmdLine);
        let st = &mut st;

        // Clicks elsewhere (including the command line) are ignored.
        for (column, row) in [(5, 5), (5, 15)] {
            let (changed, eff) = st.on_mouse(down(column, row), &data, 2_000);
            assert!(!changed, "a click under the overlay reached {column},{row}");
            assert_eq!(eff, GEffect::None);
        }
        assert_eq!(st.focus, Focus::CmdLine);

        // The Output panel scrolls with the wheel; its border is not a scroll target.
        let (changed, eff) = st.on_mouse(mouse(MouseEventKind::ScrollDown, 5, 12), &data, 3_000);
        assert!(changed);
        assert_eq!(eff, GEffect::None);
        assert_eq!(st.output_scroll, 1);
        let (changed, _) = st.on_mouse(mouse(MouseEventKind::ScrollUp, 5, 12), &data, 4_000);
        assert!(changed);
        assert_eq!(st.output_scroll, 0);
        let (changed, _) = st.on_mouse(mouse(MouseEventKind::ScrollUp, 5, 10), &data, 5_000);
        assert!(!changed, "the Output border must not scroll");

        // The Output border is not a drag handle: Down → Drag → Up is ignored.
        for (kind, now) in [
            (MouseEventKind::Down(MouseButton::Left), 6_000),
            (MouseEventKind::Drag(MouseButton::Left), 6_100),
            (MouseEventKind::Up(MouseButton::Left), 6_200),
        ] {
            let (changed, eff) = st.on_mouse(mouse(kind, 5, 10), &data, now);
            assert!(!changed, "{kind:?} on the Output border moved something");
            assert_eq!(eff, GEffect::None);
        }
    }

    #[test]
    fn mouse_ignores_non_left_and_moved_events() {
        let (mut st, data) = overview_state();
        let before = (st.focus, st.sel_terminal.clone(), st.sel_inbox);
        for kind in [
            MouseEventKind::Moved,
            MouseEventKind::Up(MouseButton::Left),
            MouseEventKind::Drag(MouseButton::Left),
            MouseEventKind::Down(MouseButton::Right),
            MouseEventKind::Down(MouseButton::Middle),
            MouseEventKind::Up(MouseButton::Right),
            MouseEventKind::Drag(MouseButton::Right),
        ] {
            for (column, row) in [(5, 5), (5, 1), (45, 1), (5, 15)] {
                let (changed, eff) = st.on_mouse(mouse(kind, column, row), &data, 1_000);
                assert!(!changed, "{kind:?} at {column},{row} forced a redraw");
                assert_eq!(eff, GEffect::None);
            }
        }
        assert_eq!((st.focus, st.sel_terminal.clone(), st.sel_inbox), before);
    }

    #[test]
    fn layout_override_parse_roundtrip_and_garbage() {
        let empty = LayoutOverride::default();
        assert_eq!(empty.to_string(), "");
        assert_eq!(LayoutOverride::parse(""), Some(empty.clone()));
        assert_eq!(LayoutOverride::parse("   "), Some(empty));

        let mut layout = LayoutOverride {
            top_pair_width: Some(0.625),
            ..Default::default()
        };
        layout.heights.insert(2, 0.25);
        layout.heights.insert(3, 0.375);
        let text = layout.to_string();
        assert_eq!(text, "w=0.6250;h2=0.2500;h3=0.3750");
        assert_eq!(LayoutOverride::parse(&text), Some(layout));

        // Anything that is not `w=<fraction>` / `h<index>=<fraction>` is ignored, never half-read.
        for garbage in [
            "nonsense",
            "0.5",
            "w",
            "w=abc",
            "w=0.5;oops",
            "h=0.5",
            "hx=0.5",
            "h2=x",
            "h2=nan",
            "h2=inf",
            "h2=-0.1",
            "h2=1.5",
            "x=0.5",
            "w=0.5;w=0.6",
            "h2=0.5;h2=0.6",
        ] {
            assert_eq!(
                LayoutOverride::parse(garbage),
                None,
                "{garbage:?} must be ignored"
            );
        }
    }

    #[test]
    fn layout_override_resets_only_the_named_boundary() {
        let mut layout = LayoutOverride {
            top_pair_width: Some(0.5),
            ..Default::default()
        };
        layout.heights.insert(2, 0.3);
        layout.heights.insert(3, 0.4);

        layout.reset(BorderId::OutputTop);
        assert_eq!(layout.top_pair_width, Some(0.5), "OutputTop is not stored");
        assert_eq!(layout.heights.len(), 2);

        layout.reset(BorderId::Horizontal(2));
        assert_eq!(layout.top_pair_width, Some(0.5));
        assert_eq!(layout.heights.keys().copied().collect::<Vec<_>>(), vec![3]);

        layout.reset(BorderId::TopPairVertical);
        assert_eq!(layout.top_pair_width, None);
        assert_eq!(layout.heights.len(), 1);

        layout.clear();
        assert!(layout.is_empty());
    }

    fn drag_map(st: &GlobalState) {
        st.push_panel(Rect::new(0, 0, 60, 10), Section::Quota);
        st.push_panel(Rect::new(0, 10, 60, 10), Section::Inbox);
        st.push_panel(Rect::new(0, 20, 60, 5), Section::Messages);
        st.push_border(Rect::new(0, 10, 60, 1), BorderId::Horizontal(2));
    }

    fn drag_map_vertical(st: &GlobalState) {
        st.push_panel(Rect::new(0, 0, 50, 10), Section::Quota);
        st.push_panel(Rect::new(50, 0, 50, 10), Section::Terminals);
        st.push_panel(Rect::new(0, 10, 100, 5), Section::Messages);
        st.push_border(Rect::new(49, 0, 2, 10), BorderId::TopPairVertical);
    }

    #[test]
    fn mouse_drag_horizontal_resizes_and_clamps() {
        let data = data_with(vec![Section::Quota, Section::Inbox], vec![], vec![]);

        // Down starts the drag, Drag resizes live, Up persists what the frame now shows.
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        drag_map(&st);
        let (changed, eff) = st.on_mouse(down(5, 10), &data, 1_000);
        assert!(!changed, "starting a drag only arms it");
        assert_eq!(eff, GEffect::None);
        let (changed, eff) = st.on_mouse(
            mouse(MouseEventKind::Drag(MouseButton::Left), 5, 15),
            &data,
            1_100,
        );
        assert!(changed);
        assert_eq!(eff, GEffect::None);
        assert_eq!(st.layout.height(2), Some(0.6), "15 rows of a 25-row main");
        let (_, eff) = st.on_mouse(
            mouse(MouseEventKind::Up(MouseButton::Left), 5, 15),
            &data,
            1_200,
        );
        assert_eq!(eff, GEffect::SaveLayout("h2=0.6000".into()));

        // The group above never drops below MIN_SECTION_H...
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        drag_map(&st);
        st.on_mouse(down(5, 10), &data, 1_000);
        st.on_mouse(
            mouse(MouseEventKind::Drag(MouseButton::Left), 5, 0),
            &data,
            1_100,
        );
        assert_eq!(st.layout.height(2), Some(0.12), "3 rows of 25");
        // ... and Messages keeps MESSAGES_MIN rows.
        st.on_mouse(
            mouse(MouseEventKind::Drag(MouseButton::Left), 5, 200),
            &data,
            1_200,
        );
        assert_eq!(st.layout.height(2), Some(21.0 / 25.0), "25 - MESSAGES_MIN");

        // A drag that ends without moving persists nothing.
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        drag_map(&st);
        st.on_mouse(down(5, 10), &data, 1_000);
        let (_, eff) = st.on_mouse(
            mouse(MouseEventKind::Up(MouseButton::Left), 5, 10),
            &data,
            1_100,
        );
        assert_eq!(eff, GEffect::None);
    }

    #[test]
    fn mouse_drag_top_pair_vertical_keeps_twenty_columns() {
        let data = data_with(
            vec![Section::Quota, Section::Terminals],
            vec![term("t1", "worker-1")],
            vec![],
        );
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        drag_map_vertical(&st);
        st.on_mouse(down(49, 5), &data, 1_000);
        let (changed, eff) = st.on_mouse(
            mouse(MouseEventKind::Drag(MouseButton::Left), 45, 5),
            &data,
            1_100,
        );
        assert!(changed);
        assert_eq!(eff, GEffect::None);
        assert_eq!(st.layout.top_pair_width, Some(0.45), "45 of 100 columns");

        // Both panels keep at least 20 columns whatever the pointer does.
        st.on_mouse(down(49, 5), &data, 2_000);
        st.on_mouse(
            mouse(MouseEventKind::Drag(MouseButton::Left), 0, 5),
            &data,
            2_100,
        );
        assert_eq!(st.layout.top_pair_width, Some(0.2));
        st.on_mouse(
            mouse(MouseEventKind::Drag(MouseButton::Left), 200, 5),
            &data,
            2_200,
        );
        assert_eq!(st.layout.top_pair_width, Some(0.8));
    }

    #[test]
    fn mouse_drag_is_not_the_first_click_of_a_double_click() {
        let data = data_with(vec![Section::Quota, Section::Inbox], vec![], vec![]);
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        drag_map(&st);

        // Down → Drag → Up resizes and persists h2.
        st.on_mouse(down(5, 10), &data, 1_000);
        let (changed, _) = st.on_mouse(drag(5, 15), &data, 1_100);
        assert!(changed);
        st.on_mouse(up(5, 15), &data, 1_200);
        assert_eq!(st.layout.height(2), Some(0.6));

        // The next Down on the same border is 350 ms after the first one, but the drag in
        // between means it is a new press, not the second half of a double-click.
        let (changed, eff) = st.on_mouse(down(5, 10), &data, 1_350);
        assert!(!changed, "the press after a drag reset the boundary");
        assert_eq!(eff, GEffect::None);
        assert_eq!(
            st.layout.height(2),
            Some(0.6),
            "a drag leaked into last_click and reset the boundary"
        );
    }

    #[test]
    fn mouse_popup_and_output_overlay_drop_a_pending_drag_and_click() {
        let mut data = data_with(vec![Section::Quota, Section::Inbox], vec![], vec![]);
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        st.layout.set_height(2, 0.4);
        drag_map(&st);

        // A Down on the boundary arms a drag and remembers a click.
        st.on_mouse(down(5, 10), &data, 1_000);

        // The popup owns the mouse: the event also forgets both.
        st.popup = Some(Popup::Confirm {
            prompt: "kill?".into(),
            input: String::new(),
        });
        let (changed, eff) = st.on_mouse(drag(5, 15), &data, 1_100);
        assert!(!changed);
        assert_eq!(eff, GEffect::None);
        st.popup = None;

        // The drag is gone: the same Drag no longer resizes anything.
        let (changed, eff) = st.on_mouse(drag(5, 15), &data, 1_200);
        assert!(!changed, "a drag survived the popup");
        assert_eq!(eff, GEffect::None);
        assert_eq!(st.layout.height(2), Some(0.4));
        // The click is gone too: a Down 300 ms after the armed one is not a double-click.
        let (changed, eff) = st.on_mouse(down(5, 10), &data, 1_300);
        assert!(
            !changed,
            "a click survived the popup and reset the boundary"
        );
        assert_eq!(eff, GEffect::None);
        assert_eq!(st.layout.height(2), Some(0.4));

        // Same for the Output overlay.
        st.on_mouse(up(5, 10), &data, 1_400);
        st.on_mouse(down(5, 10), &data, 2_000);
        data.output = Some("cleanup 1 t2".into());
        let (changed, eff) = st.on_mouse(down(5, 15), &data, 2_100);
        assert!(!changed);
        assert_eq!(eff, GEffect::None);
        data.output = None;
        let (changed, eff) = st.on_mouse(drag(5, 15), &data, 2_200);
        assert!(!changed, "a drag survived the Output overlay");
        assert_eq!(eff, GEffect::None);
        let (changed, eff) = st.on_mouse(down(5, 10), &data, 2_300);
        assert!(
            !changed,
            "a click survived the Output overlay and reset the boundary"
        );
        assert_eq!(eff, GEffect::None);
        assert_eq!(st.layout.height(2), Some(0.4));
    }

    #[test]
    fn mouse_double_click_border_resets_only_that_boundary() {
        let data = data_with(vec![Section::Quota, Section::Inbox], vec![], vec![]);
        let mut st = GlobalState::new(ThemeName::CatppuccinMocha);
        st.layout.set_top_pair_width(0.5);
        st.layout.set_height(2, 0.4);
        st.layout.set_height(3, 0.3);
        drag_map(&st);

        st.on_mouse(down(5, 10), &data, 1_000);
        let (changed, eff) = st.on_mouse(down(5, 10), &data, 1_200);
        assert!(changed);
        assert_eq!(eff, GEffect::SaveLayout("w=0.5000;h3=0.3000".into()));
        assert_eq!(st.layout.height(2), None, "only Horizontal(2) was reset");
        assert_eq!(st.layout.height(3), Some(0.3));
        assert_eq!(st.layout.top_pair_width, Some(0.5));

        // A double-click on a boundary with no override left is not a visible change. The first
        // Down below is a fresh press (the earlier pair was consumed), the second one is the
        // double-click that really reaches the `!stored` branch.
        let (changed, eff) = st.on_mouse(down(5, 10), &data, 3_000);
        assert!(!changed);
        assert_eq!(eff, GEffect::None);
        let (changed, eff) = st.on_mouse(down(5, 10), &data, 3_100);
        assert!(!changed);
        assert_eq!(eff, GEffect::None);
        assert_eq!(st.layout.height(2), None, "the reset boundary came back");
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
