use std::fs::File;
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

pub fn discover_source(source: Source, root: &Path, since_ms: i64) -> (Vec<PathBuf>, u64) {
    let mut files = Vec::new();
    let mut dir_errors = 0u64;
    if !root.exists() {
        return (files, 0);
    }
    for entry_res in walkdir::WalkDir::new(root) {
        let entry = match entry_res {
            Ok(e) => e,
            Err(err) => {
                if !matches!(err.io_error().map(|e| e.kind()), Some(std::io::ErrorKind::NotFound)) {
                    dir_errors += 1;
                }
                continue;
            }
        };
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
            files.push(path.to_path_buf());
        }
    }
    files.sort();
    (files, dir_errors)
}

pub fn discover(roots: &Roots, since_ms: i64) -> Vec<(Source, PathBuf)> {
    let mut files = Vec::new();
    for (source, root) in [
        (Source::Codex, &roots.codex),
        (Source::Claude, &roots.claude),
    ] {
        let (found, _) = discover_source(source.clone(), root, since_ms);
        for p in found {
            files.push((source.clone(), p));
        }
    }
    files.sort_by(|a, b| a.1.cmp(&b.1));
    files
}

pub fn ingest_source(store: &Store, source: Source, root: &Path, since_ms: i64) -> Result<IngestStats> {
    let (paths, dir_errors) = discover_source(source.clone(), root, since_ms);
    let mut total = IngestStats {
        skipped: dir_errors,
        ..Default::default()
    };
    for path in paths {
        // Một tệp lỗi (bị khóa, bị xóa giữa chừng) không được làm hỏng cả lần quét.
        let stats = match ingest_file(store, source.clone(), &path) {
            Ok(stats) => stats,
            Err(err) => {
                eprintln!("agm: bỏ qua {}: {err:#}", path.display());
                total.skipped += 1;
                continue;
            }
        };
        total.add(&stats);
    }
    Ok(total)
}

pub fn ingest_all(store: &Store, roots: &Roots, since_ms: i64) -> Result<IngestStats> {
    let mut total = IngestStats::default();
    total.add(&ingest_source(store, Source::Codex, &roots.codex, since_ms)?);
    total.add(&ingest_source(store, Source::Claude, &roots.claude, since_ms)?);
    Ok(total)
}

#[derive(Default, Debug, PartialEq)]
pub struct IngestStats {
    pub files: u64,
    pub lines: u64,
    pub events: u64,
    pub unknown: u64,
    pub bad: u64,
    pub skipped: u64,
}

impl IngestStats {
    pub fn add(&mut self, other: &IngestStats) {
        self.files += other.files;
        self.lines += other.lines;
        self.events += other.events;
        self.unknown += other.unknown;
        self.bad += other.bad;
        self.skipped += other.skipped;
    }
}

/// FNV-1a 64-bit của phần đầu tệp: ổn định giữa các phiên bản Rust (khác `DefaultHasher`).
pub fn head_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// Khóa ổn định cho một tệp log. Trên Windows đường dẫn không phân biệt hoa thường và chấp nhận cả `/`,
/// nên chuẩn hóa để cùng một tệp không thành hai phiên (đếm token hai lần).
pub fn path_key(path: &Path) -> String {
    let s = path.to_string_lossy();
    if cfg!(windows) {
        s.replace('/', "\\").to_lowercase()
    } else {
        s.into_owned()
    }
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
    let path_key = path_key(path);
    let tx = store.conn().unchecked_transaction()?;
    let sid = store.session_id_for_path(source.clone(), &path_key)?;
    let saved = store.cursor(&path_key)?;
    let mut head = Vec::new();
    reader.by_ref().take(4096).read_to_end(&mut head)?;
    let hash = head_hash;
    // Compare the old prefix length: appending to a short file changes its full head hash.
    let mut offset = saved
        .as_ref()
        .filter(|c| {
            size >= c.offset
                && size >= c.size
                && hash(&head[..head.len().min(c.size as usize)]) == c.head_hash
        })
        .map_or(0, |c| c.offset);
    if saved.is_some() && offset == 0 {
        // Tệp đã bị thay: bỏ dữ liệu cũ để đọc lại không bị cộng dồn (cùng transaction của tệp).
        store.reset_session(sid)?;
    }
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
        if let Some(ts_ms) = store.session_last_ts(sid)? {
            parser.seed_last_ts(ts_ms);
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
        path: path_key,
        offset,
        size,
        mtime_ms,
        head_hash: hash(&head),
    })?;
    tx.commit()?;
    Ok(stats)
}
