use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest as _, Sha256};

const SCHEMA: u64 = 1;
const FIXTURE_ID: &str = "m7-two-board-start";
const TINYBEE_ID: &str = "mks-tinybee-v1";
const T_DECK_ID: &str = "t-deck-pro";
pub(crate) const EVIDENCE_PREFIX: &str = "docs/hil/runs";
pub(crate) const ARTIFACT_PREFIX: &str = "target";

const FIELDS: &[&str] = &[
    "schema",
    "fixture_id",
    "run_id",
    "started_utc",
    "operator",
    "reviewer",
    "disposition",
    "wifi_condition",
    "alumina-firmware_commit",
    "alumina_interface_commit",
    "hazardous_loads_disconnected",
    "tinybee_board_id",
    "tinybee_revision",
    "tinybee_serial",
    "tinybee_boot_id",
    "tinybee_output_route",
    "tinybee_artifact_path",
    "tinybee_artifact_sha256",
    "tinybee_scheduled_cycle",
    "tinybee_observed_earliest_cycle",
    "tinybee_observed_latest_cycle",
    "tinybee_photo_path",
    "tinybee_photo_sha256",
    "tinybee_photo_license",
    "tinybee_photo_attribution",
    "t_deck_board_id",
    "t_deck_revision",
    "t_deck_serial",
    "t_deck_boot_id",
    "t_deck_output_route",
    "t_deck_output_reviewed",
    "t_deck_artifact_path",
    "t_deck_artifact_sha256",
    "t_deck_scheduled_cycle",
    "t_deck_observed_earliest_cycle",
    "t_deck_observed_latest_cycle",
    "t_deck_photo_path",
    "t_deck_photo_sha256",
    "t_deck_photo_license",
    "t_deck_photo_attribution",
    "target_ui_ns",
    "predicted_max_edge_spread_ns",
    "reconciled_max_edge_spread_ns",
    "captured_edge_spread_ns",
    "analyzer_model",
    "analyzer_firmware",
    "analyzer_sample_rate_hz",
    "analyzer_threshold_mv",
    "clock_log_path",
    "clock_log_sha256",
    "capture_path",
    "capture_sha256",
    "review_notes_path",
    "review_notes_sha256",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HilRunSummary {
    pub run_id: String,
    pub disposition: String,
    pub wifi_condition: String,
    pub captured_edge_spread_ns: u64,
    pub predicted_max_edge_spread_ns: u64,
}

#[derive(Debug)]
pub(crate) struct ParsedField {
    value: String,
    line: usize,
}

pub(crate) struct Record<'a> {
    fields: &'a BTreeMap<String, ParsedField>,
    source: &'a Path,
}

impl Record<'_> {
    pub(crate) const fn new<'a>(
        fields: &'a BTreeMap<String, ParsedField>,
        source: &'a Path,
    ) -> Record<'a> {
        Record { fields, source }
    }

    pub(crate) fn string(&self, key: &str) -> Result<String, String> {
        let field = self.field(key)?;
        super::parse_string(&field.value, self.source, field.line)
    }

    pub(crate) fn nonempty(&self, key: &str) -> Result<String, String> {
        let value = self.string(key)?;
        if value.trim().is_empty() {
            Err(self.error(key, "cannot be empty"))
        } else {
            Ok(value)
        }
    }

    pub(crate) fn u64(&self, key: &str) -> Result<u64, String> {
        let field = self.field(key)?;
        if field.value.is_empty()
            || !field.value.bytes().all(|byte| byte.is_ascii_digit())
            || field.value.len() > 1 && field.value.starts_with('0')
        {
            return Err(self.error(key, "must be a canonical unsigned decimal integer"));
        }
        field.value.parse().map_err(|error| {
            format!(
                "{}:{}: invalid `{key}`: {error}",
                self.source.display(),
                field.line
            )
        })
    }

    pub(crate) fn boolean(&self, key: &str) -> Result<bool, String> {
        let field = self.field(key)?;
        field.value.parse().map_err(|error| {
            format!(
                "{}:{}: invalid `{key}`: {error}",
                self.source.display(),
                field.line
            )
        })
    }

    fn field(&self, key: &str) -> Result<&ParsedField, String> {
        self.fields
            .get(key)
            .ok_or_else(|| format!("{}: missing `{key}`", self.source.display()))
    }

    pub(crate) fn error(&self, key: &str, reason: &str) -> String {
        let line = self.fields.get(key).map_or(0, |field| field.line);
        format!("{}:{line}: `{key}` {reason}", self.source.display())
    }
}

pub fn validate(root: &Path, record_path: &Path) -> Result<HilRunSummary, String> {
    let record_path = repository_path(root, record_path, EVIDENCE_PREFIX, "run record")?;
    let source = fs::read_to_string(&record_path)
        .map_err(|error| format!("cannot read {}: {error}", record_path.display()))?;
    let fields = parse_fields(&source, &record_path, FIELDS)?;
    let record = Record::new(&fields, &record_path);

    if record.u64("schema")? != SCHEMA {
        return Err(record.error("schema", "is not the exact supported schema"));
    }
    if record.string("fixture_id")? != FIXTURE_ID {
        return Err(record.error("fixture_id", "is not the two-board start fixture"));
    }
    let run_id = record.nonempty("run_id")?;
    if !run_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(record.error("run_id", "contains an unsafe character"));
    }
    let started_utc = record.nonempty("started_utc")?;
    if !valid_utc_timestamp(&started_utc) {
        return Err(record.error("started_utc", "is not a canonical UTC timestamp"));
    }
    record.nonempty("operator")?;
    record.nonempty("reviewer")?;

    let disposition = record.string("disposition")?;
    if !matches!(disposition.as_str(), "pass" | "fail" | "inconclusive") {
        return Err(record.error("disposition", "must be pass, fail, or inconclusive"));
    }
    let wifi_condition = record.string("wifi_condition")?;
    if !matches!(wifi_condition.as_str(), "nominal" | "saturated") {
        return Err(record.error("wifi_condition", "must be nominal or saturated"));
    }
    require_hex(&record, "alumina-firmware_commit", 40)?;
    require_hex(&record, "alumina_interface_commit", 40)?;
    if !record.boolean("hazardous_loads_disconnected")? {
        return Err(record.error(
            "hazardous_loads_disconnected",
            "must be true for this fixture",
        ));
    }

    validate_participant(
        root,
        &record,
        ParticipantFields {
            prefix: "tinybee",
            expected_board_id: TINYBEE_ID,
            review_output: false,
        },
    )?;
    validate_participant(
        root,
        &record,
        ParticipantFields {
            prefix: "t_deck",
            expected_board_id: T_DECK_ID,
            review_output: true,
        },
    )?;

    if record.u64("target_ui_ns")? == 0 {
        return Err(record.error("target_ui_ns", "must be nonzero"));
    }
    let predicted_max_edge_spread_ns = record.u64("predicted_max_edge_spread_ns")?;
    if predicted_max_edge_spread_ns == 0 {
        return Err(record.error("predicted_max_edge_spread_ns", "must be nonzero"));
    }
    let reconciled = record.u64("reconciled_max_edge_spread_ns")?;
    let captured_edge_spread_ns = record.u64("captured_edge_spread_ns")?;
    if disposition == "pass" && captured_edge_spread_ns > predicted_max_edge_spread_ns {
        return Err(record.error(
            "captured_edge_spread_ns",
            "exceeds the pre-commit admitted edge-spread bound",
        ));
    }
    if disposition == "pass" && reconciled < captured_edge_spread_ns {
        return Err(record.error(
            "reconciled_max_edge_spread_ns",
            "cannot conservatively exclude the captured physical spread",
        ));
    }

    record.nonempty("analyzer_model")?;
    record.nonempty("analyzer_firmware")?;
    if record.u64("analyzer_sample_rate_hz")? == 0 {
        return Err(record.error("analyzer_sample_rate_hz", "must be nonzero"));
    }
    if record.u64("analyzer_threshold_mv")? == 0 {
        return Err(record.error("analyzer_threshold_mv", "must be nonzero"));
    }
    for (path_key, digest_key) in [
        ("clock_log_path", "clock_log_sha256"),
        ("capture_path", "capture_sha256"),
        ("review_notes_path", "review_notes_sha256"),
    ] {
        verify_evidence_asset(root, &record, path_key, digest_key)?;
    }

    Ok(HilRunSummary {
        run_id,
        disposition,
        wifi_condition,
        captured_edge_spread_ns,
        predicted_max_edge_spread_ns,
    })
}

#[derive(Clone, Copy)]
struct ParticipantFields {
    prefix: &'static str,
    expected_board_id: &'static str,
    review_output: bool,
}

fn validate_participant(
    root: &Path,
    record: &Record<'_>,
    participant: ParticipantFields,
) -> Result<(), String> {
    let key = |suffix: &str| format!("{}_{}", participant.prefix, suffix);
    let board_key = key("board_id");
    if record.string(&board_key)? != participant.expected_board_id {
        return Err(record.error(&board_key, "does not match the fixture board"));
    }
    record.nonempty(&key("revision"))?;
    record.nonempty(&key("serial"))?;
    require_hex(record, &key("boot_id"), 32)?;
    record.nonempty(&key("output_route"))?;
    if participant.review_output && !record.boolean(&key("output_reviewed"))? {
        return Err(record.error(&key("output_reviewed"), "must be true"));
    }

    let artifact_path_key = key("artifact_path");
    let artifact_digest_key = key("artifact_sha256");
    let artifact = record.nonempty(&artifact_path_key)?;
    let artifact = repository_path(root, Path::new(&artifact), ARTIFACT_PREFIX, "artifact")?;
    verify_digest(record, &artifact_digest_key, &artifact)?;

    let scheduled_key = key("scheduled_cycle");
    let earliest_key = key("observed_earliest_cycle");
    let latest_key = key("observed_latest_cycle");
    let scheduled = record.u64(&scheduled_key)?;
    let earliest = record.u64(&earliest_key)?;
    let latest = record.u64(&latest_key)?;
    if scheduled == 0 {
        return Err(record.error(&scheduled_key, "must be nonzero"));
    }
    if earliest < scheduled {
        return Err(record.error(&earliest_key, "precedes the scheduled cycle"));
    }
    if latest < earliest {
        return Err(record.error(&latest_key, "precedes the earliest observation"));
    }

    let photo_path_key = key("photo_path");
    let photo_digest_key = key("photo_sha256");
    verify_evidence_asset(root, record, &photo_path_key, &photo_digest_key)?;
    let license_key = key("photo_license");
    if !matches!(
        record.string(&license_key)?.as_str(),
        "CC0-1.0" | "CC-BY-4.0"
    ) {
        return Err(record.error(&license_key, "must be CC0-1.0 or CC-BY-4.0"));
    }
    record.nonempty(&key("photo_attribution"))?;
    Ok(())
}

pub(crate) fn parse_fields(
    source: &str,
    path: &Path,
    allowed_fields: &[&str],
) -> Result<BTreeMap<String, ParsedField>, String> {
    let mut fields = BTreeMap::new();
    for (line_index, raw_line) in source.lines().enumerate() {
        let line_number = line_index + 1;
        let line = super::without_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("{}:{line_number}: expected `key = value`", path.display()))?;
        let key = key.trim();
        let value = value.trim();
        if !allowed_fields.contains(&key) {
            return Err(format!(
                "{}:{line_number}: unknown HIL field `{key}`",
                path.display()
            ));
        }
        if value.is_empty() {
            return Err(format!(
                "{}:{line_number}: HIL field `{key}` has no value",
                path.display()
            ));
        }
        if fields
            .insert(
                key.to_owned(),
                ParsedField {
                    value: value.to_owned(),
                    line: line_number,
                },
            )
            .is_some()
        {
            return Err(format!(
                "{}:{line_number}: duplicate HIL field `{key}`",
                path.display()
            ));
        }
    }
    if fields.len() != allowed_fields.len() {
        let missing = allowed_fields
            .iter()
            .find(|field| !fields.contains_key(**field))
            .copied()
            .unwrap_or("unknown");
        return Err(format!("{}: missing `{missing}`", path.display()));
    }
    Ok(fields)
}

pub(crate) fn repository_path(
    root: &Path,
    value: &Path,
    required_prefix: &str,
    kind: &str,
) -> Result<PathBuf, String> {
    let relative = if value.is_absolute() {
        value
            .strip_prefix(root)
            .map_err(|_| format!("{kind} path {} is outside the repository", value.display()))?
    } else {
        value
    };
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        || !relative.starts_with(required_prefix)
    {
        return Err(format!(
            "{kind} path {} must remain below `{required_prefix}`",
            value.display()
        ));
    }
    let candidate = root.join(relative);
    let canonical_root = fs::canonicalize(root)
        .map_err(|error| format!("cannot resolve repository root {}: {error}", root.display()))?;
    let canonical_prefix = fs::canonicalize(root.join(required_prefix)).map_err(|error| {
        format!(
            "cannot resolve {kind} directory {}: {error}",
            root.join(required_prefix).display()
        )
    })?;
    let canonical_candidate = fs::canonicalize(&candidate)
        .map_err(|error| format!("cannot resolve {kind} {}: {error}", candidate.display()))?;
    if !canonical_prefix.starts_with(&canonical_root)
        || !canonical_candidate.starts_with(&canonical_prefix)
    {
        return Err(format!(
            "{kind} path {} resolves outside `{required_prefix}`",
            value.display()
        ));
    }
    Ok(canonical_candidate)
}

pub(crate) fn verify_evidence_asset(
    root: &Path,
    record: &Record<'_>,
    path_key: &str,
    digest_key: &str,
) -> Result<(), String> {
    let value = record.nonempty(path_key)?;
    let path = repository_path(root, Path::new(&value), EVIDENCE_PREFIX, "evidence")?;
    verify_digest(record, digest_key, &path)
}

pub(crate) fn verify_digest(
    record: &Record<'_>,
    digest_key: &str,
    path: &Path,
) -> Result<(), String> {
    let expected = require_hex(record, digest_key, 64)?;
    let actual = sha256_file(path)?;
    if actual != expected {
        return Err(record.error(digest_key, &format!("does not match {}", path.display())));
    }
    Ok(())
}

pub(crate) fn require_hex(record: &Record<'_>, key: &str, digits: usize) -> Result<String, String> {
    let value = record.string(key)?;
    if value.len() != digits
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(record.error(
            key,
            &format!("must be {digits} lowercase hexadecimal digits"),
        ));
    }
    if value.bytes().all(|byte| byte == b'0') {
        return Err(record.error(key, "cannot be the zero identity"));
    }
    Ok(value)
}

pub(crate) fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("cannot open evidence {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1_024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("cannot read evidence {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(super::bytes_hex(&hasher.finalize()))
}

pub(crate) fn valid_utc_timestamp(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() < 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes.last() != Some(&b'Z')
    {
        return false;
    }
    let Some(year) = decimal(&bytes[0..4]) else {
        return false;
    };
    let Some(month) = decimal(&bytes[5..7]) else {
        return false;
    };
    let Some(day) = decimal(&bytes[8..10]) else {
        return false;
    };
    let Some(hour) = decimal(&bytes[11..13]) else {
        return false;
    };
    let Some(minute) = decimal(&bytes[14..16]) else {
        return false;
    };
    let Some(second) = decimal(&bytes[17..19]) else {
        return false;
    };
    let maximum_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => return false,
    };
    if year == 0 || !(1..=maximum_day).contains(&day) || hour > 23 || minute > 59 || second > 59 {
        return false;
    }
    match &bytes[19..bytes.len() - 1] {
        [] => true,
        [b'.', fraction @ ..] => {
            !fraction.is_empty() && fraction.len() <= 9 && fraction.iter().all(u8::is_ascii_digit)
        }
        _ => false,
    }
}

fn decimal(bytes: &[u8]) -> Option<u32> {
    if !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    bytes.iter().try_fold(0_u32, |value, byte| {
        value.checked_mul(10)?.checked_add(u32::from(*byte - b'0'))
    })
}

const fn is_leap_year(year: u32) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

#[cfg(test)]
mod tests {
    use core::fmt::Write as _;
    use core::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

    struct Fixture {
        root: PathBuf,
        record: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let suffix = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "alumina-hil-record-{}-{suffix}",
                std::process::id()
            ));
            let evidence = root.join(EVIDENCE_PREFIX).join("run-1");
            let target = root.join(ARTIFACT_PREFIX).join("fixture");
            fs::create_dir_all(&evidence).unwrap();
            fs::create_dir_all(&target).unwrap();
            for relative in [
                "docs/hil/runs/run-1/tinybee.webp",
                "docs/hil/runs/run-1/t-deck.webp",
                "docs/hil/runs/run-1/clocks.bin",
                "docs/hil/runs/run-1/capture.bin",
                "docs/hil/runs/run-1/review.md",
                "target/fixture/tinybee.elf",
                "target/fixture/t-deck.elf",
            ] {
                let path = root.join(relative);
                fs::write(path, relative.as_bytes()).unwrap();
            }
            let record = evidence.join("record.toml");
            let source = valid_record(&root);
            fs::write(&record, source).unwrap();
            Self { root, record }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn digest(root: &Path, relative: &str) -> String {
        sha256_file(&root.join(relative)).unwrap()
    }

    fn valid_record(root: &Path) -> String {
        let mut source = String::new();
        let values = [
            ("schema", "1".to_owned()),
            ("fixture_id", quoted(FIXTURE_ID)),
            ("run_id", quoted("run-1")),
            ("started_utc", quoted("2026-08-11T20:00:00Z")),
            ("operator", quoted("operator")),
            ("reviewer", quoted("reviewer")),
            ("disposition", quoted("pass")),
            ("wifi_condition", quoted("nominal")),
            ("alumina-firmware_commit", quoted(&"1".repeat(40))),
            ("alumina_interface_commit", quoted(&"2".repeat(40))),
            ("hazardous_loads_disconnected", "true".to_owned()),
            ("tinybee_board_id", quoted(TINYBEE_ID)),
            ("tinybee_revision", quoted("V1.0")),
            ("tinybee_serial", quoted("tinybee-fixture")),
            ("tinybee_boot_id", quoted(&"3".repeat(32))),
            ("tinybee_output_route", quoted("I2S0 WS GPIO26")),
            (
                "tinybee_artifact_path",
                quoted("target/fixture/tinybee.elf"),
            ),
            (
                "tinybee_artifact_sha256",
                quoted(&digest(root, "target/fixture/tinybee.elf")),
            ),
            ("tinybee_scheduled_cycle", "1000".to_owned()),
            ("tinybee_observed_earliest_cycle", "1001".to_owned()),
            ("tinybee_observed_latest_cycle", "1001".to_owned()),
            (
                "tinybee_photo_path",
                quoted("docs/hil/runs/run-1/tinybee.webp"),
            ),
            (
                "tinybee_photo_sha256",
                quoted(&digest(root, "docs/hil/runs/run-1/tinybee.webp")),
            ),
            ("tinybee_photo_license", quoted("CC0-1.0")),
            ("tinybee_photo_attribution", quoted("fixture operator")),
            ("t_deck_board_id", quoted(T_DECK_ID)),
            ("t_deck_revision", quoted("V1.0")),
            ("t_deck_serial", quoted("t-deck-fixture")),
            ("t_deck_boot_id", quoted(&"4".repeat(32))),
            ("t_deck_output_route", quoted("reviewed fixture route")),
            ("t_deck_output_reviewed", "true".to_owned()),
            ("t_deck_artifact_path", quoted("target/fixture/t-deck.elf")),
            (
                "t_deck_artifact_sha256",
                quoted(&digest(root, "target/fixture/t-deck.elf")),
            ),
            ("t_deck_scheduled_cycle", "2000".to_owned()),
            ("t_deck_observed_earliest_cycle", "2001".to_owned()),
            ("t_deck_observed_latest_cycle", "2001".to_owned()),
            (
                "t_deck_photo_path",
                quoted("docs/hil/runs/run-1/t-deck.webp"),
            ),
            (
                "t_deck_photo_sha256",
                quoted(&digest(root, "docs/hil/runs/run-1/t-deck.webp")),
            ),
            ("t_deck_photo_license", quoted("CC-BY-4.0")),
            ("t_deck_photo_attribution", quoted("fixture operator")),
            ("target_ui_ns", "5000000".to_owned()),
            ("predicted_max_edge_spread_ns", "100".to_owned()),
            ("reconciled_max_edge_spread_ns", "80".to_owned()),
            ("captured_edge_spread_ns", "75".to_owned()),
            ("analyzer_model", quoted("fixture analyzer")),
            ("analyzer_firmware", quoted("1.0")),
            ("analyzer_sample_rate_hz", "100000000".to_owned()),
            ("analyzer_threshold_mv", "1600".to_owned()),
            ("clock_log_path", quoted("docs/hil/runs/run-1/clocks.bin")),
            (
                "clock_log_sha256",
                quoted(&digest(root, "docs/hil/runs/run-1/clocks.bin")),
            ),
            ("capture_path", quoted("docs/hil/runs/run-1/capture.bin")),
            (
                "capture_sha256",
                quoted(&digest(root, "docs/hil/runs/run-1/capture.bin")),
            ),
            ("review_notes_path", quoted("docs/hil/runs/run-1/review.md")),
            (
                "review_notes_sha256",
                quoted(&digest(root, "docs/hil/runs/run-1/review.md")),
            ),
        ];
        for (key, value) in values {
            writeln!(&mut source, "{key} = {value}").unwrap();
        }
        source
    }

    fn quoted(value: &str) -> String {
        format!("\"{value}\"")
    }

    #[test]
    fn complete_two_board_record_and_every_digest_validate() {
        let fixture = Fixture::new();
        let summary = validate(&fixture.root, &fixture.record).unwrap();
        assert_eq!(summary.run_id, "run-1");
        assert_eq!(summary.disposition, "pass");
        assert_eq!(summary.captured_edge_spread_ns, 75);
    }

    #[test]
    fn tampered_capture_and_unsafe_path_fail_closed() {
        let fixture = Fixture::new();
        fs::write(
            fixture.root.join("docs/hil/runs/run-1/capture.bin"),
            b"tampered",
        )
        .unwrap();
        assert!(
            validate(&fixture.root, &fixture.record)
                .unwrap_err()
                .contains("capture_sha256")
        );

        let source = valid_record(&fixture.root).replace(
            "docs/hil/runs/run-1/review.md",
            "docs/hil/runs/../review.md",
        );
        fs::write(&fixture.record, source).unwrap();
        assert!(
            validate(&fixture.root, &fixture.record)
                .unwrap_err()
                .contains("must remain below")
        );
    }

    #[test]
    fn physical_spread_cannot_exceed_pass_certificate() {
        let fixture = Fixture::new();
        let source = fs::read_to_string(&fixture.record).unwrap().replace(
            "captured_edge_spread_ns = 75",
            "captured_edge_spread_ns = 101",
        );
        fs::write(&fixture.record, source).unwrap();
        assert!(
            validate(&fixture.root, &fixture.record)
                .unwrap_err()
                .contains("pre-commit admitted")
        );
    }

    #[test]
    fn utc_timestamp_validation_is_calendar_exact() {
        assert!(valid_utc_timestamp("2028-02-29T23:59:59.123456789Z"));
        assert!(!valid_utc_timestamp("2027-02-29T23:59:59Z"));
        assert!(!valid_utc_timestamp("2028-01-01T24:00:00Z"));
        assert!(!valid_utc_timestamp("2028-01-01T00:00:00+00:00"));
    }
}
