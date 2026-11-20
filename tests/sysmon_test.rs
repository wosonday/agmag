use agmag::config::Config;
use agmag::dashboard::{Section, parse_view, render_sys_report, render_view, snapshot};
use agmag::store::Store;
use agmag::sysmon::{
    AgentLoad, CpuStats, DiskStats, MemStats, ProcInfo, SysStats, agent_of, attribute, human_bytes,
    merge_disks, pct, volume_id,
};

const GB: u64 = 1024 * 1024 * 1024;

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for ch in chars.by_ref() {
                if ch.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn proc(
    pid: u32,
    parent: Option<u32>,
    start_s: u64,
    agent: Option<&'static str>,
    mem_gb: u64,
    cpu: f32,
) -> ProcInfo {
    ProcInfo {
        pid,
        parent,
        start_s,
        agent,
        memory_bytes: mem_gb * GB,
        cpu_percent: cpu,
    }
}

#[test]
fn classifies_agent_processes_by_name_path_and_npm_entry() {
    assert_eq!(
        agent_of("claude.exe", Some(r"C:\Users\u\.local\bin\claude.exe"), ""),
        Some("claude-code")
    );
    assert_eq!(
        agent_of(
            "claude.exe",
            Some(r"C:\Program Files\WindowsApps\Claude_6.6_x64\app\claude.exe"),
            ""
        ),
        Some("claude-app")
    );
    assert_eq!(
        agent_of(
            "Codex.exe",
            Some(r"C:\Program Files\WindowsApps\OpenAI.Codex\codex.exe"),
            ""
        ),
        Some("codex-app")
    );
    assert_eq!(
        agent_of("codex", Some("/usr/local/bin/codex"), ""),
        Some("codex")
    );
    assert_eq!(agent_of("opencode.exe", None, ""), Some("opencode"));
    assert_eq!(agent_of("agy.exe", None, ""), Some("agy"));
    assert_eq!(agent_of("Antigravity.exe", None, ""), Some("agy-app"));
    assert_eq!(
        agent_of(
            "Claude",
            Some("/Applications/Claude.app/Contents/MacOS/Claude"),
            ""
        ),
        Some("claude-app")
    );
    // npm installs: the agent is the JS entry point under node.
    assert_eq!(
        agent_of(
            "node.exe",
            None,
            r"node C:\npm\node_modules\@anthropic-ai\claude-code\cli.js"
        ),
        Some("claude-code")
    );
    assert_eq!(
        agent_of(
            "node",
            None,
            "node /usr/lib/node_modules/@openai/codex/bin/codex.js"
        ),
        Some("codex")
    );
    // Edge: an MCP server under node, or a lookalike name, is not an agent by itself.
    assert_eq!(
        agent_of(
            "node.exe",
            None,
            "node @modelcontextprotocol/server-filesystem"
        ),
        None
    );
    assert_eq!(agent_of("claude-helper.exe", None, ""), None);
    assert_eq!(agent_of("cargo.exe", None, ""), None);
}

#[test]
fn attributes_descendants_to_the_nearest_agent() {
    let procs = vec![
        proc(1, None, 0, None, 1, 50.0), // explorer: no agent
        proc(10, Some(1), 10, Some("claude-code"), 1, 100.0), // CEO session
        proc(11, Some(10), 11, None, 2, 200.0), // its cargo build
        proc(20, Some(10), 12, Some("opencode"), 1, 0.0), // worker spawned by the CEO
        proc(21, Some(20), 13, None, 1, 100.0), // worker's shell
    ];
    let loads = attribute(&procs, 4, true);
    assert_eq!(
        loads,
        vec![
            AgentLoad {
                agent: "claude-code".into(),
                processes: 2,
                memory_bytes: 3 * GB,
                cpu_percent: Some(75.0)
            },
            AgentLoad {
                agent: "opencode".into(),
                processes: 2,
                memory_bytes: 2 * GB,
                cpu_percent: Some(25.0)
            },
        ]
    );
}

#[test]
fn attribute_survives_pid_reuse_cycles_and_unready_cpu() {
    let procs = vec![
        // Parent pid 10 was reused by an agent that started *after* this orphan: not its real parent.
        proc(10, None, 500, Some("codex"), 1, 0.0),
        proc(11, Some(10), 100, None, 4, 0.0),
        // A parent loop (corrupt table) must terminate and attribute nothing.
        proc(30, Some(31), 1, None, 1, 0.0),
        proc(31, Some(30), 1, None, 1, 0.0),
        // A process that is its own parent.
        proc(40, Some(40), 1, None, 1, 0.0),
    ];
    let loads = attribute(&procs, 0, false);
    assert_eq!(
        loads,
        vec![AgentLoad {
            agent: "codex".into(),
            processes: 1,
            memory_bytes: GB,
            cpu_percent: None
        }]
    );
    assert!(attribute(&[], 8, true).is_empty());
}

#[test]
fn percentages_and_units() {
    assert_eq!(pct(1, 4), Some(25.0));
    assert_eq!(pct(5, 0), None, "unknown total must not read as 0%");
    assert_eq!(pct(9, 4), Some(100.0));
    assert_eq!(human_bytes(512), "512 B");
    assert_eq!(human_bytes(3 * 1024 * 1024), "3 MB");
    assert_eq!(human_bytes(31 * GB + GB / 2), "31.5 GB");
    assert_eq!(human_bytes(2 * 1024 * GB), "2.0 TB");
    let d = DiskStats::new("C:\\", "NTFS", 100 * GB, 25 * GB, false);
    assert_eq!(
        (d.label(), d.used_bytes(), d.used_percent),
        ("C:".to_string(), 75 * GB, Some(75.0))
    );
    assert_eq!(DiskStats::new("/", "ext4", 0, 0, false).label(), "/");
}

#[test]
fn merge_disks_uses_volume_identity() {
    // Two disks with identical fs/size/free but different volume ids: must stay separate.
    let merged = merge_disks(vec![
        DiskStats::new("G:\\", "FAT32", 474 * GB, 127 * GB, false).with_volume_id("vol-g"),
        DiskStats::new("H:\\", "FAT32", 474 * GB, 127 * GB, false).with_volume_id("vol-h"),
    ]);
    let mounts: Vec<&str> = merged.iter().map(|d| d.mount.as_str()).collect();
    assert_eq!(mounts, ["G:\\", "H:\\"]);
    assert!(merged.iter().all(|d| d.also_mounted_at.is_empty()));

    // Different mounts, same volume id: merged into one row (mount-sorted), also_mounted_at correct.
    let merged = merge_disks(vec![
        DiskStats::new("H:\\", "FAT32", 474 * GB, 127 * GB, false).with_volume_id("vol-shared"),
        DiskStats::new("G:\\", "FAT32", 474 * GB, 127 * GB, false).with_volume_id("vol-shared"),
    ]);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].mount, "G:\\");
    assert_eq!(merged[0].also_mounted_at, vec!["H:\\"]);

    // Both volume_id None and identical stats: must NOT merge.
    let merged = merge_disks(vec![
        DiskStats::new("G:\\", "FAT32", 474 * GB, 127 * GB, false),
        DiskStats::new("H:\\", "FAT32", 474 * GB, 127 * GB, false),
    ]);
    assert_eq!(merged.len(), 2);

    // Same mount listed twice: one row. Empty list: empty.
    let merged = merge_disks(vec![
        DiskStats::new("C:\\", "NTFS", 474 * GB, 134 * GB, false).with_volume_id("vol-c"),
        DiskStats::new("C:\\", "NTFS", 474 * GB, 134 * GB, false).with_volume_id("vol-c"),
    ]);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].mount, "C:\\");
    assert!(merged[0].also_mounted_at.is_empty());
    assert!(merge_disks(Vec::new()).is_empty());
}

#[cfg(windows)]
#[test]
fn volume_id_reads_windows_guid_path() {
    let id = volume_id("C:\\");
    let s = id.expect("C:\\ should have a volume id");
    assert!(s.starts_with(r"\\?\Volume{"), "got: {s}");

    // Non-existent mount / garbage path should return None, not panic.
    assert!(volume_id("not-a-mount").is_none());
}

fn sample_stats() -> SysStats {
    let mut c = DiskStats::new("C:\\", "NTFS", 476 * GB, 136 * GB, false);
    c.read_bytes_per_s = Some(2.0 * 1024.0 * 1024.0);
    c.write_bytes_per_s = Some(0.0);
    c.also_mounted_at = vec!["K:\\".into()];
    SysStats {
        memory: Some(MemStats::new(32 * GB, 29 * GB, 8 * GB, GB)),
        cpu: Some(CpuStats {
            usage_percent: 23.0,
            logical_cores: 4,
            per_core_percent: vec![3.0, 97.0, 45.0, 70.0],
        }),
        disks: vec![c, DiskStats::new("E:\\", "FAT32", 64 * GB, 60 * GB, true)],
        agents: vec![AgentLoad {
            agent: "claude-code".into(),
            processes: 35,
            memory_bytes: 3 * GB,
            cpu_percent: Some(4.6),
        }],
    }
}

#[test]
fn fullest_disk_ignores_removable_drives() {
    let s = sample_stats();
    assert_eq!(s.fullest_disk().map(|d| d.mount.as_str()), Some("C:\\"));
    assert!(SysStats::default().fullest_disk().is_none());
}

#[test]
fn tui_shows_system_summary_in_header_and_system_section() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let mut s = snapshot(&store, &Config::builtin(), 1_000_000)?;
    assert!(s.system.is_none(), "the database has no system readings");
    let plain = strip_ansi(&render_view(&s, 120, &[Section::System], None));
    assert!(
        !plain.contains("cpu "),
        "no header summary when nothing was measured"
    );
    assert!(plain.contains("system metrics unavailable"));

    s.system = Some(sample_stats());
    let raw = render_view(&s, 120, &[Section::System], None);
    let plain = strip_ansi(&raw);
    // Header: CPU, RAM (91% → red) and the fullest fixed disk.
    assert!(plain.contains("cpu 23% ram 91% C: 71%"), "{plain}");
    assert!(raw.contains("\x1b[91m91%"), "RAM above 85% must be red");
    for needle in [
        "SYSTEM",
        "● HIGH LOAD  cpu 23% · ram 91%  → hold off spawning new workers",
        "▣ CPU",
        "4 cores · busiest #1 97% · 1 idle",
        "┊ cores  ▁█▄▆",
        "▤ RAM",
        "29.0 GB / 32.0 GB",
        "↺ swap",
        "1.0 GB / 8.0 GB",
        "◫ C:",
        "136.0 GB free of 476.0 GB",
        "↓ 2 MB/s  ↑ 0 B/s",
        "(also K:)",
        "E:",
        "removable",
        "share of RAM",
        "● claude-code",
        "9%",
        "3.0 GB",
        "35",
        "4.6%",
        "Σ SUM",
        "9% of RAM",
    ] {
        assert!(plain.contains(needle), "missing {needle:?} in:\n{plain}");
    }
    // claude-code keeps its TERMINALS color (bright magenta) for the dot, name and RAM-share bar.
    assert!(
        raw.contains("\x1b[95m●"),
        "agent dot must use the agent's color"
    );

    // CPU not ready yet (first frame) and no agents: n/a, never a fake 0%.
    let mut early = sample_stats();
    early.cpu = None;
    early.agents.clear();
    s.system = Some(early);
    let plain = strip_ansi(&render_view(&s, 120, &[Section::System], None));
    assert!(plain.contains("cpu n/a"), "{plain}");
    assert!(plain.contains("(no agent processes running)"));
    // Narrow pane: every line stays within the width.
    s.system = Some(sample_stats());
    for line in strip_ansi(&render_view(&s, 40, &[Section::System], None)).lines() {
        assert!(
            unicode_width::UnicodeWidthStr::width(line) <= 39,
            "too wide: {line:?}"
        );
    }
    Ok(())
}

#[test]
fn sys_report_has_sections_verdicts_and_fits_width() {
    // Calm machine: green OK verdict and all three sections.
    let mut calm = sample_stats();
    calm.memory = Some(MemStats::new(32 * GB, 8 * GB, 0, 0));
    let raw = render_sys_report(&calm, 120);
    let plain = strip_ansi(&raw);
    for needle in [
        "◉ MACHINE",
        "◫ DISKS",
        "◆ AGENTS (each with its whole process tree)",
        "● OK",
        "room for more workers",
    ] {
        assert!(plain.contains(needle), "missing {needle:?} in:\n{plain}");
    }
    assert!(
        raw.contains("\x1b[1;92m● OK"),
        "OK verdict must be bold green"
    );
    // RAM-share bar has 1/8-cell steps: 5% is half a cell, 0% is empty, not a rounded-away blank.
    calm.agents = vec![
        AgentLoad {
            agent: "opencode".into(),
            processes: 9,
            memory_bytes: 32 * GB / 20,
            cpu_percent: Some(1.7),
        },
        AgentLoad {
            agent: "codex".into(),
            processes: 1,
            memory_bytes: 0,
            cpu_percent: Some(0.0),
        },
    ];
    let plain = strip_ansi(&render_sys_report(&calm, 120));
    assert!(plain.contains("● opencode     ▌░░░░░░░░░   5%"), "{plain}");
    assert!(plain.contains("● codex        ░░░░░░░░░░   0%"), "{plain}");
    assert!(!plain.contains("↺ swap"), "no swap row without swap");
    // `agm sys` shows every core as a grid under the CPU bar (TUI keeps the one-line spark instead).
    assert!(!plain.contains("┊ cores"), "{plain}");
    assert!(
        plain.contains(
            "           #0 ▎░░░░░░░   3%   #1 ███████▊  97%   #2 ███▋░░░░  45%   #3 █████▋░░  70%"
        ),
        "{plain}"
    );
    let narrow = strip_ansi(&render_sys_report(&calm, 40));
    let rows: Vec<&str> = narrow
        .lines()
        .filter(|l| l.trim_start().starts_with('#'))
        .collect();
    assert_eq!(
        rows.len(),
        4,
        "one core per row when only one fits:\n{narrow}"
    );

    // Busy (60–85%) is yellow; a fixed disk at 90%+ adds a red warning.
    let mut busy = sample_stats();
    busy.memory = Some(MemStats::new(32 * GB, 22 * GB, 0, 0));
    busy.disks = vec![DiskStats::new("/", "ext4", 100 * GB, 5 * GB, false)];
    let raw = render_sys_report(&busy, 120);
    let plain = strip_ansi(&raw);
    assert!(raw.contains("\x1b[1;93m● BUSY"), "{plain}");
    assert!(
        plain.contains("spawn new workers with care") && plain.contains("⚠ / nearly full"),
        "{plain}"
    );

    // Nothing measured yet: no fake figures.
    let unknown = SysStats::default();
    let plain = strip_ansi(&render_sys_report(&unknown, 120));
    assert!(
        plain.contains("○ UNKNOWN")
            && plain.contains("cpu n/a")
            && plain.contains("(no agent processes running)")
    );
    assert!(!plain.contains("◫ DISKS"), "no DISKS section without disks");

    for width in [60, 80] {
        for line in strip_ansi(&render_sys_report(&sample_stats(), width)).lines() {
            assert!(
                unicode_width::UnicodeWidthStr::width(line) < width,
                "too wide at {width}: {line:?}"
            );
        }
    }
}

#[test]
fn system_is_a_selectable_view() {
    assert_eq!(
        parse_view("system quota"),
        Ok(vec![Section::Quota, Section::System])
    );
    assert!(Section::names().contains("system"));
    assert!(!Section::DEFAULT.contains(&Section::System));
}
