use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Codex,
    Claude,
    OpenCode,
    Agy,
}

impl Source {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::OpenCode => "opencode",
            Self::Agy => "agy",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    pub input_uncached: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub output: u64,
    pub reasoning: u64,
}

impl Usage {
    pub fn add(&mut self, o: &Usage) {
        self.input_uncached += o.input_uncached;
        self.cache_read += o.cache_read;
        self.cache_write += o.cache_write;
        self.output += o.output;
        self.reasoning += o.reasoning;
    }

    pub fn total(&self) -> u64 {
        self.input_uncached + self.cache_read + self.cache_write + self.output
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageRecord {
    pub dedupe_key: String,
    pub ts_ms: i64,
    pub model: Option<String>,
    pub usage: Usage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuotaWindow {
    pub window_minutes: u32,
    pub used_percent: f64,
    pub resets_at_s: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuotaSnapshot {
    pub provider: String,
    pub ts_ms: i64,
    pub windows: Vec<QuotaWindow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextSnapshot {
    pub ts_ms: i64,
    pub used_tokens: u64,
    pub window_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionMeta {
    pub session_id: String,
    pub cwd: Option<String>,
    pub cli_version: Option<String>,
    pub started_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnSummary {
    pub ts_ms: i64,
    pub role: Role,
    pub summary: String,
    pub byte_offset: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParseEvent {
    Meta(SessionMeta),
    Model(String),
    /// Reasoning effort / variant of the session's model (`high`, `medium`, ...).
    Effort(String),
    Usage(UsageRecord),
    Quota(QuotaSnapshot),
    Context(ContextSnapshot),
    Turn(TurnSummary),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LineOutcome {
    Events(Vec<ParseEvent>),
    Ignored,
    Unknown(String),
}

pub fn summarize(text: &str) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= 200 {
        normalized
    } else {
        normalized.chars().take(199).chain(['…']).collect()
    }
}

/// Parses UTC timestamps in `YYYY-MM-DDTHH:MM:SS(.fff)?Z` form.
pub fn parse_rfc3339_ms(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if !matches!(b.len(), 20 | 24)
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[b.len() - 1] != b'Z'
    {
        return None;
    }
    let number = |digits: &[u8]| -> Option<i64> {
        digits.iter().try_fold(0, |n, digit| {
            digit
                .is_ascii_digit()
                .then(|| n * 10 + i64::from(digit - b'0'))
        })
    };
    let year = number(&b[..4])?;
    let month = number(&b[5..7])?;
    let day = number(&b[8..10])?;
    let hour = number(&b[11..13])?;
    let minute = number(&b[14..16])?;
    let second = number(&b[17..19])?;
    let millis = if b.len() == 24 {
        if b[19] != b'.' {
            return None;
        }
        number(&b[20..23])?
    } else {
        0
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    if !(1..=days_in_month).contains(&day) || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    // Count complete years and months relative to the Unix epoch, including year 0.
    let leap_years_before =
        |y: i64| (y - 1).div_euclid(4) - (y - 1).div_euclid(100) + (y - 1).div_euclid(400);
    let mut days = 365 * (year - 1970) + leap_years_before(year) - leap_years_before(1970);
    let month_lengths = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    days += month_lengths[..(month - 1) as usize].iter().sum::<i64>();
    if leap && month > 2 {
        days += 1;
    }
    days += day - 1;
    Some(((days * 24 + hour) * 3600 + minute * 60 + second) * 1000 + millis)
}
