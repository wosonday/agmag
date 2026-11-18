use std::fs::File;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::Result;

use crate::model::{LineOutcome, ParseEvent, Source};
use crate::parse::parser_for;
use crate::store::{Cursor, Store};

pub struct Roots {
    pub codex: PathBuf,
    pub claude: PathBuf,
}

impl Roots {
    pub fn from_home() -> Roots {
        let home = dirs::home_dir().expect("home directory is unavailable");
        Roots {
            codex: home.join(".codex").join("sessions"),
            claude: home.join(".claude").join("projects"),
        }
    }
}

pub fn discover(roots: &Roots, since_ms: i64) -> Vec<(Source, PathBuf)> {
    let mut files = Vec::new();
    for (source, root) in [
        (Source::Codex, &roots.codex),
        (Source::Claude, &roots.claude),
    ] {
        for entry in walkdir::WalkDir::new(root)
            .into_iter()
            .filter_map(Result::ok)
        {
            let path = entry.path();
            if !entry.file_type().is_file() || path.extension().is_none_or(|ext| ext != "jsonl") {
                continue;
            }
            if source == Source::Codex
                && !entry.file_name().to_string_lossy().starts_with("rollout-")
            {
                continue;
            }
            let modified = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .and_then(|d| i64::try_from(d.as_millis()).ok());
            if modified.is_some_and(|ms| ms >= since_ms) {
                files.push((source.clone(), path.to_path_buf()));
            }
        }
    }
    files.sort_by(|a, b| a.1.cmp(&b.1));
    files
}

pub fn ingest_all(store: &Store, roots: &Roots, since_ms: i64) -> Result<IngestStats> {
    let mut total = IngestStats::default();
    for (source, path) in discover(roots, since_ms) {
        let stats = ingest_file(store, source, &path)?;
        total.files += stats.files;
        total.lines += stats.lines;
        total.events += stats.events;
        total.unknown += stats.unknown;
        total.bad += stats.bad;
    }
    Ok(total)
}

#[derive(Default, Debug, PartialEq)]
pub struct IngestStats {
    pub files: u64,
    pub lines: u64,
    pub events: u64,
    pub unknown: u64,
    pub bad: u64,
}

pub fn ingest_file(store: &Store, source: Source, path: &Path) -> Result<IngestStats> {
    let mut reader = BufReader::new(File::open(path)?);
    let metadata = reader.get_ref().metadata()?;
    let size = metadata.len();
    let mtime_ms = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)?
        .as_millis()
        .try_into()?;
    let path_key = path.to_string_lossy();
    let tx = store.conn().unchecked_transaction()?;
    let sid = store.session_id_for_path(source.clone(), &path_key)?;
    let saved = store.cursor(&path_key)?;
    let mut head = Vec::new();
    reader.by_ref().take(4096).read_to_end(&mut head)?;
    let hash = |bytes: &[u8]| {
        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        hasher.finish()
    };
    // Compare the old prefix length: appending to a short file changes its full head hash.
    let mut offset = saved
        .as_ref()
        .filter(|c| {
            size >= c.offset
                && size >= c.size
                && hash(&head[..head.len().min(c.size as usize)]) == c.head_hash
        })
        .map_or(0, |c| c.offset);
    reader.seek(SeekFrom::Start(offset))?;
    let mut parser = parser_for(source);
    if offset > 0 {
        let model: Option<String> =
            store
                .conn()
                .query_row("SELECT model FROM sessions WHERE id=?1", [sid], |row| {
                    row.get(0)
                })?;
        if let Some(model) = model {
            parser.seed_model(&model);
        }
    }
    let mut stats = IngestStats {
        files: 1,
        ..Default::default()
    };
    let mut bytes = Vec::new();
    loop {
        bytes.clear();
        if reader.read_until(b'\n', &mut bytes)? == 0 || bytes.last() != Some(&b'\n') {
            break;
        }
        stats.lines += 1;
        match parser.parse_line(&String::from_utf8_lossy(&bytes), offset) {
            Ok(LineOutcome::Events(events)) => {
                for event in events {
                    match event {
                        ParseEvent::Meta(meta) => store.set_meta(sid, &meta)?,
                        ParseEvent::Model(model) => store.set_model(sid, &model)?,
                        ParseEvent::Usage(usage) => store.upsert_usage(sid, &usage)?,
                        ParseEvent::Quota(quota) => store.insert_quota(&quota)?,
                        ParseEvent::Context(context) => store.set_context(sid, &context)?,
                        ParseEvent::Turn(turn) => store.insert_turn(sid, &turn)?,
                    }
                    stats.events += 1;
                }
            }
            Ok(LineOutcome::Unknown(_)) => stats.unknown += 1,
            Err(_) => stats.bad += 1,
            Ok(LineOutcome::Ignored) => {}
        }
        offset += bytes.len() as u64;
    }
    store.add_line_stats(sid, stats.unknown, stats.bad)?;
    store.save_cursor(&Cursor {
        path: path_key.into_owned(),
        offset,
        size,
        mtime_ms,
        head_hash: hash(&head),
    })?;
    tx.commit()?;
    Ok(stats)
}
