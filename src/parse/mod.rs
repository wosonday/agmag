use crate::model::{LineOutcome, Source};

pub mod agy;
pub mod claude;
pub mod codex;

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("bad json: {0}")]
    BadJson(String),
    #[error("missing timestamp")]
    MissingTimestamp,
}

pub trait LogParser {
    fn parse_line(&mut self, line: &str, byte_offset: u64) -> Result<LineOutcome, ParseError>;
    fn seed_model(&mut self, model: &str);
    fn seed_last_ts(&mut self, _ts_ms: i64) {}
}

pub fn parser_for(source: Source) -> Box<dyn LogParser> {
    match source {
        Source::Codex => Box::new(codex::CodexParser::default()),
        Source::Claude => Box::new(claude::ClaudeParser::default()),
        Source::Agy => Box::new(agy::AgyParser::default()),
        Source::OpenCode => unreachable!("nguồn này không đọc theo dòng log"),
    }
}
