//! Local token / cost usage from Claude Code transcripts
//! (`~/.claude/projects/**/*.jsonl`). Incremental: a per-file cache keyed by
//! (size, mtime) with a byte offset so append-only files are only re-read
//! from where we left off. Dedup key is `message.id:requestId` (one JSONL
//! line per content block repeats the same usage).

use chrono::{DateTime, Local, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::pricing::{PriceBook, Rate};

const LOOKBACK_DAYS: i64 = 31;
const CACHE_VERSION: u32 = 2;

pub fn data_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".config").join("swapbar")
}

/// Every Claude Code `projects` directory we know about: the default
/// profile, `CLAUDE_CONFIG_DIR`, sibling profiles (`~/.claude-*`) and
/// claude-swap session profiles. Deduped by canonical path.
fn claude_projects_dirs() -> Vec<PathBuf> {
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(cfg) = std::env::var("CLAUDE_CONFIG_DIR") {
        candidates.push(PathBuf::from(cfg).join("projects"));
    }
    candidates.push(home.join(".claude").join("projects"));
    candidates.push(home.join(".config").join("claude").join("projects"));
    if let Ok(rd) = fs::read_dir(&home) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with(".claude-") && !name.starts_with(".claude-swap") {
                candidates.push(e.path().join("projects"));
            }
        }
    }
    if let Ok(rd) = fs::read_dir(home.join(".claude-swap-backup").join("sessions")) {
        for e in rd.flatten() {
            candidates.push(e.path().join("projects"));
        }
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for c in candidates {
        if !c.is_dir() {
            continue;
        }
        let canon = fs::canonicalize(&c).unwrap_or(c.clone());
        if seen.insert(canon) {
            out.push(c);
        }
    }
    out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rec {
    /// dedupe key hash
    pub k: u64,
    /// unix seconds
    pub t: i64,
    /// model id; shared so cloning a record does not allocate
    pub m: Arc<str>,
    pub i: u64,
    pub cr: u64,
    pub cc: u64,
    /// 1h cache creation subset of cc
    pub c1: u64,
    pub o: u64,
    /// incomplete streaming row (no cost)
    #[serde(default)]
    pub inc: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct FileEntry {
    size: u64,
    mtime: i64,
    offset: u64,
    recs: Vec<Rec>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Cache {
    version: u32,
    files: BTreeMap<String, FileEntry>,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

impl Cache {
    fn path() -> PathBuf {
        data_dir().join("usage-cache.json")
    }
    fn load() -> Cache {
        let Ok(text) = fs::read_to_string(Self::path()) else {
            return Cache {
                version: CACHE_VERSION,
                ..Default::default()
            };
        };
        match serde_json::from_str::<Cache>(&text) {
            Ok(c) if c.version == CACHE_VERSION => c,
            _ => Cache {
                version: CACHE_VERSION,
                ..Default::default()
            },
        }
    }
    fn save(&self) {
        let path = Self::path();
        let _ = fs::create_dir_all(path.parent().unwrap());
        if let Ok(text) = serde_json::to_string(self) {
            let tmp = path.with_extension("json.tmp");
            if fs::write(&tmp, text).is_ok() {
                let _ = fs::rename(tmp, path);
            }
        }
    }
}

// ---- JSONL row shape ----------------------------------------------------

#[derive(Deserialize)]
struct Row<'a> {
    #[serde(rename = "type")]
    kind: Option<&'a str>,
    timestamp: Option<&'a str>,
    #[serde(rename = "requestId")]
    request_id: Option<&'a str>,
    #[serde(rename = "sessionId")]
    session_id: Option<&'a str>,
    message: Option<Msg<'a>>,
}

#[derive(Deserialize)]
struct Msg<'a> {
    id: Option<&'a str>,
    model: Option<&'a str>,
    stop_reason: Option<&'a str>,
    usage: Option<UsageJson>,
}

#[derive(Deserialize, Default)]
struct UsageJson {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_creation: Option<CacheCreation>,
}

#[derive(Deserialize, Default)]
struct CacheCreation {
    #[serde(default)]
    ephemeral_1h_input_tokens: u64,
}

fn hash_key(a: &str, b: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    a.hash(&mut h);
    b.hash(&mut h);
    h.finish()
}

fn parse_line(line: &str, min_ts: i64) -> Option<Rec> {
    if !line.contains("\"assistant\"") || !line.contains("\"usage\"") {
        return None;
    }
    let row: Row = serde_json::from_str(line).ok()?;
    if row.kind? != "assistant" {
        return None;
    }
    let msg = row.message?;
    let usage = msg.usage?;
    let model = msg.model?;
    let ts = DateTime::parse_from_rfc3339(row.timestamp?)
        .ok()?
        .timestamp();
    if ts < min_ts {
        return None;
    }
    if usage.input_tokens == 0
        && usage.output_tokens == 0
        && usage.cache_creation_input_tokens == 0
        && usage.cache_read_input_tokens == 0
    {
        return None;
    }
    let msg_id = msg.id.unwrap_or("");
    let key = match (msg_id, row.request_id, row.session_id) {
        (id, Some(req), _) if !id.is_empty() => hash_key(id, req),
        (id, None, Some(sess)) if !id.is_empty() => hash_key(sess, id),
        _ => hash_key(row.timestamp.unwrap_or(""), line),
    };
    let incomplete = msg.stop_reason.is_none()
        && usage.output_tokens == 0
        && usage.input_tokens > 0
        && usage.cache_creation_input_tokens == 0
        && usage.cache_read_input_tokens == 0;
    let c1 = usage
        .cache_creation
        .map(|c| c.ephemeral_1h_input_tokens)
        .unwrap_or(0)
        .min(usage.cache_creation_input_tokens);
    Some(Rec {
        k: key,
        t: ts,
        m: model.into(),
        i: usage.input_tokens,
        cr: usage.cache_read_input_tokens,
        cc: usage.cache_creation_input_tokens,
        c1,
        o: usage.output_tokens,
        inc: incomplete,
    })
}

fn scan_file(path: &Path, from_offset: u64, min_ts: i64) -> std::io::Result<(Vec<Rec>, u64)> {
    let mut f = fs::File::open(path)?;
    f.seek(SeekFrom::Start(from_offset))?;
    let mut reader = BufReader::with_capacity(1 << 20, f);
    let mut recs: Vec<Rec> = Vec::new();
    let mut index: HashMap<u64, usize> = HashMap::new();
    let mut line = String::new();
    let mut consumed = from_offset;
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }
        if !line.ends_with('\n') {
            // partial trailing line (still being written) — leave for next pass
            break;
        }
        consumed += n as u64;
        if line.len() > 512 * 1024 {
            continue;
        }
        if let Some(rec) = parse_line(&line, min_ts) {
            match index.get(&rec.k) {
                // later row wins within a file (final streaming chunk), unless incomplete
                Some(&idx) => {
                    if !rec.inc || recs[idx].inc {
                        recs[idx] = rec;
                    }
                }
                None => {
                    index.insert(rec.k, recs.len());
                    recs.push(rec);
                }
            }
        }
    }
    Ok((recs, consumed))
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>, min_mtime: i64) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(md) = e.metadata() else { continue };
        if md.is_dir() {
            walk(&p, out, min_mtime);
        } else if p.extension().is_some_and(|x| x == "jsonl") {
            let mtime = md
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            if mtime >= min_mtime {
                out.push(p);
            }
        }
    }
}

/// All deduplicated records within the lookback window, refreshed from disk.
pub fn collect_records() -> Vec<Rec> {
    let now = chrono::Utc::now().timestamp();
    let min_ts = now - LOOKBACK_DAYS * 86400;
    // Kept in memory between scans; disk is only read on the first scan after launch.
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = guard.get_or_insert_with(Cache::load);

    let mut files = Vec::new();
    for d in claude_projects_dirs() {
        walk(&d, &mut files, min_ts - 36 * 3600);
    }
    files.sort();
    if std::env::var("SWAPBAR_DEBUG").is_ok() {
        eprintln!("jsonl files in window: {}", files.len());
    }

    let mut seen: HashSet<String> = HashSet::new();
    let mut changed = false;
    for path in &files {
        let key = path.to_string_lossy().to_string();
        seen.insert(key.clone());
        let Ok(md) = fs::metadata(path) else { continue };
        let size = md.len();
        let mtime = md
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let entry = cache.files.entry(key).or_default();
        if entry.size == size && entry.mtime == mtime {
            continue;
        }
        let (from, mut recs) = if size >= entry.offset && entry.offset > 0 {
            (entry.offset, std::mem::take(&mut entry.recs))
        } else {
            (0, Vec::new())
        };
        match scan_file(path, from, min_ts) {
            Ok((new_recs, consumed)) => {
                if std::env::var("SWAPBAR_DEBUG").is_ok() {
                    eprintln!(
                        "scanned {} from {from}: {} recs, consumed {consumed} of {size}",
                        path.display(),
                        new_recs.len()
                    );
                }
                if from == 0 {
                    recs = new_recs;
                } else {
                    let mut index: HashMap<u64, usize> =
                        recs.iter().enumerate().map(|(i, r)| (r.k, i)).collect();
                    for r in new_recs {
                        match index.get(&r.k) {
                            Some(&i) => {
                                if !r.inc || recs[i].inc {
                                    recs[i] = r;
                                }
                            }
                            None => {
                                index.insert(r.k, recs.len());
                                recs.push(r);
                            }
                        }
                    }
                }
                entry.recs = recs;
                entry.offset = consumed;
                entry.size = size;
                entry.mtime = mtime;
                changed = true;
            }
            Err(_) => {
                entry.recs = recs;
            }
        }
    }
    let before = cache.files.len();
    cache.files.retain(|k, _| seen.contains(k));
    if changed || cache.files.len() != before {
        cache.save();
    }

    // Cross-file dedup: files in sorted path order, first complete row wins.
    let mut out: Vec<Rec> = Vec::new();
    let mut index: HashMap<u64, usize> = HashMap::new();
    for entry in cache.files.values() {
        for r in &entry.recs {
            if r.t < min_ts {
                continue;
            }
            match index.get(&r.k) {
                Some(&i) => {
                    if out[i].inc && !r.inc {
                        out[i] = r.clone();
                    }
                }
                None => {
                    index.insert(r.k, out.len());
                    out.push(r.clone());
                }
            }
        }
    }
    out
}

// ---- Aggregation -----------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    Today,
    Week,
    Month,
}

impl Period {
    pub fn label(self) -> &'static str {
        match self {
            Period::Today => "Bugün",
            Period::Week => "7 gün",
            Period::Month => "30 gün",
        }
    }
    fn start(self) -> i64 {
        let now = Local::now();
        let today = now.date_naive();
        let day = |d: NaiveDate| {
            Local
                .from_local_datetime(&d.and_hms_opt(0, 0, 0).unwrap())
                .single()
                .map(|t| t.timestamp())
                .unwrap_or(0)
        };
        match self {
            Period::Today => day(today),
            Period::Week => day(today - chrono::Duration::days(6)),
            Period::Month => day(today - chrono::Duration::days(29)),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Totals {
    pub input: u64,
    pub cache_read: u64,
    pub cache_create: u64,
    pub output: u64,
    pub cost: f64,
    pub requests: u64,
    pub unpriced: u64,
}

impl Totals {
    pub fn tokens(&self) -> u64 {
        self.input + self.cache_read + self.cache_create + self.output
    }
    fn add(&mut self, r: &Rec, cost: Option<f64>) {
        self.input += r.i;
        self.cache_read += r.cr;
        self.cache_create += r.cc;
        self.output += r.o;
        self.requests += 1;
        match cost {
            Some(c) => self.cost += c,
            None => self.unpriced += 1,
        }
    }
    fn merge(&mut self, o: &Totals) {
        self.input += o.input;
        self.cache_read += o.cache_read;
        self.cache_create += o.cache_create;
        self.output += o.output;
        self.requests += o.requests;
        self.cost += o.cost;
        self.unpriced += o.unpriced;
    }
}

#[derive(Debug, Clone, Default)]
pub struct Summary {
    pub total: Totals,
    /// sorted by cost desc
    pub by_model: Vec<(String, Totals)>,
    /// newest first
    pub by_day: Vec<(NaiveDate, Totals)>,
}

pub fn summarize(recs: &[Rec], period: Period, prices: &PriceBook) -> Summary {
    let start = period.start();
    let mut total = Totals::default();
    let mut by_model: HashMap<String, Totals> = HashMap::new();
    let mut by_day: BTreeMap<NaiveDate, Totals> = BTreeMap::new();
    // Price and group by raw model id (one lookup per distinct id), normalize names afterwards.
    let mut by_raw: HashMap<&str, (Option<Rate>, Totals)> = HashMap::new();
    for r in recs {
        if r.t < start {
            continue;
        }
        let (rate, totals) = by_raw
            .entry(&r.m)
            .or_insert_with(|| (prices.lookup(&r.m), Totals::default()));
        let cost = if r.inc {
            Some(0.0)
        } else {
            rate.map(|rate| rate.cost(r.i, r.cr, r.cc - r.c1, r.c1, r.o))
        };
        total.add(r, cost);
        totals.add(r, cost);
        let day = Local
            .timestamp_opt(r.t, 0)
            .single()
            .map(|d| d.date_naive())
            .unwrap_or_default();
        by_day.entry(day).or_default().add(r, cost);
    }
    for (raw, (_, t)) in by_raw {
        by_model
            .entry(crate::pricing::normalize_model(raw))
            .or_default()
            .merge(&t);
    }
    let mut by_model: Vec<_> = by_model.into_iter().collect();
    by_model.sort_by(|a, b| {
        b.1.cost
            .partial_cmp(&a.1.cost)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.1.tokens().cmp(&a.1.tokens()))
    });
    let by_day: Vec<_> = by_day.into_iter().rev().collect();
    Summary {
        total,
        by_model,
        by_day,
    }
}

pub fn format_tokens(n: u64) -> String {
    let f = n as f64;
    let (v, suffix) = if f >= 1e12 {
        (f / 1e12, "T")
    } else if f >= 1e9 {
        (f / 1e9, "B")
    } else if f >= 1e6 {
        (f / 1e6, "M")
    } else if f >= 1e3 {
        (f / 1e3, "K")
    } else {
        return n.to_string();
    };
    let s = if v < 10.0 {
        format!("{v:.2}")
    } else if v < 100.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.0}")
    };
    let s = if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    };
    format!("{s}{suffix}")
}

pub fn format_usd(v: f64) -> String {
    let whole = v.trunc() as i64;
    let cents = ((v - v.trunc()) * 100.0).round() as i64;
    let mut s = whole.to_string();
    let mut grouped = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    s = grouped.chars().rev().collect();
    format!("${s}.{cents:02}")
}
