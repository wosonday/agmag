//! Dữ liệu và bản vẽ văn bản cho `agm tui` (spec §5.8). Thuần — không đọc phím, không in.

use anyhow::Result;
use comfy_table::{presets::UTF8_FULL_CONDENSED, Table};

use crate::actions::KINDS;
use crate::config::Config;
use crate::policy::{budget_value, enabled, session_context_pct};
use crate::report::{quota, QuotaRow};
use crate::store::{InboxRow, MessageRow, Store};

#[derive(Debug, Clone, PartialEq)]
pub struct TermView {
    pub address: String,
    pub name: String,
    pub app: String,
    pub status: String,
    pub task: String,
    /// None khi terminal chưa ghép được với phiên log (không biết, không phải 0).
    pub tokens: Option<u64>,
    pub context_pct: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BudgetView {
    pub scope: String,
    pub unit: String,
    pub value: Option<f64>,
    pub soft: f64,
    pub hard: f64,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub now_s: i64,
    pub policy_on: bool,
    pub quota: Vec<QuotaRow>,
    pub terminals: Vec<TermView>,
    pub budgets: Vec<BudgetView>,
    pub inbox: Vec<InboxRow>,
    pub messages: Vec<MessageRow>,
}

pub fn snapshot(store: &Store, cfg: &Config, now_ms: i64) -> Result<Snapshot> {
    let mut terminals = Vec::new();
    for t in store.terminals(true)? {
        let (tokens, ctx) = match t.session {
            Some(sid) => {
                let n: i64 = store.conn().query_row(
                    "SELECT COALESCE(SUM(input_uncached + cache_read + cache_write + output), 0) FROM usage WHERE session = ?1",
                    [sid],
                    |r| r.get(0),
                )?;
                (Some(n.max(0) as u64), session_context_pct(store, cfg, sid)?)
            }
            None => (None, None),
        };
        let status = match (t.status.as_deref(), t.kind.as_deref()) {
            (Some("blocked"), Some(k)) => format!("BLOCKED ({k})"),
            (Some("blocked"), None) => "BLOCKED".into(),
            (Some(s), _) => s.to_string(),
            (None, _) => "-".into(),
        };
        terminals.push(TermView {
            address: t.address,
            name: t.name.unwrap_or_default(),
            app: t.app.unwrap_or_else(|| "?".into()),
            status,
            task: t.task.unwrap_or_default(),
            tokens,
            context_pct: ctx,
        });
    }
    let mut budgets = Vec::new();
    for b in store.budgets()? {
        budgets.push(BudgetView { value: budget_value(store, &b)?, scope: b.scope, unit: b.unit, soft: b.soft, hard: b.hard });
    }
    Ok(Snapshot {
        now_s: now_ms / 1000,
        policy_on: enabled(store)?,
        quota: quota(store, now_ms / 1000)?,
        terminals,
        budgets,
        inbox: store.inbox(true)?,
        messages: store.messages(5)?,
    })
}

fn bar(pct: f64) -> String {
    let filled = ((pct / 10.0).round().clamp(0.0, 10.0)) as usize;
    format!("{}{}", "█".repeat(filled), "░".repeat(10 - filled))
}

fn tokens(n: u64) -> String {
    match n {
        n if n >= 1_000_000 => format!("{:.1}M", n as f64 / 1e6),
        n if n >= 1_000 => format!("{:.0}k", n as f64 / 1e3),
        n => n.to_string(),
    }
}

fn section(title: &str, width: usize) -> String {
    let line = "─".repeat(width.saturating_sub(title.chars().count() + 3));
    format!("── {title} {line}\n")
}

fn table(header: &[&str], width: usize) -> Table {
    let mut t = Table::new();
    t.load_style(UTF8_FULL_CONDENSED).set_header(header.to_vec()).set_width(width as u16);
    t
}

/// Bản vẽ toàn màn hình dạng văn bản (đã gồm tiêu đề các khu vực).
pub fn render(s: &Snapshot, width: usize) -> String {
    let mut out = String::new();
    out += &format!("agmag · policy {} · {} terminal · hộp thư {}\n", if s.policy_on { "bật" } else { "TẮT" }, s.terminals.len(), s.inbox.len());

    out += &section("HẠN MỨC GÓI", width);
    if s.quota.is_empty() {
        out += "  (chưa có số liệu — chạy agm scan; Claude cần `agm hook install`)\n";
    }
    for q in &s.quota {
        let win = if q.window_minutes % 1440 == 0 { format!("{}d", q.window_minutes / 1440) } else { format!("{}h", q.window_minutes / 60) };
        let reset = if q.reset_passed {
            "đã reset".to_string()
        } else {
            let left = q.resets_at_s - s.now_s;
            format!("còn {}h{:02}m", left / 3600, left % 3600 / 60)
        };
        out += &format!("  {:<8} {:>3}  {} {:>3.0}%  {reset}\n", q.provider, win, bar(q.used_percent), q.used_percent);
    }

    out += &section("TERMINALS", width);
    let mut t = table(&["t", "tên", "app", "trạng thái", "task", "token", "ngữ cảnh"], width);
    for v in &s.terminals {
        t.add_row(vec![
            v.address.clone(),
            v.name.clone(),
            v.app.clone(),
            v.status.clone(),
            v.task.clone(),
            v.tokens.map(tokens).unwrap_or_else(|| "?".into()),
            v.context_pct.map(|p| format!("{p:.0}%")).unwrap_or_else(|| "?".into()),
        ]);
    }
    out += &format!("{t}\n");

    out += &section("NGÂN SÁCH", width);
    if s.budgets.is_empty() {
        out += "  (chưa đặt — agm budget set task:<ID> --soft … --hard …)\n";
    }
    for b in &s.budgets {
        let pct = b.value.filter(|_| b.hard > 0.0).map(|v| v / b.hard * 100.0);
        let flag = match b.value {
            Some(v) if v >= b.hard => " CỨNG",
            Some(v) if v >= b.soft => " mềm",
            _ => "",
        };
        // Chưa có số liệu → "?" (không vẽ 0% giả). Ngân sách % hiển thị số thực, không làm tròn kiểu token.
        let num = |v: f64| if b.unit == "tokens" { tokens(v as u64) } else { format!("{v:.1}") };
        out += &format!(
            "  {:<24} {} {:>4}  {} / {} {}{flag}\n",
            b.scope,
            pct.map(bar).unwrap_or_else(|| "··········".into()),
            pct.map(|p| format!("{p:.0}%")).unwrap_or_else(|| "?".into()),
            b.value.map(num).unwrap_or_else(|| "?".into()),
            num(b.hard),
            b.unit
        );
    }

    out += &section("HỘP THƯ", width);
    if s.inbox.is_empty() {
        out += "  (trống)\n";
    }
    for i in &s.inbox {
        out += &format!("  #{} [{}] {} — {}\n", i.id, i.source, i.terminal.as_deref().unwrap_or("-"), one_line(&i.text, 200));
    }

    out += &section("TIN GẦN ĐÂY", width);
    for m in &s.messages {
        out += &format!("  {} → {}  {}  {}  ({})\n", m.from, m.to, m.kind, one_line(&m.text, 60), m.status);
    }
    out += &"─".repeat(width);
    out += "\nlệnh: r <#> <trả lời> · d <#> (đóng) · s <t> <LOẠI> <tin> · i <t> · k <t> · p on|off · q   (Enter: làm mới)\n";
    out
}

/// Một dòng hiển thị: bỏ xuống dòng và ký tự điều khiển (kể cả mã ANSI) để không phá khung TUI.
fn one_line(s: &str, max: usize) -> String {
    s.chars().map(|c| if c.is_control() { ' ' } else { c }).take(max).collect()
}

#[derive(Debug, Clone, PartialEq)]
pub enum TuiCmd {
    Refresh,
    Quit,
    Reply(i64, String),
    Done(i64),
    Send(String, String, String),
    Interrupt(String),
    Kill(String),
    Policy(bool),
}

pub fn parse_command(line: &str) -> std::result::Result<TuiCmd, String> {
    let line = line.trim();
    let (head, rest) = line.split_once(' ').map_or((line, ""), |(h, r)| (h, r.trim()));
    Ok(match head {
        "" => TuiCmd::Refresh,
        "q" => TuiCmd::Quit,
        "r" => {
            let (id, text) = rest.split_once(' ').unwrap_or((rest, ""));
            let Ok(id) = id.parse() else { return Err("cú pháp: r <số mục> <trả lời>".into()) };
            if text.trim().is_empty() {
                return Err("thiếu nội dung trả lời".into());
            }
            TuiCmd::Reply(id, text.trim().into())
        }
        "d" => match rest.parse() {
            Ok(id) => TuiCmd::Done(id),
            Err(_) => return Err("cú pháp: d <số mục>".into()),
        },
        "s" => {
            let (target, after) = rest.split_once(' ').unwrap_or((rest, ""));
            let after = after.trim_start();
            // Loại tin có thể có hai từ (BÁO CÁO, CHƯA ĐẠT); sau loại phải là khoảng trắng (ranh giới từ).
            let Some(kind) = KINDS
                .iter()
                .filter(|k| after.strip_prefix(**k).is_some_and(|r| r.starts_with(' ')))
                .max_by_key(|k| k.len())
            else {
                return Err(format!("cú pháp: s <terminal> <{}> <tin>", KINDS.join("|")));
            };
            let text = after[kind.len()..].trim();
            if target.is_empty() || text.is_empty() {
                return Err("cú pháp: s <terminal> <LOẠI> <tin>".into());
            }
            TuiCmd::Send(target.into(), kind.to_string(), text.into())
        }
        "i" if !rest.is_empty() => TuiCmd::Interrupt(rest.into()),
        "k" if !rest.is_empty() => TuiCmd::Kill(rest.into()),
        "p" if rest == "on" || rest == "off" => TuiCmd::Policy(rest == "on"),
        _ => return Err(format!("không hiểu lệnh '{line}'")),
    })
}
