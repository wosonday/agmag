//! Live RAM / CPU / disk readings for this machine, plus the share taken by each AI agent's process tree
//! (`agm sys`, the `agm tui` header and its SYSTEM section). Nothing here is written to the database.
//!
//! Reading the OS goes through [`SysSampler`]; classification ([`agent_of`]) and per-agent totals ([`attribute`])
//! are pure functions so they can be tested without a real process table.

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use serde::Serialize;
use sysinfo::{
    CpuRefreshKind, DiskRefreshKind, Disks, MINIMUM_CPU_UPDATE_INTERVAL, MemoryRefreshKind,
    ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MemStats {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub used_percent: Option<f64>,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
}

impl MemStats {
    pub fn new(
        total_bytes: u64,
        used_bytes: u64,
        swap_total_bytes: u64,
        swap_used_bytes: u64,
    ) -> Self {
        MemStats {
            total_bytes,
            used_bytes,
            used_percent: pct(used_bytes, total_bytes),
            swap_total_bytes,
            swap_used_bytes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CpuStats {
    /// Whole machine, 0–100 (all logical cores together).
    pub usage_percent: f64,
    pub logical_cores: usize,
    /// Each logical core, 0–100, in the OS's order (`#0`, `#1`, …).
    pub per_core_percent: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiskStats {
    /// Mount point (`C:\`, `/`, `/home`).
    pub mount: String,
    pub file_system: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_percent: Option<f64>,
    pub removable: bool,
    /// Bytes per second since the previous reading; `None` before there are two readings.
    pub read_bytes_per_s: Option<f64>,
    pub write_bytes_per_s: Option<f64>,
    /// Other mount points showing the very same volume (e.g. one cloud drive under several letters).
    pub also_mounted_at: Vec<String>,
}

impl DiskStats {
    pub fn new(
        mount: &str,
        file_system: &str,
        total_bytes: u64,
        available_bytes: u64,
        removable: bool,
    ) -> Self {
        DiskStats {
            mount: mount.to_string(),
            file_system: file_system.to_string(),
            total_bytes,
            available_bytes,
            used_percent: pct(total_bytes.saturating_sub(available_bytes), total_bytes),
            removable,
            read_bytes_per_s: None,
            write_bytes_per_s: None,
            also_mounted_at: Vec::new(),
        }
    }

    pub fn used_bytes(&self) -> u64 {
        self.total_bytes.saturating_sub(self.available_bytes)
    }

    pub fn label(&self) -> String {
        mount_label(&self.mount)
    }
}

/// One agent's process tree: the agent's own processes plus everything they started (shells, builds, MCP servers).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AgentLoad {
    pub agent: String,
    pub processes: usize,
    pub memory_bytes: u64,
    /// Share of the whole machine's CPU, 0–100; `None` until two readings exist.
    pub cpu_percent: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct SysStats {
    pub memory: Option<MemStats>,
    pub cpu: Option<CpuStats>,
    pub disks: Vec<DiskStats>,
    pub agents: Vec<AgentLoad>,
}

impl SysStats {
    /// The fullest fixed (non-removable) disk, for the one-line summary.
    pub fn fullest_disk(&self) -> Option<&DiskStats> {
        self.disks
            .iter()
            .filter(|d| !d.removable && d.used_percent.is_some())
            .max_by(|a, b| {
                a.used_percent
                    .partial_cmp(&b.used_percent)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }
}

/// Short mount label: `C:` for `C:\`, otherwise the mount point itself (`/` stays `/`).
pub fn mount_label(mount: &str) -> String {
    let m = mount.trim_end_matches(['\\', '/']);
    if m.is_empty() {
        mount.to_string()
    } else {
        m.to_string()
    }
}

/// `part / total` as a percentage, `None` when the total is unknown (0) — never a fake 0%.
pub fn pct(part: u64, total: u64) -> Option<f64> {
    (total > 0).then(|| part.min(total) as f64 * 100.0 / total as f64)
}

/// Binary units like Windows Task Manager / Explorer (1 GB = 1024³ bytes).
pub fn human_bytes(n: u64) -> String {
    const K: f64 = 1024.0;
    let f = n as f64;
    if f >= K * K * K * K {
        format!("{:.1} TB", f / (K * K * K * K))
    } else if f >= K * K * K {
        format!("{:.1} GB", f / (K * K * K))
    } else if f >= K * K {
        format!("{:.0} MB", f / (K * K))
    } else if f >= K {
        format!("{:.0} KB", f / K)
    } else {
        format!("{n} B")
    }
}

fn is_desktop_app(exe: &str) -> bool {
    let e = exe.to_ascii_lowercase().replace('\\', "/");
    e.contains("/windowsapps/") || e.contains(".app/contents/")
}

/// Agent label for a process that *is* an agent (not a child of one), using the names `agm ps` shows.
/// Desktop apps (Microsoft Store / macOS bundles) get an `-app` label so they are not mixed with CLI sessions.
pub fn agent_of(name: &str, exe: Option<&str>, cmd: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    let stem = lower.strip_suffix(".exe").unwrap_or(&lower);
    let app = exe.is_some_and(is_desktop_app);
    match stem {
        "claude" => Some(if app { "claude-app" } else { "claude-code" }),
        "codex" => Some(if app { "codex-app" } else { "codex" }),
        "opencode" => Some("opencode"),
        "agy" => Some("agy"),
        "antigravity" => Some("agy-app"),
        // npm installs run the agent's JS entry point under node/bun.
        "node" | "bun" => {
            let c = cmd.to_ascii_lowercase().replace('\\', "/");
            if c.contains("@anthropic-ai/claude-code") {
                Some("claude-code")
            } else if c.contains("@openai/codex") {
                Some("codex")
            } else if c.contains("opencode-ai") || c.contains("/opencode/") {
                Some("opencode")
            } else {
                None
            }
        }
        _ => None,
    }
}

/// One row of the process table, already classified.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcInfo {
    pub pid: u32,
    pub parent: Option<u32>,
    /// Start time in seconds since the epoch; guards against a parent pid that was reused by a newer process.
    pub start_s: u64,
    pub agent: Option<&'static str>,
    pub memory_bytes: u64,
    /// As the OS reports it: 100 = one full core.
    pub cpu_percent: f32,
}

/// Longest parent chain followed; deeper trees (or a parent loop) stop attributing.
const MAX_DEPTH: usize = 64;

/// Sums each process into the nearest agent among itself and its ancestors (a worker started by a CEO counts for
/// the worker). CPU is turned into a share of the whole machine. Sorted by memory, largest first.
pub fn attribute(procs: &[ProcInfo], logical_cores: usize, cpu_ready: bool) -> Vec<AgentLoad> {
    let by_pid: HashMap<u32, &ProcInfo> = procs.iter().map(|p| (p.pid, p)).collect();
    let mut sums: BTreeMap<&'static str, (usize, u64, f64)> = BTreeMap::new();
    for p in procs {
        let mut cur = p;
        let mut owner = None;
        for _ in 0..MAX_DEPTH {
            if let Some(a) = cur.agent {
                owner = Some(a);
                break;
            }
            match cur
                .parent
                .filter(|pp| *pp != cur.pid)
                .and_then(|pp| by_pid.get(&pp))
            {
                // A "parent" that started after its child is a reused pid, not the real parent.
                Some(parent) if parent.start_s <= cur.start_s => cur = parent,
                _ => break,
            }
        }
        if let Some(a) = owner {
            let e = sums.entry(a).or_default();
            e.0 += 1;
            e.1 += p.memory_bytes;
            e.2 += f64::from(p.cpu_percent.max(0.0));
        }
    }
    let cores = logical_cores.max(1) as f64;
    let mut out: Vec<AgentLoad> = sums
        .into_iter()
        .map(|(agent, (n, mem, cpu))| AgentLoad {
            agent: agent.to_string(),
            processes: n,
            memory_bytes: mem,
            cpu_percent: cpu_ready.then(|| (cpu / cores).clamp(0.0, 100.0)),
        })
        .collect();
    out.sort_by(|a, b| {
        b.memory_bytes
            .cmp(&a.memory_bytes)
            .then_with(|| a.agent.cmp(&b.agent))
    });
    out
}

/// Sorts by mount point and folds duplicates into the first one: the same mount listed twice, or several mounts
/// with identical file system, size and free bytes (one volume under several drive letters — two real disks never
/// match to the byte). Counting such a volume once keeps the header from reporting the same drive three times.
pub fn merge_disks(mut disks: Vec<DiskStats>) -> Vec<DiskStats> {
    disks.sort_by(|a, b| a.mount.cmp(&b.mount));
    let mut out: Vec<DiskStats> = Vec::new();
    for d in disks {
        let same = |x: &DiskStats| {
            x.mount == d.mount
                || (x.file_system == d.file_system
                    && x.total_bytes == d.total_bytes
                    && x.available_bytes == d.available_bytes)
        };
        match out.iter_mut().find(|x| same(x)) {
            Some(first) if first.mount != d.mount => first.also_mounted_at.push(d.mount),
            Some(_) => {}
            None => out.push(d),
        }
    }
    out
}

/// Virtual / pseudo file systems that are not real storage.
fn skip_fs(fs: &str) -> bool {
    matches!(
        fs.to_ascii_lowercase().as_str(),
        "tmpfs" | "devtmpfs" | "squashfs" | "overlay" | "ramfs" | "devfs" | "autofs"
    )
}

/// Keeps the OS handles between readings: CPU % and disk rates are differences between two readings.
pub struct SysSampler {
    sys: System,
    disks: Option<Disks>,
    /// When CPU, processes and disks were last refreshed.
    refreshed_at: Instant,
    last: Option<SysStats>,
}

fn proc_kind() -> ProcessRefreshKind {
    ProcessRefreshKind::nothing()
        .with_memory()
        .with_cpu()
        .with_exe(UpdateKind::OnlyIfNotSet)
        .with_cmd(UpdateKind::OnlyIfNotSet)
}

fn disk_kind() -> DiskRefreshKind {
    DiskRefreshKind::nothing().with_storage().with_io_usage()
}

impl SysSampler {
    /// Takes the baseline readings (blocks for about [`MINIMUM_CPU_UPDATE_INTERVAL`]). `None` when the platform
    /// offers no system information at all.
    pub fn new() -> Option<Self> {
        let mut sys = System::new().ok()?;
        sys.refresh_cpu_specifics(CpuRefreshKind::nothing().with_usage());
        sys.refresh_memory_specifics(MemoryRefreshKind::everything());
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, proc_kind());
        // sysinfo (Windows) keeps no CPU baseline on a process's first refresh, so the second refresh reports the
        // average since the process started. A second baseline refresh makes the next sample a real interval.
        std::thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL);
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, proc_kind());
        let disks = Disks::new_with_refreshed_list_specifics(disk_kind()).ok();
        Some(SysSampler {
            sys,
            disks,
            refreshed_at: Instant::now(),
            last: None,
        })
    }

    /// Fresh reading. Calls closer together than the OS minimum return the previous reading unchanged, so a
    /// redraw right after a keypress never shows a bogus 0% CPU.
    pub fn sample(&mut self) -> SysStats {
        let elapsed = self.refreshed_at.elapsed();
        if elapsed < MINIMUM_CPU_UPDATE_INTERVAL
            && let Some(last) = &self.last
        {
            return last.clone();
        }
        let cpu_ready = elapsed >= MINIMUM_CPU_UPDATE_INTERVAL;
        self.sys
            .refresh_cpu_specifics(CpuRefreshKind::nothing().with_usage());
        self.sys
            .refresh_memory_specifics(MemoryRefreshKind::everything());
        self.sys
            .refresh_processes_specifics(ProcessesToUpdate::All, true, proc_kind());
        if let Some(d) = self.disks.as_mut() {
            d.refresh_specifics(true, disk_kind());
        }
        self.refreshed_at = Instant::now();

        let cores = self.sys.cpus().len();
        let memory = (self.sys.total_memory() > 0).then(|| {
            MemStats::new(
                self.sys.total_memory(),
                self.sys.used_memory(),
                self.sys.total_swap(),
                self.sys.used_swap(),
            )
        });
        let cpu = (cpu_ready && cores > 0).then(|| CpuStats {
            usage_percent: f64::from(self.sys.global_cpu_usage()).clamp(0.0, 100.0),
            logical_cores: cores,
            per_core_percent: self
                .sys
                .cpus()
                .iter()
                .map(|c| f64::from(c.usage()).clamp(0.0, 100.0))
                .collect(),
        });
        let procs: Vec<ProcInfo> = self
            .sys
            .processes()
            .values()
            .map(|p| {
                let name = p.name().to_string_lossy();
                let exe = p.exe().map(|e| e.to_string_lossy().into_owned());
                let cmd = p
                    .cmd()
                    .iter()
                    .map(|a| a.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" ");
                ProcInfo {
                    pid: p.pid().as_u32(),
                    parent: p.parent().map(|pp| pp.as_u32()),
                    start_s: p.start_time(),
                    agent: agent_of(&name, exe.as_deref(), &cmd),
                    memory_bytes: p.memory(),
                    cpu_percent: p.cpu_usage(),
                }
            })
            .collect();
        let secs = elapsed.as_secs_f64();
        let rate = |bytes: u64| {
            (secs >= Duration::from_millis(50).as_secs_f64()).then(|| bytes as f64 / secs)
        };
        let mut disks: Vec<DiskStats> = Vec::new();
        for d in self.disks.iter().flat_map(|ds| ds.iter()) {
            let fs = d.file_system().to_string_lossy().into_owned();
            if d.total_space() == 0 || skip_fs(&fs) {
                continue;
            }
            let mount = d.mount_point().to_string_lossy();
            let mut s = DiskStats::new(
                &mount,
                &fs,
                d.total_space(),
                d.available_space(),
                d.is_removable(),
            );
            let io = d.usage();
            s.read_bytes_per_s = rate(io.read_bytes);
            s.write_bytes_per_s = rate(io.written_bytes);
            disks.push(s);
        }
        let disks = merge_disks(disks);
        let stats = SysStats {
            memory,
            cpu,
            disks,
            agents: attribute(&procs, cores, cpu_ready),
        };
        self.last = Some(stats.clone());
        stats
    }
}

/// One-shot reading for CLI use: waits long enough for CPU % and disk rates to mean something.
pub fn read_once() -> Option<SysStats> {
    let mut s = SysSampler::new()?;
    std::thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL.max(Duration::from_millis(500)));
    Some(s.sample())
}
