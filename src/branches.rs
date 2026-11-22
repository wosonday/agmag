//! Đọc và sửa khối `### <nhánh>` trong BRANCHES.md — chỉ là hàm thuần trên văn bản,
//! không đụng tệp, không khóa.

use anyhow::{Result, bail};

pub const STATUSES: [&str; 8] = [
    "wip",
    "blocked",
    "ready",
    "merging",
    "merged",
    "deployed",
    "needs-rebase",
    "failed",
];

pub const FIELDS: [&str; 14] = [
    "status",
    "lane",
    "tier",
    "head",
    "base",
    "depends_on",
    "migration",
    "touches_hotspot",
    "summary",
    "verify",
    "review",
    "docs",
    "registered",
    "git",
];

fn valid_part(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && s.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        })
}

/// Tên mục dạng `<repo>/<nhánh>`: đúng một `/`, mỗi phần 1–64 ký tự `[a-z0-9._-]`.
pub fn validate_entry(entry: &str) -> Result<()> {
    let parts: Vec<&str> = entry.split('/').collect();
    if parts.len() != 2 || !parts.iter().all(|p| valid_part(p)) {
        bail!(
            "tên mục không hợp lệ: {entry:?} (cần dạng repo/nhánh, mỗi phần 1-64 ký tự a-z 0-9 . _ -)"
        );
    }
    Ok(())
}

pub fn validate_fields(fields: &[(String, String)]) -> Result<()> {
    for (i, (k, v)) in fields.iter().enumerate() {
        if !FIELDS.contains(&k.as_str()) {
            bail!("khóa trường không hợp lệ: {k:?}");
        }
        if fields[..i].iter().any(|(k2, _)| k2 == k) {
            bail!("khóa trường bị trùng: {k:?}");
        }
        if v.trim().is_empty() {
            bail!("giá trị của {k} đang trống");
        }
        if v.contains('\n') || v.contains('\r') {
            bail!("giá trị của {k} không được chứa xuống dòng");
        }
        if k == "status" && !STATUSES.contains(&v.trim()) {
            bail!(
                "trạng thái không hợp lệ: {v:?} (hợp lệ: {})",
                STATUSES.join(" ")
            );
        }
    }
    Ok(())
}

fn content(line: &str) -> &str {
    line.trim_end_matches(['\n', '\r'])
}

/// Phân tích dòng trường `- khóa: giá trị`, trả khóa và phần sau dấu `:` (chưa cắt chú thích).
fn parse_field(line: &str) -> Option<(&str, &str)> {
    let rest = content(line).strip_prefix("- ")?;
    let (k, v) = rest.split_once(':')?;
    if k.is_empty() || !k.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
        return None;
    }
    Some((k, v))
}

fn strip_comment(v: &str) -> &str {
    let cut = [v.find(" # "), v.find("\t# ")]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(v.len());
    v[..cut].trim()
}

/// Tìm khối của `entry`: `(chỉ số dòng tiêu đề, chỉ số dòng kết thúc không gồm)`.
fn find_block(lines: &[&str], entry: &str) -> Result<Option<(usize, usize)>> {
    let heading = format!("### {entry}");
    let mut hits = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| content(l).trim_end() == heading)
        .map(|(i, _)| i);
    let Some(start) = hits.next() else {
        return Ok(None);
    };
    if hits.next().is_some() {
        bail!("mục {entry} xuất hiện nhiều lần trong BRANCHES.md");
    }
    let end = lines[start + 1..]
        .iter()
        .position(|l| l.starts_with('#'))
        .map_or(lines.len(), |p| start + 1 + p);
    Ok(Some((start, end)))
}

pub fn read_entry(text: &str, entry: &str) -> Result<Option<Vec<(String, String)>>> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let Some((start, end)) = find_block(&lines, entry)? else {
        return Ok(None);
    };
    let fields = lines[start + 1..end]
        .iter()
        .filter_map(|l| parse_field(l))
        .map(|(k, v)| (k.to_string(), strip_comment(v).to_string()))
        .collect();
    Ok(Some(fields))
}

/// Cập nhật (hoặc tạo) khối `entry`; trả `(văn bản mới, created)`.
pub fn upsert_entry(
    text: &str,
    entry: &str,
    updates: &[(String, String)],
) -> Result<(String, bool)> {
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let field_line = |k: &str, v: &str| format!("- {k}: {}{eol}", v.trim());
    let lines: Vec<&str> = text.split_inclusive('\n').collect();

    let Some((start, end)) = find_block(&lines, entry)? else {
        let mut out = text.trim_end_matches(['\n', '\r']).to_string();
        if !out.is_empty() {
            out.push_str(eol);
            out.push_str(eol);
        }
        out.push_str(&format!("### {entry}{eol}"));
        for (k, v) in updates {
            out.push_str(&field_line(k, v));
        }
        return Ok((out, true));
    };

    let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    let mut seen = vec![false; updates.len()];
    let mut last_field = start;
    for (i, line) in lines.iter().enumerate().take(end).skip(start + 1) {
        let Some((k, _)) = parse_field(line) else {
            continue;
        };
        last_field = i;
        if let Some(j) = updates.iter().rposition(|(uk, _)| uk == k) {
            out[i] = field_line(k, &updates[j].1);
            seen[j] = true;
            // khóa trùng trong cùng khối: các cập nhật khác cùng khóa cũng coi như đã xử lý
            for (s, (uk, _)) in seen.iter_mut().zip(updates) {
                if uk == k {
                    *s = true;
                }
            }
        }
    }
    let new_lines: String = updates
        .iter()
        .zip(&seen)
        .filter(|(_, s)| !**s)
        .map(|((k, v), _)| field_line(k, v))
        .collect();
    if !new_lines.is_empty() {
        // dòng liền trước chỗ chèn có thể là dòng cuối tệp thiếu xuống dòng
        if !out[last_field].ends_with('\n') {
            out[last_field].push_str(eol);
        }
        out[last_field].push_str(&new_lines);
    }
    Ok((out.concat(), false))
}
