use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write as _};
use std::path::{Component, Path, PathBuf};

use crate::hil_record::{
    EVIDENCE_PREFIX, Record, parse_fields, repository_path, require_hex, sha256_file,
};

const REPORT_SCHEMA: u64 = 1;
const SAFE_IMAGE: u32 = board_mks_tinybee::DESCRIBED_SAFE_I2S_IMAGE;
const IMAGE_WIDTH: usize = board_mks_tinybee::SHIFT_CHAIN_WIDTH as usize;
const FRAME_BITS: usize = 64;
const IMAGE_MASK: u32 = if IMAGE_WIDTH == u32::BITS as usize {
    u32::MAX
} else {
    (1_u32 << IMAGE_WIDTH) - 1
};
const MAXIMUM_MARKER_CODE: u64 = 32;
const LONG_MARKER_MINIMUM_PS: u64 = 500_000_000;
const LONG_MARKER_MAXIMUM_PS: u64 = 2_000_000_000;
const SHORT_MARKER_MINIMUM_PS: u64 = 50_000_000;
const SHORT_MARKER_MAXIMUM_PS: u64 = 250_000_000;

const REPORT_FIELDS: &[&str] = &[
    "schema",
    "source_vcd_sha256",
    "decoded_marker_code",
    "decoded_static_image",
    "decoded_stream_image",
    "decoded_live_frames",
    "decoded_live_tail_bclk_count",
    "decoded_post_stop_complete_frames",
    "decoded_post_stop_safe_latches",
    "decoded_invalid_image_count",
    "decoded_non64_frame_count",
    "minimum_bclk_period_ps",
    "maximum_bclk_period_ps",
    "minimum_frame_period_ps",
    "maximum_frame_period_ps",
    "minimum_data_setup_ps",
    "minimum_data_hold_ps",
];

/// Measurements reconstructed directly from the four-channel VCD edge stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TinyBeePcmVcdAnalysis {
    pub decoded_marker_code: u64,
    pub decoded_static_image: u64,
    pub decoded_stream_image: u64,
    pub decoded_live_frames: u64,
    pub decoded_live_tail_bclk_count: u64,
    pub decoded_post_stop_complete_frames: u64,
    pub decoded_post_stop_safe_latches: u64,
    pub decoded_invalid_image_count: u64,
    pub decoded_non64_frame_count: u64,
    pub minimum_bclk_period_ps: u64,
    pub maximum_bclk_period_ps: u64,
    pub minimum_frame_period_ps: u64,
    pub maximum_frame_period_ps: u64,
    pub minimum_data_setup_ps: u64,
    pub minimum_data_hold_ps: u64,
}

impl TinyBeePcmVcdAnalysis {
    /// Parses a canonical analyzer report and binds it to one VCD digest.
    pub fn read_report(path: &Path) -> Result<(String, Self), String> {
        let source = fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        let fields = parse_fields(&source, path, REPORT_FIELDS)?;
        let record = Record::new(&fields, path);
        if record.u64("schema")? != REPORT_SCHEMA {
            return Err(record.error("schema", "is not the exact supported analysis schema"));
        }
        let source_vcd_sha256 = require_hex(&record, "source_vcd_sha256", 64)?;
        let analysis = Self {
            decoded_marker_code: record.u64("decoded_marker_code")?,
            decoded_static_image: record.u64("decoded_static_image")?,
            decoded_stream_image: record.u64("decoded_stream_image")?,
            decoded_live_frames: record.u64("decoded_live_frames")?,
            decoded_live_tail_bclk_count: record.u64("decoded_live_tail_bclk_count")?,
            decoded_post_stop_complete_frames: record.u64("decoded_post_stop_complete_frames")?,
            decoded_post_stop_safe_latches: record.u64("decoded_post_stop_safe_latches")?,
            decoded_invalid_image_count: record.u64("decoded_invalid_image_count")?,
            decoded_non64_frame_count: record.u64("decoded_non64_frame_count")?,
            minimum_bclk_period_ps: record.u64("minimum_bclk_period_ps")?,
            maximum_bclk_period_ps: record.u64("maximum_bclk_period_ps")?,
            minimum_frame_period_ps: record.u64("minimum_frame_period_ps")?,
            maximum_frame_period_ps: record.u64("maximum_frame_period_ps")?,
            minimum_data_setup_ps: record.u64("minimum_data_setup_ps")?,
            minimum_data_hold_ps: record.u64("minimum_data_hold_ps")?,
        };
        analysis.validate()?;
        Ok((source_vcd_sha256, analysis))
    }

    fn validate(self) -> Result<(), String> {
        if self.decoded_marker_code > MAXIMUM_MARKER_CODE {
            return Err("analysis marker code exceeds the fixture grammar".to_owned());
        }
        if self.decoded_static_image & !u64::from(IMAGE_MASK) != 0
            || self.decoded_stream_image & !u64::from(IMAGE_MASK) != 0
        {
            return Err("analysis image lies outside the physical shift chain".to_owned());
        }
        if self.decoded_post_stop_safe_latches > self.decoded_post_stop_complete_frames {
            return Err("analysis has more safe latches than complete post-stop frames".to_owned());
        }
        for (name, minimum, maximum) in [
            (
                "BCLK",
                self.minimum_bclk_period_ps,
                self.maximum_bclk_period_ps,
            ),
            (
                "frame",
                self.minimum_frame_period_ps,
                self.maximum_frame_period_ps,
            ),
        ] {
            if minimum > maximum || (minimum == 0) != (maximum == 0) {
                return Err(format!("analysis {name} period range is inconsistent"));
            }
        }
        Ok(())
    }

    pub(crate) fn report(self, source_vcd_sha256: &str) -> String {
        format!(
            concat!(
                "schema = 1\n",
                "source_vcd_sha256 = \"{}\"\n",
                "decoded_marker_code = {}\n",
                "decoded_static_image = {}\n",
                "decoded_stream_image = {}\n",
                "decoded_live_frames = {}\n",
                "decoded_live_tail_bclk_count = {}\n",
                "decoded_post_stop_complete_frames = {}\n",
                "decoded_post_stop_safe_latches = {}\n",
                "decoded_invalid_image_count = {}\n",
                "decoded_non64_frame_count = {}\n",
                "minimum_bclk_period_ps = {}\n",
                "maximum_bclk_period_ps = {}\n",
                "minimum_frame_period_ps = {}\n",
                "maximum_frame_period_ps = {}\n",
                "minimum_data_setup_ps = {}\n",
                "minimum_data_hold_ps = {}\n"
            ),
            source_vcd_sha256,
            self.decoded_marker_code,
            self.decoded_static_image,
            self.decoded_stream_image,
            self.decoded_live_frames,
            self.decoded_live_tail_bclk_count,
            self.decoded_post_stop_complete_frames,
            self.decoded_post_stop_safe_latches,
            self.decoded_invalid_image_count,
            self.decoded_non64_frame_count,
            self.minimum_bclk_period_ps,
            self.maximum_bclk_period_ps,
            self.minimum_frame_period_ps,
            self.maximum_frame_period_ps,
            self.minimum_data_setup_ps,
            self.minimum_data_hold_ps,
        )
    }
}

/// Analyzes one retained VCD below `docs/hil/runs` without loading it in memory.
pub fn analyze(root: &Path, vcd_path: &Path) -> Result<(String, TinyBeePcmVcdAnalysis), String> {
    let vcd_path = repository_path(root, vcd_path, EVIDENCE_PREFIX, "VCD capture")?;
    let digest = sha256_file(&vcd_path)?;
    let file = File::open(&vcd_path)
        .map_err(|error| format!("cannot open VCD {}: {error}", vcd_path.display()))?;
    let analysis = VcdReader::new(&vcd_path).read(BufReader::new(file))?;
    Ok((digest, analysis))
}

/// Writes a new canonical report below `docs/hil/runs`, refusing overwrite.
pub fn analyze_to_report(root: &Path, vcd_path: &Path, report_path: &Path) -> Result<(), String> {
    let (digest, analysis) = analyze(root, vcd_path)?;
    let report_path = new_evidence_path(root, report_path, "analysis report")?;
    let report = analysis.report(&digest);
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&report_path)
        .map_err(|error| format!("cannot create new {}: {error}", report_path.display()))?;
    output
        .write_all(report.as_bytes())
        .and_then(|()| output.sync_all())
        .map_err(|error| format!("cannot write {}: {error}", report_path.display()))?;
    println!(
        "analyzed {} live frames; marker={} safe=0x{:06x}; report={}",
        analysis.decoded_live_frames,
        analysis.decoded_marker_code,
        analysis.decoded_stream_image,
        report_path.display()
    );
    Ok(())
}

fn new_evidence_path(root: &Path, value: &Path, kind: &str) -> Result<PathBuf, String> {
    if value.is_absolute()
        || value.as_os_str().is_empty()
        || !value
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        || !value.starts_with(EVIDENCE_PREFIX)
    {
        return Err(format!(
            "{kind} path {} must be a new file below `{EVIDENCE_PREFIX}`",
            value.display()
        ));
    }
    let destination = root.join(value);
    if destination.exists() {
        return Err(format!(
            "{kind} {} already exists; refusing overwrite",
            destination.display()
        ));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| format!("{kind} {} has no parent", destination.display()))?;
    let canonical_root = fs::canonicalize(root)
        .map_err(|error| format!("cannot resolve repository root {}: {error}", root.display()))?;
    let canonical_prefix = fs::canonicalize(root.join(EVIDENCE_PREFIX)).map_err(|error| {
        format!(
            "cannot resolve evidence directory {}: {error}",
            root.join(EVIDENCE_PREFIX).display()
        )
    })?;
    let canonical_parent = fs::canonicalize(parent)
        .map_err(|error| format!("cannot resolve {kind} parent {}: {error}", parent.display()))?;
    if !canonical_prefix.starts_with(&canonical_root)
        || !canonical_parent.starts_with(&canonical_prefix)
    {
        return Err(format!(
            "{kind} path {} resolves outside `{EVIDENCE_PREFIX}`",
            value.display()
        ));
    }
    Ok(destination)
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Signal {
    Bclk = 0,
    Ws = 1,
    Data = 2,
    Marker = 3,
}

impl Signal {
    const ALL: [Self; 4] = [Self::Bclk, Self::Ws, Self::Data, Self::Marker];

    const fn index(self) -> usize {
        self as usize
    }

    fn from_name(name: &str) -> Option<Self> {
        let canonical: String = name
            .bytes()
            .filter(u8::is_ascii_alphanumeric)
            .map(|byte| char::from(byte.to_ascii_uppercase()))
            .collect();
        match canonical.as_str() {
            "D0" | "BCLK" | "SRCLK" | "GPIO25" => Some(Self::Bclk),
            "D1" | "WS" | "RCLK" | "GPIO26" => Some(Self::Ws),
            "D2" | "DATA" | "SER" | "GPIO27" => Some(Self::Data),
            "D3" | "MARKER" | "LCDRS" | "LCDRSO" | "GPIO4" => Some(Self::Marker),
            _ => None,
        }
    }
}

struct VcdReader<'a> {
    source: &'a Path,
    timescale_femtoseconds: Option<u64>,
    identifiers: BTreeMap<String, Signal>,
    assigned: [bool; 4],
}

impl<'a> VcdReader<'a> {
    const fn new(source: &'a Path) -> Self {
        Self {
            source,
            timescale_femtoseconds: None,
            identifiers: BTreeMap::new(),
            assigned: [false; 4],
        }
    }

    fn read<R: BufRead>(mut self, reader: R) -> Result<TinyBeePcmVcdAnalysis, String> {
        let mut analyzer = EdgeAnalyzer::default();
        let mut in_header = true;
        let mut directive = String::new();
        let mut current_tick = 0_u64;
        let mut changes = [None; 4];
        let mut have_group = false;
        let mut skip_data_directive = false;
        let mut value_data_directive = false;

        for (index, line) in reader.lines().enumerate() {
            let line_number = index + 1;
            let line = line.map_err(|error| {
                format!(
                    "{}:{line_number}: cannot read VCD: {error}",
                    self.source.display()
                )
            })?;
            let trimmed = line.trim();
            if in_header {
                if !directive.is_empty() || trimmed.starts_with('$') {
                    if !directive.is_empty() {
                        directive.push(' ');
                    }
                    directive.push_str(trimmed);
                    if directive
                        .split_ascii_whitespace()
                        .any(|token| token == "$end")
                    {
                        self.header_directive(&directive, line_number)?;
                        if directive.starts_with("$enddefinitions") {
                            in_header = false;
                        }
                        directive.clear();
                    }
                } else if !trimmed.is_empty() {
                    return Err(self.error(line_number, "unexpected text in VCD header"));
                }
                continue;
            }

            if skip_data_directive {
                if trimmed
                    .split_ascii_whitespace()
                    .any(|token| token == "$end")
                {
                    skip_data_directive = false;
                }
                continue;
            }
            if value_data_directive && trimmed == "$end" {
                value_data_directive = false;
                continue;
            }
            if trimmed.starts_with('$') {
                let tokens: Vec<_> = trimmed.split_ascii_whitespace().collect();
                let ended = tokens.contains(&"$end");
                if matches!(
                    tokens.first().copied(),
                    Some("$dumpvars" | "$dumpall" | "$dumpon" | "$dumpoff")
                ) {
                    if tokens.iter().skip(1).any(|token| *token != "$end") {
                        return Err(
                            self.error(line_number, "inline VCD dump values are not supported")
                        );
                    }
                    value_data_directive = !ended;
                } else if !ended {
                    skip_data_directive = true;
                }
                continue;
            }
            if trimmed.is_empty() {
                continue;
            }
            if let Some(ticks) = trimmed.strip_prefix('#') {
                let next_tick = ticks.parse::<u64>().map_err(|error| {
                    self.error(line_number, &format!("invalid timestamp: {error}"))
                })?;
                if next_tick < current_tick {
                    return Err(self.error(line_number, "VCD timestamp moved backward"));
                }
                if have_group && next_tick != current_tick {
                    analyzer.apply(self.picoseconds(current_tick, line_number)?, changes)?;
                    changes = [None; 4];
                }
                current_tick = next_tick;
                have_group = true;
                continue;
            }
            let bytes = trimmed.as_bytes();
            if bytes.len() >= 2 && matches!(bytes[0], b'0' | b'1' | b'x' | b'X' | b'z' | b'Z') {
                let identifier = trimmed[1..].trim();
                if let Some(signal) = self.identifiers.get(identifier).copied() {
                    let value = match bytes[0] {
                        b'0' => Some(false),
                        b'1' => Some(true),
                        _ => None,
                    };
                    changes[signal.index()] = Some(value);
                    have_group = true;
                }
                continue;
            }
            if trimmed.starts_with('b') || trimmed.starts_with('B') {
                continue;
            }
            return Err(self.error(line_number, "unsupported VCD value change"));
        }

        if in_header {
            return Err(format!(
                "{}: missing `$enddefinitions`",
                self.source.display()
            ));
        }
        if skip_data_directive || value_data_directive {
            return Err(format!(
                "{}: unterminated VCD data directive",
                self.source.display()
            ));
        }
        if have_group {
            analyzer.apply(self.picoseconds(current_tick, 0)?, changes)?;
        }
        analyzer.finish()
    }

    fn header_directive(&mut self, directive: &str, line: usize) -> Result<(), String> {
        let tokens: Vec<_> = directive.split_ascii_whitespace().collect();
        match tokens.first().copied() {
            Some("$timescale") => {
                if self.timescale_femtoseconds.is_some() {
                    return Err(self.error(line, "duplicate VCD timescale"));
                }
                let content: Vec<_> = tokens[1..]
                    .iter()
                    .copied()
                    .take_while(|token| *token != "$end")
                    .collect();
                let (magnitude, unit) = match content.as_slice() {
                    [combined] => split_timescale(combined)
                        .ok_or_else(|| self.error(line, "invalid VCD timescale"))?,
                    [magnitude, unit] => (*magnitude, *unit),
                    _ => return Err(self.error(line, "invalid VCD timescale")),
                };
                let magnitude = magnitude.parse::<u64>().map_err(|error| {
                    self.error(line, &format!("invalid timescale magnitude: {error}"))
                })?;
                let unit = match unit {
                    "s" => 1_000_000_000_000_000,
                    "ms" => 1_000_000_000_000,
                    "us" => 1_000_000_000,
                    "ns" => 1_000_000,
                    "ps" => 1_000,
                    "fs" => 1,
                    _ => return Err(self.error(line, "unsupported VCD timescale unit")),
                };
                self.timescale_femtoseconds = Some(
                    magnitude
                        .checked_mul(unit)
                        .ok_or_else(|| self.error(line, "VCD timescale overflowed"))?,
                );
            }
            Some("$var") => {
                if tokens.len() < 6 || tokens[2] != "1" {
                    return Ok(());
                }
                let identifier = tokens[3];
                let reference = tokens[4];
                let Some(signal) = Signal::from_name(reference) else {
                    return Ok(());
                };
                if self.assigned[signal.index()] {
                    return Err(self.error(line, "duplicate reviewed signal alias"));
                }
                if self.identifiers.contains_key(identifier) {
                    return Err(self.error(line, "one VCD identifier names multiple signals"));
                }
                self.assigned[signal.index()] = true;
                self.identifiers.insert(identifier.to_owned(), signal);
            }
            Some("$enddefinitions") => {
                if self.timescale_femtoseconds.is_none() {
                    return Err(self.error(line, "missing VCD timescale"));
                }
                if let Some(signal) = Signal::ALL
                    .into_iter()
                    .find(|signal| !self.assigned[signal.index()])
                {
                    return Err(self.error(line, &format!("missing reviewed {signal:?} signal")));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn picoseconds(&self, ticks: u64, line: usize) -> Result<u64, String> {
        let femtoseconds = u128::from(ticks)
            .checked_mul(u128::from(
                self.timescale_femtoseconds
                    .ok_or_else(|| self.error(line, "missing VCD timescale"))?,
            ))
            .ok_or_else(|| self.error(line, "VCD timestamp overflowed"))?;
        if femtoseconds % 1_000 != 0 {
            return Err(self.error(line, "VCD timestamp is not an integer picosecond"));
        }
        u64::try_from(femtoseconds / 1_000)
            .map_err(|_| self.error(line, "VCD picosecond timestamp overflowed"))
    }

    fn error(&self, line: usize, reason: &str) -> String {
        format!("{}:{line}: {reason}", self.source.display())
    }
}

fn split_timescale(value: &str) -> Option<(&str, &str)> {
    let split = value.find(|character: char| !character.is_ascii_digit())?;
    Some(value.split_at(split))
}

#[derive(Default)]
struct EdgeAnalyzer {
    levels: [Option<bool>; 4],
    last_data_change: Option<u64>,
    prelive_bits: BitWindow,
    live_started: bool,
    live_ended: bool,
    report_started: bool,
    live_bits: BitWindow,
    post_bits: BitWindow,
    last_live_bclk: Option<u64>,
    last_live_ws: Option<u64>,
    last_post_ws: Option<u64>,
    static_image: Option<u32>,
    stream_image: Option<u32>,
    live_frames: u64,
    live_tail_bclk_count: u64,
    post_complete_frames: u64,
    post_safe_latches: u64,
    invalid_images: u64,
    non64_frames: u64,
    bclk_period: Range,
    frame_period: Range,
    data_setup: Minimum,
    data_hold: Minimum,
    report_high_started: Option<u64>,
    report_first_sentinel: bool,
    report_final_sentinel: bool,
    report_code_pulses: u64,
}

impl EdgeAnalyzer {
    fn apply(&mut self, at: u64, changes: [Option<Option<bool>>; 4]) -> Result<(), String> {
        let old = self.levels;
        for signal in Signal::ALL {
            if let Some(value) = changes[signal.index()] {
                let value = value
                    .ok_or_else(|| format!("VCD selected {signal:?} became unknown at {at} ps"))?;
                self.levels[signal.index()] = Some(value);
            }
        }
        let new = self.levels;
        let level = |levels: [Option<bool>; 4], signal: Signal| levels[signal.index()];
        let rising = |signal| level(old, signal) == Some(false) && level(new, signal) == Some(true);
        let falling =
            |signal| level(old, signal) == Some(true) && level(new, signal) == Some(false);

        if rising(Signal::Marker) {
            if !self.live_started {
                self.live_started = true;
                self.live_bits.clear();
            } else if self.live_ended {
                if !self.report_started {
                    self.finalize_post_tail()?;
                    self.report_started = true;
                }
                if self.report_final_sentinel {
                    return Err(format!("marker rose after its final sentinel at {at} ps"));
                }
                if self.report_high_started.replace(at).is_some() {
                    return Err(format!("marker rose twice without falling at {at} ps"));
                }
            }
        }
        if falling(Signal::Marker) {
            if self.live_started && !self.live_ended {
                self.live_ended = true;
                self.live_tail_bclk_count = self.live_bits.len();
                if self.live_tail_bclk_count == FRAME_BITS as u64 {
                    self.classify_frame(true)?;
                }
                self.post_bits.clear();
            } else if self.report_started {
                let started = self
                    .report_high_started
                    .take()
                    .ok_or_else(|| format!("marker fell without rising at {at} ps"))?;
                let duration = at
                    .checked_sub(started)
                    .ok_or_else(|| "marker time moved backward".to_owned())?;
                if (LONG_MARKER_MINIMUM_PS..=LONG_MARKER_MAXIMUM_PS).contains(&duration) {
                    if !self.report_first_sentinel {
                        self.report_first_sentinel = true;
                    } else if !self.report_final_sentinel {
                        self.report_final_sentinel = true;
                    } else {
                        return Err("marker contains more than two long sentinels".to_owned());
                    }
                } else if (SHORT_MARKER_MINIMUM_PS..=SHORT_MARKER_MAXIMUM_PS).contains(&duration) {
                    if !self.report_first_sentinel || self.report_final_sentinel {
                        return Err("marker code pulse lies outside its sentinels".to_owned());
                    }
                    self.report_code_pulses = self
                        .report_code_pulses
                        .checked_add(1)
                        .ok_or_else(|| "marker code overflowed".to_owned())?;
                    if self.report_code_pulses > MAXIMUM_MARKER_CODE {
                        return Err("marker code exceeds the fixture grammar".to_owned());
                    }
                } else {
                    return Err("marker high pulse is outside its admitted duration".to_owned());
                }
            }
        }

        let live = self.live_started && !self.live_ended;
        let post = self.live_ended && !self.report_started;
        if rising(Signal::Ws) {
            if live {
                self.live_ws(at)?;
            } else if !self.live_started {
                if let Some(image) = decode_suffix(&self.prelive_bits) {
                    self.static_image = Some(image);
                }
                self.prelive_bits.clear();
            } else if post {
                self.post_ws(at)?;
            }
        }

        let data_changed = matches!(
            (level(old, Signal::Data), level(self.levels, Signal::Data)),
            (Some(before), Some(after)) if before != after
        );
        let bclk_rising = rising(Signal::Bclk);
        if data_changed {
            if live {
                let hold = if bclk_rising {
                    Some(0)
                } else {
                    self.last_live_bclk.and_then(|edge| at.checked_sub(edge))
                };
                if let Some(hold) = hold {
                    self.data_hold.observe(hold);
                }
            }
            self.last_data_change = Some(at);
        }

        if bclk_rising {
            let data = level(self.levels, Signal::Data)
                .ok_or_else(|| format!("DATA had no known level at {at} ps"))?;
            if live {
                if let Some(previous) = self.last_live_bclk {
                    self.bclk_period.observe(
                        at.checked_sub(previous)
                            .ok_or_else(|| "BCLK moved backward".to_owned())?,
                    );
                }
                if let Some(changed) = self.last_data_change {
                    self.data_setup.observe(
                        at.checked_sub(changed)
                            .ok_or_else(|| "DATA setup moved backward".to_owned())?,
                    );
                }
                self.last_live_bclk = Some(at);
                self.live_bits.push(data)?;
            } else if !self.live_started {
                self.prelive_bits.push(data)?;
            } else if post {
                self.post_bits.push(data)?;
            }
        }
        Ok(())
    }

    fn live_ws(&mut self, at: u64) -> Result<(), String> {
        if let Some(previous) = self.last_live_ws {
            self.frame_period.observe(
                at.checked_sub(previous)
                    .ok_or_else(|| "WS moved backward".to_owned())?,
            );
            self.live_frames = self
                .live_frames
                .checked_add(1)
                .ok_or_else(|| "live frame count overflowed".to_owned())?;
            self.classify_frame(true)?;
        }
        self.live_bits.clear();
        self.last_live_ws = Some(at);
        Ok(())
    }

    fn post_ws(&mut self, at: u64) -> Result<(), String> {
        if self.last_post_ws.is_some() {
            if self.post_bits.len() == FRAME_BITS as u64 {
                self.post_complete_frames = self
                    .post_complete_frames
                    .checked_add(1)
                    .ok_or_else(|| "post-stop frame count overflowed".to_owned())?;
                if decode_suffix(&self.post_bits) == Some(SAFE_IMAGE) {
                    self.post_safe_latches = self
                        .post_safe_latches
                        .checked_add(1)
                        .ok_or_else(|| "post-stop latch count overflowed".to_owned())?;
                } else {
                    self.invalid_images = self
                        .invalid_images
                        .checked_add(1)
                        .ok_or_else(|| "invalid image count overflowed".to_owned())?;
                }
            } else {
                self.non64_frames = self
                    .non64_frames
                    .checked_add(1)
                    .ok_or_else(|| "non-64 frame count overflowed".to_owned())?;
            }
        }
        self.post_bits.clear();
        self.last_post_ws = Some(at);
        Ok(())
    }

    fn classify_frame(&mut self, live: bool) -> Result<(), String> {
        let bits = if live {
            &self.live_bits
        } else {
            &self.post_bits
        };
        if bits.len() != FRAME_BITS as u64 {
            self.non64_frames = self
                .non64_frames
                .checked_add(1)
                .ok_or_else(|| "non-64 frame count overflowed".to_owned())?;
            return Ok(());
        }
        let Some(image) = decode_suffix(bits) else {
            self.invalid_images = self
                .invalid_images
                .checked_add(1)
                .ok_or_else(|| "invalid image count overflowed".to_owned())?;
            return Ok(());
        };
        if live && self.stream_image.is_none() {
            self.stream_image = Some(image);
        }
        if image != SAFE_IMAGE {
            self.invalid_images = self
                .invalid_images
                .checked_add(1)
                .ok_or_else(|| "invalid image count overflowed".to_owned())?;
        }
        Ok(())
    }

    fn finalize_post_tail(&mut self) -> Result<(), String> {
        if self.post_bits.is_empty() {
            return Ok(());
        }
        if self.post_bits.len() == FRAME_BITS as u64 {
            self.post_complete_frames = self
                .post_complete_frames
                .checked_add(1)
                .ok_or_else(|| "post-stop frame count overflowed".to_owned())?;
            if decode_suffix(&self.post_bits) != Some(SAFE_IMAGE) {
                self.invalid_images = self
                    .invalid_images
                    .checked_add(1)
                    .ok_or_else(|| "invalid image count overflowed".to_owned())?;
            }
        } else {
            self.non64_frames = self
                .non64_frames
                .checked_add(1)
                .ok_or_else(|| "non-64 frame count overflowed".to_owned())?;
        }
        self.post_bits.clear();
        Ok(())
    }

    fn finish(self) -> Result<TinyBeePcmVcdAnalysis, String> {
        if !self.live_started || !self.live_ended || !self.report_started {
            return Err("capture does not contain complete live-marker phases".to_owned());
        }
        if self.report_high_started.is_some() {
            return Err("capture ended during a marker-high pulse".to_owned());
        }
        if !self.report_first_sentinel || !self.report_final_sentinel {
            return Err("marker report is not self-delimiting".to_owned());
        }
        Ok(TinyBeePcmVcdAnalysis {
            decoded_marker_code: self.report_code_pulses,
            decoded_static_image: u64::from(self.static_image.unwrap_or(0)),
            decoded_stream_image: u64::from(self.stream_image.unwrap_or(0)),
            decoded_live_frames: self.live_frames,
            decoded_live_tail_bclk_count: self.live_tail_bclk_count,
            decoded_post_stop_complete_frames: self.post_complete_frames,
            decoded_post_stop_safe_latches: self.post_safe_latches,
            decoded_invalid_image_count: self.invalid_images,
            decoded_non64_frame_count: self.non64_frames,
            minimum_bclk_period_ps: self.bclk_period.minimum_or_zero(),
            maximum_bclk_period_ps: self.bclk_period.maximum_or_zero(),
            minimum_frame_period_ps: self.frame_period.minimum_or_zero(),
            maximum_frame_period_ps: self.frame_period.maximum_or_zero(),
            minimum_data_setup_ps: self.data_setup.value_or_zero(),
            minimum_data_hold_ps: self.data_hold.value_or_zero(),
        })
    }
}

fn decode_suffix(bits: &BitWindow) -> Option<u32> {
    bits.decode_suffix()
}

#[derive(Default)]
struct BitWindow {
    count: u64,
    suffix: u32,
    suffix_bits: usize,
}

impl BitWindow {
    fn push(&mut self, bit: bool) -> Result<(), String> {
        self.count = self
            .count
            .checked_add(1)
            .ok_or_else(|| "VCD frame bit count overflowed".to_owned())?;
        self.suffix = ((self.suffix << 1) | u32::from(bit)) & IMAGE_MASK;
        self.suffix_bits = (self.suffix_bits + 1).min(IMAGE_WIDTH);
        Ok(())
    }

    const fn len(&self) -> u64 {
        self.count
    }

    const fn is_empty(&self) -> bool {
        self.count == 0
    }

    const fn decode_suffix(&self) -> Option<u32> {
        if self.suffix_bits == IMAGE_WIDTH {
            Some(self.suffix)
        } else {
            None
        }
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

#[derive(Default)]
struct Range {
    minimum: Option<u64>,
    maximum: Option<u64>,
}

impl Range {
    fn observe(&mut self, value: u64) {
        self.minimum = Some(self.minimum.map_or(value, |prior| prior.min(value)));
        self.maximum = Some(self.maximum.map_or(value, |prior| prior.max(value)));
    }

    fn minimum_or_zero(&self) -> u64 {
        self.minimum.unwrap_or(0)
    }

    fn maximum_or_zero(&self) -> u64 {
        self.maximum.unwrap_or(0)
    }
}

#[derive(Default)]
struct Minimum(Option<u64>);

impl Minimum {
    fn observe(&mut self, value: u64) {
        self.0 = Some(self.0.map_or(value, |prior| prior.min(value)));
    }

    fn value_or_zero(&self) -> u64 {
        self.0.unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use core::fmt::Write as _;
    use std::collections::BTreeMap;
    use std::io::Cursor;

    use super::*;

    #[test]
    fn complete_synthetic_capture_reconstructs_every_phase() {
        let source = synthetic_vcd(false);
        let analysis = VcdReader::new(Path::new("fixture.vcd"))
            .read(Cursor::new(source))
            .unwrap();
        assert_eq!(analysis.decoded_marker_code, 1);
        assert_eq!(analysis.decoded_static_image, u64::from(SAFE_IMAGE));
        assert_eq!(analysis.decoded_stream_image, u64::from(SAFE_IMAGE));
        assert_eq!(analysis.decoded_live_frames, 3);
        assert_eq!(analysis.decoded_live_tail_bclk_count, 0);
        assert_eq!(analysis.decoded_post_stop_complete_frames, 2);
        assert_eq!(analysis.decoded_post_stop_safe_latches, 1);
        assert_eq!(analysis.decoded_invalid_image_count, 0);
        assert_eq!(analysis.decoded_non64_frame_count, 0);
        assert_eq!(analysis.minimum_bclk_period_ps, 62_500);
        assert_eq!(analysis.maximum_bclk_period_ps, 62_500);
        assert_eq!(analysis.minimum_frame_period_ps, 4_000_000);
        assert_eq!(analysis.maximum_frame_period_ps, 4_000_000);
        assert_eq!(analysis.minimum_data_setup_ps, 31_250);
        assert_eq!(analysis.minimum_data_hold_ps, 31_250);
    }

    #[test]
    fn malformed_live_frame_is_counted_without_inventing_a_bit() {
        let source = synthetic_vcd(true);
        let analysis = VcdReader::new(Path::new("fixture.vcd"))
            .read(Cursor::new(source))
            .unwrap();
        assert_eq!(analysis.decoded_live_frames, 3);
        assert_eq!(analysis.decoded_non64_frame_count, 1);
    }

    #[test]
    fn report_round_trip_binds_digest_and_exact_measurements() {
        let analysis = VcdReader::new(Path::new("fixture.vcd"))
            .read(Cursor::new(synthetic_vcd(false)))
            .unwrap();
        let root =
            std::env::temp_dir().join(format!("alumina-pcm-vcd-report-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let report = root.join("analysis.toml");
        fs::write(&report, analysis.report(&"a".repeat(64))).unwrap();
        let (digest, replay) = TinyBeePcmVcdAnalysis::read_report(&report).unwrap();
        assert_eq!(digest, "a".repeat(64));
        assert_eq!(replay, analysis);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn command_path_stays_owned_and_refuses_report_overwrite() {
        let root =
            std::env::temp_dir().join(format!("alumina-pcm-vcd-command-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let run = root.join("docs/hil/runs/run-1");
        fs::create_dir_all(&run).unwrap();
        let vcd = Path::new("docs/hil/runs/run-1/capture.vcd");
        let report = Path::new("docs/hil/runs/run-1/analysis.toml");
        fs::write(root.join(vcd), synthetic_vcd(false)).unwrap();

        assert!(
            analyze_to_report(&root, vcd, Path::new("docs/hil/runs/../escaped.toml"))
                .unwrap_err()
                .contains("must be a new file")
        );
        analyze_to_report(&root, vcd, report).unwrap();
        let (digest, analysis) = TinyBeePcmVcdAnalysis::read_report(&root.join(report)).unwrap();
        assert_eq!(digest, sha256_file(&root.join(vcd)).unwrap());
        assert_eq!(analysis.decoded_live_frames, 3);
        assert!(
            analyze_to_report(&root, vcd, report)
                .unwrap_err()
                .contains("refusing overwrite")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unknown_signal_and_fractional_picosecond_reject() {
        let unknown = synthetic_vcd(false).replacen("0!", "x!", 1);
        assert!(
            VcdReader::new(Path::new("fixture.vcd"))
                .read(Cursor::new(unknown))
                .unwrap_err()
                .contains("unknown")
        );
        let fractional = synthetic_vcd(false).replace("$timescale 1 ps", "$timescale 1 fs");
        assert!(
            VcdReader::new(Path::new("fixture.vcd"))
                .read(Cursor::new(fractional))
                .unwrap_err()
                .contains("integer picosecond")
        );
    }

    #[test]
    fn complete_failure_marker_retains_analyzable_zero_measurements() {
        let analysis = EdgeAnalyzer {
            live_started: true,
            live_ended: true,
            report_started: true,
            static_image: Some(SAFE_IMAGE),
            report_first_sentinel: true,
            report_final_sentinel: true,
            report_code_pulses: MAXIMUM_MARKER_CODE,
            ..EdgeAnalyzer::default()
        }
        .finish()
        .unwrap();
        assert_eq!(analysis.decoded_marker_code, MAXIMUM_MARKER_CODE);
        assert_eq!(analysis.decoded_static_image, u64::from(SAFE_IMAGE));
        assert_eq!(analysis.decoded_stream_image, 0);
        assert_eq!(analysis.decoded_live_frames, 0);
        assert_eq!(analysis.minimum_bclk_period_ps, 0);
        assert_eq!(analysis.minimum_data_hold_ps, 0);
    }

    #[test]
    fn complete_live_tail_is_image_checked_before_post_stop_phase() {
        for (corrupt, expected_invalid) in [(false, 0), (true, 1)] {
            let mut analyzer = EdgeAnalyzer {
                levels: [Some(false), Some(false), Some(false), Some(true)],
                live_started: true,
                ..EdgeAnalyzer::default()
            };
            for bit_index in 0..FRAME_BITS {
                let mut bit = u64::from(SAFE_IMAGE) & (1_u64 << (31 - bit_index % 32)) != 0;
                if corrupt && bit_index == FRAME_BITS - 1 {
                    bit = !bit;
                }
                analyzer.live_bits.push(bit).unwrap();
            }
            analyzer
                .apply(1, [None, None, None, Some(Some(false))])
                .unwrap();
            assert_eq!(analyzer.live_tail_bclk_count, FRAME_BITS as u64);
            assert_eq!(analyzer.invalid_images, expected_invalid);
            assert_eq!(
                analyzer.stream_image,
                if corrupt {
                    Some(SAFE_IMAGE ^ 1)
                } else {
                    Some(SAFE_IMAGE)
                }
            );
        }
    }

    fn synthetic_vcd(drop_live_bit: bool) -> String {
        let mut events: BTreeMap<u64, Vec<String>> = BTreeMap::new();
        let mut levels = [false; 4];

        let mut at = 10_000_u64;
        let static_bits = image_bits(SAFE_IMAGE);
        for bit in static_bits {
            set(&mut events, &mut levels, at, 2, bit, "#");
            set(&mut events, &mut levels, at + 10_000, 0, true, "!");
            set(&mut events, &mut levels, at + 20_000, 0, false, "!");
            at += 40_000;
        }
        set(&mut events, &mut levels, at, 1, true, "\"");
        set(&mut events, &mut levels, at + 10_000, 1, false, "\"");

        let live_start = 50_000_000_000_u64;
        set(&mut events, &mut levels, live_start, 3, true, "$");
        let mut frame_start = live_start + 1_000_000;
        set(&mut events, &mut levels, frame_start, 1, true, "\"");
        set(
            &mut events,
            &mut levels,
            frame_start + 62_500,
            1,
            false,
            "\"",
        );
        for frame in 0..3 {
            emit_frame(
                &mut events,
                &mut levels,
                frame_start,
                drop_live_bit && frame == 1,
            );
            frame_start += 4_000_000;
            set(&mut events, &mut levels, frame_start, 1, true, "\"");
            set(
                &mut events,
                &mut levels,
                frame_start + 62_500,
                1,
                false,
                "\"",
            );
        }
        let live_stop = frame_start + 500_000;
        set(&mut events, &mut levels, live_stop, 3, false, "$");

        let post_start = live_stop + 500_000;
        set(&mut events, &mut levels, post_start, 1, true, "\"");
        set(
            &mut events,
            &mut levels,
            post_start + 62_500,
            1,
            false,
            "\"",
        );
        emit_frame(&mut events, &mut levels, post_start, false);
        let second = post_start + 4_000_000;
        set(&mut events, &mut levels, second, 1, true, "\"");
        set(&mut events, &mut levels, second + 62_500, 1, false, "\"");
        emit_frame(&mut events, &mut levels, second, false);

        let sentinel = live_stop + 1_000_000_000;
        set(&mut events, &mut levels, sentinel, 3, true, "$");
        set(
            &mut events,
            &mut levels,
            sentinel + 1_000_000_000,
            3,
            false,
            "$",
        );
        let pulse = sentinel + 2_000_000_000;
        set(&mut events, &mut levels, pulse, 3, true, "$");
        set(&mut events, &mut levels, pulse + 100_000_000, 3, false, "$");
        let final_sentinel = pulse + 200_000_000;
        set(&mut events, &mut levels, final_sentinel, 3, true, "$");
        set(
            &mut events,
            &mut levels,
            final_sentinel + 1_000_000_000,
            3,
            false,
            "$",
        );

        let mut source = String::from(
            "$timescale 1 ps $end\n\
             $scope module logic $end\n\
             $var wire 1 ! BCLK $end\n\
             $var wire 1 \" WS $end\n\
             $var wire 1 # DATA $end\n\
             $var wire 1 $ MARKER $end\n\
             $upscope $end\n\
             $enddefinitions $end\n\
             $dumpvars\n\
             0!\n\
             0\"\n\
             0#\n\
             0$\n\
             $end\n",
        );
        for (timestamp, changes) in events {
            writeln!(&mut source, "#{timestamp}").unwrap();
            for change in changes {
                writeln!(&mut source, "{change}").unwrap();
            }
        }
        source
    }

    fn emit_frame(
        events: &mut BTreeMap<u64, Vec<String>>,
        levels: &mut [bool; 4],
        start: u64,
        drop_bit: bool,
    ) {
        let sample = u64::from(SAFE_IMAGE);
        for bit_index in 0..FRAME_BITS {
            if drop_bit && bit_index == 17 {
                continue;
            }
            let bit = sample & (1_u64 << (31 - bit_index % 32)) != 0;
            let rising = start + 31_250 + u64::try_from(bit_index).unwrap() * 62_500;
            set(events, levels, rising - 31_250, 2, bit, "#");
            set(events, levels, rising, 0, true, "!");
            set(events, levels, rising + 31_250, 0, false, "!");
        }
    }

    fn image_bits(image: u32) -> Vec<bool> {
        (0..IMAGE_WIDTH)
            .map(|index| image & (1 << (IMAGE_WIDTH - 1 - index)) != 0)
            .collect()
    }

    fn set(
        events: &mut BTreeMap<u64, Vec<String>>,
        levels: &mut [bool; 4],
        at: u64,
        signal: usize,
        value: bool,
        identifier: &str,
    ) {
        if levels[signal] == value && at != 0 {
            return;
        }
        levels[signal] = value;
        events
            .entry(at)
            .or_default()
            .push(format!("{}{identifier}", u8::from(value)));
    }
}
