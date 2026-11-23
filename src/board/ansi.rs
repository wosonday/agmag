//! Converts the dashboard's ANSI text (SGR colours only) into ratatui `Text`.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};

/// Keeps `ESC [ … m` styling; drops every other escape sequence and control character except `\n`.
pub fn ansi_to_text(s: &str) -> Text<'static> {
    let bytes = s.as_bytes();
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut plain = String::new();
    let mut style = Style::new();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'\x1b' => {
                push_span(&mut spans, &mut plain, style);
                i = skip_escape(bytes, i, &mut style);
            }
            b'\n' => {
                push_span(&mut spans, &mut plain, style);
                lines.push(Line::from(std::mem::take(&mut spans)));
                i += 1;
            }
            b if b < 0x20 || b == 0x7f => i += 1,
            _ => {
                let start = i;
                while i < bytes.len()
                    && bytes[i] >= 0x20
                    && bytes[i] != 0x7f
                    && bytes[i] != b'\x1b'
                    && bytes[i] != b'\n'
                {
                    i += 1;
                }
                // `start`/`i` are UTF-8 boundaries: the scanner only stops on ASCII bytes.
                plain.push_str(&s[start..i]);
            }
        }
    }
    push_span(&mut spans, &mut plain, style);
    if !spans.is_empty() || lines.is_empty() {
        lines.push(Line::from(spans));
    }
    Text::from(lines)
}

/// Emits the buffered text as a span with `style`, merging with the previous span when the
/// style is unchanged (escape sequences that are dropped must not split visible text).
fn push_span(spans: &mut Vec<Span<'static>>, plain: &mut String, style: Style) {
    if plain.is_empty() {
        return;
    }
    let text = std::mem::take(plain);
    if let Some(last) = spans.last_mut()
        && last.style == style
    {
        last.content.to_mut().push_str(&text);
    } else {
        spans.push(Span::styled(text, style));
    }
}

/// Returns the index just past the escape sequence at `i`, applying SGR parameters to `style`.
fn skip_escape(bytes: &[u8], i: usize, style: &mut Style) -> usize {
    match bytes.get(i + 1) {
        Some(b'[') => {
            let mut j = i + 2;
            // CSI ends at the final byte (0x40..=0x7e); ignore sequences other than `m`.
            while j < bytes.len() && !(0x40..=0x7e).contains(&bytes[j]) {
                j += 1;
            }
            if j < bytes.len() {
                if bytes[j] == b'm' {
                    apply_sgr(&bytes[i + 2..j], style);
                }
                j + 1
            } else {
                j
            }
        }
        Some(b']') => {
            // OSC ends at BEL or ST (`ESC \`); drop title/hyperlink payloads entirely.
            let mut j = i + 2;
            while j < bytes.len() {
                if bytes[j] == 0x07 {
                    return j + 1;
                }
                if bytes[j] == b'\x1b' && bytes.get(j + 1) == Some(&b'\\') {
                    return j + 2;
                }
                j += 1;
            }
            j
        }
        Some(_) => i + 2, // other ESC sequences: drop the escape and its single follower
        None => i + 1,
    }
}

/// Applies one `ESC [ … m` parameter list. Unknown codes are ignored; `38;5;n` maps to
/// `Color::Indexed(n)`. Background codes and 24-bit colours are consumed but not styled:
/// the dashboard only emits foreground SGR codes.
fn apply_sgr(params: &[u8], style: &mut Style) {
    let raw = std::str::from_utf8(params).unwrap_or("");
    let codes: Vec<u16> = raw
        .split(';')
        .map(|p| match p {
            "" | "0" => 0,
            other => other.parse().unwrap_or(u16::MAX),
        })
        .collect();
    let mut i = 0;
    while i < codes.len() {
        match codes[i] {
            0 => *style = Style::new(),
            1 => *style = style.add_modifier(Modifier::BOLD),
            2 => *style = style.add_modifier(Modifier::DIM),
            22 => *style = style.remove_modifier(Modifier::BOLD | Modifier::DIM),
            30..=37 => style.fg = Some(standard_color(codes[i] - 30)),
            39 => style.fg = None,
            90..=97 => style.fg = Some(bright_color(codes[i] - 90)),
            38 if i + 2 < codes.len() && codes[i + 1] == 5 && codes[i + 2] <= 255 => {
                style.fg = Some(Color::Indexed(codes[i + 2] as u8));
                i += 2;
            }
            38 | 48 if i + 4 < codes.len() && codes[i + 1] == 2 => i += 4,
            48 if i + 2 < codes.len() && codes[i + 1] == 5 => i += 2,
            _ => {}
        }
        i += 1;
    }
}

fn standard_color(n: u16) -> Color {
    match n {
        0 => Color::Black,
        1 => Color::Red,
        2 => Color::Green,
        3 => Color::Yellow,
        4 => Color::Blue,
        5 => Color::Magenta,
        6 => Color::Cyan,
        _ => Color::Gray,
    }
}

fn bright_color(n: u16) -> Color {
    match n {
        0 => Color::DarkGray,
        1 => Color::LightRed,
        2 => Color::LightGreen,
        3 => Color::LightYellow,
        4 => Color::LightBlue,
        5 => Color::LightMagenta,
        6 => Color::LightCyan,
        _ => Color::White,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::{self, Section};

    fn joined(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn plain_text_one_line() {
        let text = ansi_to_text("hello");
        assert_eq!(text.lines.len(), 1);
        assert_eq!(text.lines[0].spans.len(), 1);
        assert_eq!(text.lines[0].spans[0].content, "hello");
        assert_eq!(text.lines[0].spans[0].style, Style::new());
    }

    #[test]
    fn newlines_split_lines() {
        let text = ansi_to_text("a\nb\nc");
        let lines: Vec<String> = text.lines.iter().map(joined).collect();
        assert_eq!(lines, vec!["a", "b", "c"]);
    }

    #[test]
    fn colors_and_bold() {
        let text = ansi_to_text("\x1b[1;32mok\x1b[0m x");
        assert_eq!(text.lines.len(), 1);
        let spans = &text.lines[0].spans;
        assert_eq!(spans.len(), 2, "spans: {spans:?}");
        assert_eq!(spans[0].content, "ok");
        assert_eq!(spans[0].style.fg, Some(Color::Green));
        assert!(spans[0].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(spans[1].content, " x");
        assert_eq!(spans[1].style, Style::new());
    }

    #[test]
    fn bright_and_indexed() {
        let text = ansi_to_text("\x1b[93mbright\x1b[38;5;200midx\x1b[39mplain");
        let spans = &text.lines[0].spans;
        assert_eq!(spans.len(), 3, "spans: {spans:?}");
        assert_eq!(spans[0].content, "bright");
        assert_eq!(spans[0].style.fg, Some(Color::LightYellow));
        assert_eq!(spans[1].content, "idx");
        assert_eq!(spans[1].style.fg, Some(Color::Indexed(200)));
        assert_eq!(spans[2].content, "plain");
        assert_eq!(spans[2].style.fg, None);
    }

    #[test]
    fn style_carries_across_newline() {
        let text = ansi_to_text("\x1b[32ma\nb");
        assert_eq!(text.lines.len(), 2);
        assert_eq!(joined(&text.lines[0]), "a");
        assert_eq!(joined(&text.lines[1]), "b");
        for line in &text.lines {
            assert_eq!(line.spans.len(), 1, "spans: {:?}", line.spans);
            assert_eq!(line.spans[0].style.fg, Some(Color::Green));
        }
    }

    #[test]
    fn osc_and_cursor_sequences_dropped() {
        let text = ansi_to_text("\x1b]0;pwn\x07a\x1b[2Jb");
        assert_eq!(text.lines.len(), 1);
        assert_eq!(joined(&text.lines[0]), "ab");
    }

    #[test]
    fn real_dashboard_section_roundtrip() {
        let store = crate::store::Store::open_in_memory().expect("in-memory store");
        let snap =
            dashboard::snapshot(&store, &crate::config::Config::builtin(), 1_800_000_000_000)
                .expect("snapshot");
        for sec in [Section::Usage, Section::Budgets, Section::System] {
            let body = dashboard::render_section_body(&snap, sec, 100);
            let text = ansi_to_text(&body);
            assert!(!text.lines.is_empty(), "{sec:?} produced no lines");
            for line in &text.lines {
                for span in &line.spans {
                    assert!(
                        !span.content.contains('\x1b'),
                        "{sec:?}: span kept an escape: {:?}",
                        span.content
                    );
                }
            }
        }
    }
}
