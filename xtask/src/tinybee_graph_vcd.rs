use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write as _};
use std::path::{Component, Path, PathBuf};

use crate::hil_record::{
    EVIDENCE_PREFIX, Record, parse_fields, repository_path, require_hex, sha256_file,
};

const REPORT_SCHEMA: u64 = 1;
const REPORT_FIELDS: &[&str] = &[
    "schema",
    "source_vcd_sha256",
    "capture_duration_ps",
    "release_pulse_count",
    "pre_assertion_release_count",
    "post_sink_release_count",
    "raw_input_transition_count",
    "sink_transition_count",
    "first_input_assertion_ps",
    "qualifying_input_assertion_ps",
    "sink_assertion_ps",
    "input_to_sink_ps",
    "sink_after_release_ps",
    "minimum_release_pulse_ps",
    "maximum_release_pulse_ps",
    "minimum_release_period_ps",
    "maximum_release_period_ps",
];

/// Measurements reconstructed directly from the graph fixture's three-channel
/// VCD edge stream. Times are integer picoseconds from the VCD epoch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TinyBeeGraphVcdAnalysis {
    pub capture_duration_ps: u64,
    pub release_pulse_count: u64,
    pub pre_assertion_release_count: u64,
    pub post_sink_release_count: u64,
    pub raw_input_transition_count: u64,
    pub sink_transition_count: u64,
    pub first_input_assertion_ps: u64,
    pub qualifying_input_assertion_ps: u64,
    pub sink_assertion_ps: u64,
    pub input_to_sink_ps: u64,
    pub sink_after_release_ps: u64,
    pub minimum_release_pulse_ps: u64,
    pub maximum_release_pulse_ps: u64,
    pub minimum_release_period_ps: u64,
    pub maximum_release_period_ps: u64,
}

impl TinyBeeGraphVcdAnalysis {
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
            capture_duration_ps: record.u64("capture_duration_ps")?,
            release_pulse_count: record.u64("release_pulse_count")?,
            pre_assertion_release_count: record.u64("pre_assertion_release_count")?,
            post_sink_release_count: record.u64("post_sink_release_count")?,
            raw_input_transition_count: record.u64("raw_input_transition_count")?,
            sink_transition_count: record.u64("sink_transition_count")?,
            first_input_assertion_ps: record.u64("first_input_assertion_ps")?,
            qualifying_input_assertion_ps: record.u64("qualifying_input_assertion_ps")?,
            sink_assertion_ps: record.u64("sink_assertion_ps")?,
            input_to_sink_ps: record.u64("input_to_sink_ps")?,
            sink_after_release_ps: record.u64("sink_after_release_ps")?,
            minimum_release_pulse_ps: record.u64("minimum_release_pulse_ps")?,
            maximum_release_pulse_ps: record.u64("maximum_release_pulse_ps")?,
            minimum_release_period_ps: record.u64("minimum_release_period_ps")?,
            maximum_release_period_ps: record.u64("maximum_release_period_ps")?,
        };
        analysis.validate()?;
        Ok((source_vcd_sha256, analysis))
    }

    fn validate(self) -> Result<(), String> {
        if self.capture_duration_ps == 0
            || self.release_pulse_count == 0
            || self.pre_assertion_release_count > self.release_pulse_count
            || self.post_sink_release_count > self.release_pulse_count
            || self.raw_input_transition_count == 0
            || self.sink_transition_count != 1
            || self.first_input_assertion_ps > self.qualifying_input_assertion_ps
            || self.qualifying_input_assertion_ps >= self.sink_assertion_ps
            || self.input_to_sink_ps
                != self
                    .sink_assertion_ps
                    .saturating_sub(self.qualifying_input_assertion_ps)
            || self.sink_after_release_ps > self.input_to_sink_ps
        {
            return Err("graph-input analysis chronology or counts are inconsistent".to_owned());
        }
        for (name, minimum, maximum) in [
            (
                "release pulse",
                self.minimum_release_pulse_ps,
                self.maximum_release_pulse_ps,
            ),
            (
                "release period",
                self.minimum_release_period_ps,
                self.maximum_release_period_ps,
            ),
        ] {
            if minimum == 0 || minimum > maximum {
                return Err(format!("graph-input analysis {name} range is inconsistent"));
            }
        }
        Ok(())
    }

    pub(crate) fn report(self, source_vcd_sha256: &str) -> String {
        format!(
            concat!(
                "schema = 1\n",
                "source_vcd_sha256 = \"{}\"\n",
                "capture_duration_ps = {}\n",
                "release_pulse_count = {}\n",
                "pre_assertion_release_count = {}\n",
                "post_sink_release_count = {}\n",
                "raw_input_transition_count = {}\n",
                "sink_transition_count = {}\n",
                "first_input_assertion_ps = {}\n",
                "qualifying_input_assertion_ps = {}\n",
                "sink_assertion_ps = {}\n",
                "input_to_sink_ps = {}\n",
                "sink_after_release_ps = {}\n",
                "minimum_release_pulse_ps = {}\n",
                "maximum_release_pulse_ps = {}\n",
                "minimum_release_period_ps = {}\n",
                "maximum_release_period_ps = {}\n"
            ),
            source_vcd_sha256,
            self.capture_duration_ps,
            self.release_pulse_count,
            self.pre_assertion_release_count,
            self.post_sink_release_count,
            self.raw_input_transition_count,
            self.sink_transition_count,
            self.first_input_assertion_ps,
            self.qualifying_input_assertion_ps,
            self.sink_assertion_ps,
            self.input_to_sink_ps,
            self.sink_after_release_ps,
            self.minimum_release_pulse_ps,
            self.maximum_release_pulse_ps,
            self.minimum_release_period_ps,
            self.maximum_release_period_ps,
        )
    }
}

/// Analyzes one retained VCD below `docs/hil/runs` without loading it in memory.
pub fn analyze(root: &Path, vcd_path: &Path) -> Result<(String, TinyBeeGraphVcdAnalysis), String> {
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
        "analyzed {} releases; input-to-sink={} ps; maximum release pulse={} ps; report={}",
        analysis.release_pulse_count,
        analysis.input_to_sink_ps,
        analysis.maximum_release_pulse_ps,
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
    Timing = 0,
    Input = 1,
    Sink = 2,
}

impl Signal {
    const ALL: [Self; 3] = [Self::Timing, Self::Input, Self::Sink];

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
            "D0" | "TIMING" | "GRAPH" | "GRAPHRELEASE" | "LCDRS" | "LCDRSO" | "GPIO4" => {
                Some(Self::Timing)
            }
            "D1" | "INPUT" | "XENDSTOP" | "XMIN" | "X" | "GPIO33" => Some(Self::Input),
            "D2" | "SINK" | "GRAPHSINK" | "LCDEN" | "LCDENO" | "GPIO21" => Some(Self::Sink),
            _ => None,
        }
    }
}

struct VcdReader<'a> {
    source: &'a Path,
    timescale_femtoseconds: Option<u64>,
    identifiers: BTreeMap<String, Signal>,
    assigned: [bool; 3],
}

impl<'a> VcdReader<'a> {
    const fn new(source: &'a Path) -> Self {
        Self {
            source,
            timescale_femtoseconds: None,
            identifiers: BTreeMap::new(),
            assigned: [false; 3],
        }
    }

    fn read<R: BufRead>(mut self, reader: R) -> Result<TinyBeeGraphVcdAnalysis, String> {
        let mut analyzer = EdgeAnalyzer::default();
        let mut in_header = true;
        let mut directive = String::new();
        let mut current_tick = 0_u64;
        let mut changes = [None; 3];
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
                    changes = [None; 3];
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
    levels: [Option<bool>; 3],
    initial_levels: Option<[bool; 3]>,
    first_time: Option<u64>,
    last_time: Option<u64>,
    timing_high_since: Option<u64>,
    last_release_rise: Option<u64>,
    last_release_fall: Option<u64>,
    release_pulses: u64,
    release_pulse_width: Range,
    release_period: Range,
    raw_input_transitions: u64,
    first_input_assertion: Option<u64>,
    last_input_assertion: Option<u64>,
    sink_transitions: u64,
    sink_assertion: Option<u64>,
    sink_after_release: Option<u64>,
    pre_assertion_releases: u64,
    post_sink_releases: u64,
}

impl EdgeAnalyzer {
    fn apply(&mut self, at: u64, changes: [Option<Option<bool>>; 3]) -> Result<(), String> {
        if self.last_time.is_some_and(|last| at < last) {
            return Err("VCD event time moved backward".to_owned());
        }
        self.first_time.get_or_insert(at);
        self.last_time = Some(at);
        let old = self.levels;
        for signal in Signal::ALL {
            if let Some(value) = changes[signal.index()] {
                let value = value
                    .ok_or_else(|| format!("VCD selected {signal:?} became unknown at {at} ps"))?;
                self.levels[signal.index()] = Some(value);
            }
        }
        if self.initial_levels.is_none()
            && let [Some(timing), Some(input), Some(sink)] = self.levels
        {
            self.initial_levels = Some([timing, input, sink]);
        }
        let new = self.levels;
        let level = |levels: [Option<bool>; 3], signal: Signal| levels[signal.index()];
        let rising = |signal| level(old, signal) == Some(false) && level(new, signal) == Some(true);
        let falling =
            |signal| level(old, signal) == Some(true) && level(new, signal) == Some(false);

        if rising(Signal::Timing) {
            if self.timing_high_since.replace(at).is_some() {
                return Err(format!("timing marker rose twice at {at} ps"));
            }
            if let Some(previous) = self.last_release_rise {
                let period = at
                    .checked_sub(previous)
                    .ok_or_else(|| "release period moved backward".to_owned())?;
                self.release_period.observe(period);
            }
            self.last_release_rise = Some(at);
        }
        if falling(Signal::Timing) {
            if let Some(started) = self.timing_high_since.take() {
                let width = at
                    .checked_sub(started)
                    .ok_or_else(|| "release pulse moved backward".to_owned())?;
                if width == 0 {
                    return Err("release marker contained a zero-width pulse".to_owned());
                }
                self.release_pulse_width.observe(width);
                self.release_pulses = self
                    .release_pulses
                    .checked_add(1)
                    .ok_or_else(|| "release count overflowed".to_owned())?;
                if self
                    .first_input_assertion
                    .is_none_or(|asserted| started < asserted)
                {
                    self.pre_assertion_releases = self
                        .pre_assertion_releases
                        .checked_add(1)
                        .ok_or_else(|| "pre-assertion release count overflowed".to_owned())?;
                }
                if self
                    .sink_assertion
                    .is_some_and(|asserted| started > asserted)
                {
                    self.post_sink_releases = self
                        .post_sink_releases
                        .checked_add(1)
                        .ok_or_else(|| "post-sink release count overflowed".to_owned())?;
                }
                self.last_release_fall = Some(at);
            }
        }

        if rising(Signal::Input) || falling(Signal::Input) {
            self.raw_input_transitions = self
                .raw_input_transitions
                .checked_add(1)
                .ok_or_else(|| "raw input transition count overflowed".to_owned())?;
        }
        if falling(Signal::Input) {
            self.first_input_assertion.get_or_insert(at);
            self.last_input_assertion = Some(at);
        }

        if rising(Signal::Sink) {
            if self.sink_assertion.is_some() {
                return Err("graph sink asserted more than once".to_owned());
            }
            if level(self.levels, Signal::Input) != Some(false) {
                return Err("graph sink asserted while the raw input was not active-low".to_owned());
            }
            let input = self
                .last_input_assertion
                .ok_or_else(|| "graph sink asserted before the raw input".to_owned())?;
            let release = self
                .last_release_fall
                .ok_or_else(|| "graph sink asserted before any complete release".to_owned())?;
            if self.timing_high_since.is_some() || release > at {
                return Err("graph sink did not change after a completed release".to_owned());
            }
            self.sink_assertion = Some(at);
            self.sink_after_release = Some(
                at.checked_sub(release)
                    .ok_or_else(|| "sink/release time moved backward".to_owned())?,
            );
            self.sink_transitions = self
                .sink_transitions
                .checked_add(1)
                .ok_or_else(|| "sink transition count overflowed".to_owned())?;
            let _ = input;
        }
        if falling(Signal::Sink) {
            return Err("graph sink deasserted in an assertion capture".to_owned());
        }
        Ok(())
    }

    fn finish(self) -> Result<TinyBeeGraphVcdAnalysis, String> {
        let initial = self
            .initial_levels
            .ok_or_else(|| "VCD never established all reviewed signals".to_owned())?;
        if !initial[Signal::Input.index()] || initial[Signal::Sink.index()] {
            return Err("capture must begin with X-endstop high and graph sink low".to_owned());
        }
        if self.levels[Signal::Input.index()] != Some(false)
            || self.levels[Signal::Sink.index()] != Some(true)
        {
            return Err("capture must end with X-endstop low and graph sink high".to_owned());
        }
        let first_time = self
            .first_time
            .ok_or_else(|| "VCD contained no time groups".to_owned())?;
        let last_time = self
            .last_time
            .ok_or_else(|| "VCD contained no final time".to_owned())?;
        let first_input_assertion = self
            .first_input_assertion
            .ok_or_else(|| "capture contains no X-endstop assertion".to_owned())?;
        let qualifying_input_assertion = self
            .last_input_assertion
            .ok_or_else(|| "capture contains no qualifying X-endstop assertion".to_owned())?;
        let sink_assertion = self
            .sink_assertion
            .ok_or_else(|| "capture contains no graph-sink assertion".to_owned())?;
        let analysis = TinyBeeGraphVcdAnalysis {
            capture_duration_ps: last_time
                .checked_sub(first_time)
                .ok_or_else(|| "capture duration moved backward".to_owned())?,
            release_pulse_count: self.release_pulses,
            pre_assertion_release_count: self.pre_assertion_releases,
            post_sink_release_count: self.post_sink_releases,
            raw_input_transition_count: self.raw_input_transitions,
            sink_transition_count: self.sink_transitions,
            first_input_assertion_ps: first_input_assertion,
            qualifying_input_assertion_ps: qualifying_input_assertion,
            sink_assertion_ps: sink_assertion,
            input_to_sink_ps: sink_assertion
                .checked_sub(qualifying_input_assertion)
                .ok_or_else(|| "input/sink time moved backward".to_owned())?,
            sink_after_release_ps: self
                .sink_after_release
                .ok_or_else(|| "sink was not correlated with a release".to_owned())?,
            minimum_release_pulse_ps: self.release_pulse_width.minimum(),
            maximum_release_pulse_ps: self.release_pulse_width.maximum,
            minimum_release_period_ps: self.release_period.minimum(),
            maximum_release_period_ps: self.release_period.maximum,
        };
        analysis.validate()?;
        Ok(analysis)
    }
}

#[derive(Default)]
struct Range {
    minimum: u64,
    maximum: u64,
}

impl Range {
    fn observe(&mut self, value: u64) {
        if self.minimum == 0 || value < self.minimum {
            self.minimum = value;
        }
        self.maximum = self.maximum.max(value);
    }

    const fn minimum(&self) -> u64 {
        self.minimum
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn assertion_vcd(sink_before_input: bool) -> String {
        let mut vcd = String::from(
            "$timescale 1 ns $end\n\
             $scope module logic $end\n\
             $var wire 1 ! D0 $end\n\
             $var wire 1 \" D1 $end\n\
             $var wire 1 # D2 $end\n\
             $upscope $end\n\
             $enddefinitions $end\n\
             #0\n0!\n1\"\n0#\n",
        );
        for release in 1_u64..=40 {
            let rise = release * 1_000_000;
            let fall = rise + 1_000;
            vcd.push_str(&format!("#{rise}\n1!\n#{fall}\n0!\n"));
            if sink_before_input && release == 18 {
                vcd.push_str(&format!("#{}\n1#\n", fall + 100));
            }
            if release == 20 {
                vcd.push_str(&format!("#{}\n0\"\n", fall + 100));
            }
            if !sink_before_input && release == 23 {
                vcd.push_str(&format!("#{}\n1#\n", fall + 100));
            }
        }
        vcd.push_str("#41000000\n");
        vcd
    }

    #[test]
    fn reconstructs_release_timing_and_end_to_end_assertion() {
        let source = assertion_vcd(false);
        let analysis = VcdReader::new(Path::new("fixture.vcd"))
            .read(Cursor::new(source))
            .unwrap();
        assert_eq!(analysis.release_pulse_count, 40);
        assert_eq!(analysis.minimum_release_pulse_ps, 1_000_000);
        assert_eq!(analysis.maximum_release_period_ps, 1_000_000_000);
        assert_eq!(analysis.input_to_sink_ps, 3_000_000_000);
        assert_eq!(analysis.sink_after_release_ps, 100_000);
        assert_eq!(analysis.sink_transition_count, 1);
        assert!(analysis.pre_assertion_release_count >= 19);
        assert!(analysis.post_sink_release_count >= 16);
    }

    #[test]
    fn rejects_a_sink_assertion_without_input_authority() {
        let error = VcdReader::new(Path::new("fixture.vcd"))
            .read(Cursor::new(assertion_vcd(true)))
            .unwrap_err();
        assert!(error.contains("raw input"));
    }
}
