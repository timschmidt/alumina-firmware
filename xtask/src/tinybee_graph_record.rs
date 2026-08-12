use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::hil_record::{
    ARTIFACT_PREFIX, EVIDENCE_PREFIX, Record, parse_fields, repository_path, require_hex,
    valid_utc_timestamp, verify_digest, verify_evidence_asset,
};
use crate::tinybee_graph_vcd::{self, TinyBeeGraphVcdAnalysis};

const SCHEMA: u64 = 1;
const FIXTURE_ID: &str = "mks-tinybee-graph-input-timing-slogic16u3";
const BOARD_ID: &str = board_mks_tinybee::BOARD_ID;
const ANALYZER_MODEL: &str = "Sipeed SLogic16U3";
const PCB_MARKING: &str = "MKS TinyBee v1.0";
const ARTIFACT_PATH: &str =
    "target/xtensa-esp32-none-elf/release/alumina-hil-mks-tinybee-graph-input-timing-safe";
const AP_SSID: &str = "Alumina-mks-tinybee-v1";
const HTTP_LOAD_PATH: &str = "/api/v1/health";
const EXPECTED_RELEASE_PERIOD_PS: u64 = 1_000_000_000;
const EXPECTED_DEBOUNCE_PS: u64 = 2_000_000_000;
const MAXIMUM_PERIOD_ERROR_PS: u64 = 100_000_000;
const MAXIMUM_RELEASE_PULSE_PS: u64 = 300_000_000;
const MAXIMUM_INPUT_TO_SINK_PS: u64 = 3_200_000_000;
const MAXIMUM_SINK_AFTER_RELEASE_PS: u64 = 10_000_000;
const MINIMUM_SAMPLE_RATE_HZ: u64 = 50_000_000;
const MAXIMUM_SAMPLE_RATE_HZ: u64 = 800_000_000;
const MINIMUM_CAPTURE_MILLISECONDS: u64 = 200;
const MINIMUM_PRETRIGGER_MILLISECONDS: u64 = 20;
const MINIMUM_RELEASE_PULSES: u64 = 150;
const MINIMUM_SIDE_RELEASE_PULSES: u64 = 20;

const FIELDS: &[&str] = &[
    "schema",
    "fixture_id",
    "run_id",
    "started_utc",
    "operator",
    "reviewer",
    "disposition",
    "aluminafw_commit",
    "board_id",
    "pcb_marking",
    "fixture_serial",
    "esp_mac",
    "flash_bytes",
    "artifact_path",
    "artifact_sha256",
    "usb_logic_power_only",
    "main_power_disconnected",
    "motor_connectors_empty",
    "stepper_driver_sockets_empty",
    "heater_fan_connectors_empty",
    "exp1_display_disconnected",
    "x_endstop_initially_open",
    "x_endstop_signal_shunted_to_ground_only",
    "meter_verified_main_power_absent",
    "measured_x_signal_open_mv",
    "measured_timing_marker_high_mv",
    "measured_sink_marker_high_mv",
    "fixture_photo_path",
    "fixture_photo_sha256",
    "fixture_photo_license",
    "fixture_photo_attribution",
    "annotated_photo_path",
    "annotated_photo_sha256",
    "annotated_photo_license",
    "annotated_photo_attribution",
    "wifi_associated",
    "ap_ssid",
    "http_load_path",
    "http_load_client",
    "http_load_requests",
    "http_load_failures",
    "http_load_log_path",
    "http_load_log_sha256",
    "analyzer_model",
    "analyzer_serial",
    "analyzer_firmware",
    "capture_software",
    "capture_software_version",
    "sample_rate_hz",
    "threshold_mv",
    "active_channel_count",
    "timing_channel",
    "input_channel",
    "sink_channel",
    "ground_lead_count",
    "trigger",
    "pretrigger_samples",
    "sample_count",
    "expected_release_period_ps",
    "admitted_release_period_error_ps",
    "expected_debounce_ps",
    "admitted_max_release_pulse_ps",
    "admitted_max_input_to_sink_ps",
    "admitted_max_sink_after_release_ps",
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
    "raw_capture_path",
    "raw_capture_sha256",
    "vcd_capture_path",
    "vcd_capture_sha256",
    "analysis_report_path",
    "analysis_report_sha256",
    "review_notes_path",
    "review_notes_sha256",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TinyBeeGraphRunSummary {
    pub run_id: String,
    pub disposition: String,
    pub sample_rate_hz: u64,
    pub capture_milliseconds: u64,
    pub release_pulse_count: u64,
    pub input_to_sink_ps: u64,
}

pub fn validate(root: &Path, record_path: &Path) -> Result<TinyBeeGraphRunSummary, String> {
    let record_path = repository_path(root, record_path, EVIDENCE_PREFIX, "run record")?;
    let source = fs::read_to_string(&record_path)
        .map_err(|error| format!("cannot read {}: {error}", record_path.display()))?;
    let fields = parse_fields(&source, &record_path, FIELDS)?;
    let record = Record::new(&fields, &record_path);

    if record.u64("schema")? != SCHEMA {
        return Err(record.error("schema", "is not the exact supported schema"));
    }
    if record.string("fixture_id")? != FIXTURE_ID {
        return Err(record.error("fixture_id", "is not the TinyBee graph-input fixture"));
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
    require_hex(&record, "aluminafw_commit", 40)?;
    if record.string("board_id")? != BOARD_ID {
        return Err(record.error("board_id", "does not select the 8 MiB primary package"));
    }
    if record.string("pcb_marking")? != PCB_MARKING {
        return Err(record.error("pcb_marking", "does not match the reviewed V1.0 fixture"));
    }
    record.nonempty("fixture_serial")?;
    require_hex(&record, "esp_mac", 12)?;
    if record.u64("flash_bytes")? != board_mks_tinybee::PRIMARY_FLASH_BYTES as u64 {
        return Err(record.error("flash_bytes", "does not match the primary package"));
    }

    let artifact_value = record.nonempty("artifact_path")?;
    if artifact_value != ARTIFACT_PATH {
        return Err(record.error("artifact_path", "does not name the exact graph HIL image"));
    }
    let artifact = repository_path(
        root,
        Path::new(&artifact_value),
        ARTIFACT_PREFIX,
        "artifact",
    )?;
    verify_digest(&record, "artifact_sha256", &artifact)?;

    for field in [
        "usb_logic_power_only",
        "main_power_disconnected",
        "motor_connectors_empty",
        "stepper_driver_sockets_empty",
        "heater_fan_connectors_empty",
        "exp1_display_disconnected",
        "x_endstop_initially_open",
        "x_endstop_signal_shunted_to_ground_only",
        "meter_verified_main_power_absent",
    ] {
        if !record.boolean(field)? {
            return Err(record.error(field, "must be true for this disconnected-load fixture"));
        }
    }
    if !(2_700..=3_600).contains(&record.u64("measured_x_signal_open_mv")?) {
        return Err(record.error(
            "measured_x_signal_open_mv",
            "must establish the protected input's open high level",
        ));
    }
    for field in [
        "measured_timing_marker_high_mv",
        "measured_sink_marker_high_mv",
    ] {
        if !(3_000..=5_500).contains(&record.u64(field)?) {
            return Err(record.error(field, "must be within the reviewed 3.0–5.5 V range"));
        }
    }

    validate_photo(&record, root, "fixture")?;
    validate_photo(&record, root, "annotated")?;
    if record.string("fixture_photo_path")? == record.string("annotated_photo_path")?
        || require_hex(&record, "fixture_photo_sha256", 64)?
            == require_hex(&record, "annotated_photo_sha256", 64)?
    {
        return Err(record.error(
            "annotated_photo_path",
            "must be a distinct annotated derivative of the fixture photo",
        ));
    }

    if !record.boolean("wifi_associated")? {
        return Err(record.error("wifi_associated", "must prove an active Wi-Fi client"));
    }
    if record.string("ap_ssid")? != AP_SSID {
        return Err(record.error("ap_ssid", "does not name the fixture AP"));
    }
    if record.string("http_load_path")? != HTTP_LOAD_PATH {
        return Err(record.error("http_load_path", "does not select the static health route"));
    }
    record.nonempty("http_load_client")?;
    let http_load_requests = record.u64("http_load_requests")?;
    let http_load_failures = record.u64("http_load_failures")?;
    verify_evidence_asset(root, &record, "http_load_log_path", "http_load_log_sha256")?;

    if record.string("analyzer_model")? != ANALYZER_MODEL {
        return Err(record.error("analyzer_model", "does not match the fixture analyzer"));
    }
    record.nonempty("analyzer_serial")?;
    record.nonempty("analyzer_firmware")?;
    record.nonempty("capture_software")?;
    record.nonempty("capture_software_version")?;
    let sample_rate_hz = record.u64("sample_rate_hz")?;
    if !(MINIMUM_SAMPLE_RATE_HZ..=MAXIMUM_SAMPLE_RATE_HZ).contains(&sample_rate_hz) {
        return Err(record.error("sample_rate_hz", "must be within 50–800 MHz"));
    }
    if !(900..=1_600).contains(&record.u64("threshold_mv")?) {
        return Err(record.error("threshold_mv", "must be within 0.9–1.6 V"));
    }
    if record.u64("active_channel_count")? != 3 {
        return Err(record.error("active_channel_count", "must be exactly three"));
    }
    for (field, expected) in [
        ("timing_channel", 0),
        ("input_channel", 1),
        ("sink_channel", 2),
    ] {
        if record.u64(field)? != expected {
            return Err(record.error(field, "does not match the reviewed D0–D2 map"));
        }
    }
    if !(1..=3).contains(&record.u64("ground_lead_count")?) {
        return Err(record.error("ground_lead_count", "must be within one to three"));
    }
    if record.string("trigger")? != "x-endstop-falling" {
        return Err(record.error("trigger", "must capture the active-low X-endstop assertion"));
    }
    let pretrigger_samples = record.u64("pretrigger_samples")?;
    if u128::from(pretrigger_samples) * 1_000
        < u128::from(sample_rate_hz) * u128::from(MINIMUM_PRETRIGGER_MILLISECONDS)
    {
        return Err(record.error("pretrigger_samples", "must retain at least 20 ms"));
    }
    let sample_count = record.u64("sample_count")?;
    if u128::from(sample_count) * 1_000
        < u128::from(sample_rate_hz) * u128::from(MINIMUM_CAPTURE_MILLISECONDS)
    {
        return Err(record.error("sample_count", "must retain at least 200 ms"));
    }
    let capture_milliseconds =
        u64::try_from(u128::from(sample_count) * 1_000 / u128::from(sample_rate_hz))
            .map_err(|_| record.error("sample_count", "duration overflowed"))?;

    if record.u64("expected_release_period_ps")? != EXPECTED_RELEASE_PERIOD_PS {
        return Err(record.error(
            "expected_release_period_ps",
            "is not the 1 kHz fixture period",
        ));
    }
    if record.u64("expected_debounce_ps")? != EXPECTED_DEBOUNCE_PS {
        return Err(record.error(
            "expected_debounce_ps",
            "is not the 2 ms configured debounce",
        ));
    }
    let admitted_period_error = bounded_nonzero(
        &record,
        "admitted_release_period_error_ps",
        MAXIMUM_PERIOD_ERROR_PS,
    )?;
    let admitted_release_pulse = bounded_nonzero(
        &record,
        "admitted_max_release_pulse_ps",
        MAXIMUM_RELEASE_PULSE_PS,
    )?;
    let admitted_input_to_sink = bounded_nonzero(
        &record,
        "admitted_max_input_to_sink_ps",
        MAXIMUM_INPUT_TO_SINK_PS,
    )?;
    if admitted_input_to_sink < EXPECTED_DEBOUNCE_PS {
        return Err(record.error(
            "admitted_max_input_to_sink_ps",
            "cannot be shorter than the configured debounce",
        ));
    }
    let admitted_sink_after_release = bounded_nonzero(
        &record,
        "admitted_max_sink_after_release_ps",
        MAXIMUM_SINK_AFTER_RELEASE_PS,
    )?;

    let expected_analysis = TinyBeeGraphVcdAnalysis {
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

    if disposition == "pass" {
        if http_load_requests < 25 || http_load_failures != 0 {
            return Err(record.error(
                "http_load_requests",
                "must retain at least 25 successful health requests and zero failures",
            ));
        }
        if expected_analysis.capture_duration_ps < MINIMUM_CAPTURE_MILLISECONDS * 1_000_000_000
            || expected_analysis.release_pulse_count < MINIMUM_RELEASE_PULSES
            || expected_analysis.pre_assertion_release_count < MINIMUM_SIDE_RELEASE_PULSES
            || expected_analysis.post_sink_release_count < MINIMUM_SIDE_RELEASE_PULSES
        {
            return Err(record.error(
                "release_pulse_count",
                "does not retain the required pre/post timing horizon",
            ));
        }
        if expected_analysis.raw_input_transition_count == 0
            || expected_analysis.raw_input_transition_count > 16
            || expected_analysis.sink_transition_count != 1
        {
            return Err(record.error(
                "raw_input_transition_count",
                "does not contain one bounded assertion episode",
            ));
        }
        require_symmetric_bound(
            &record,
            "minimum_release_period_ps",
            "maximum_release_period_ps",
            expected_analysis.minimum_release_period_ps,
            expected_analysis.maximum_release_period_ps,
            EXPECTED_RELEASE_PERIOD_PS,
            admitted_period_error,
        )?;
        if expected_analysis.maximum_release_pulse_ps > admitted_release_pulse {
            return Err(record.error(
                "maximum_release_pulse_ps",
                "exceeds the graph WCET plus executor-reserve budget",
            ));
        }
        let debounce_floor = EXPECTED_DEBOUNCE_PS - 100_000_000;
        if expected_analysis.input_to_sink_ps < debounce_floor
            || expected_analysis.input_to_sink_ps > admitted_input_to_sink
        {
            return Err(record.error(
                "input_to_sink_ps",
                "does not respect the configured debounce and admitted dispatch latency",
            ));
        }
        if expected_analysis.sink_after_release_ps > admitted_sink_after_release {
            return Err(record.error(
                "sink_after_release_ps",
                "does not follow the correlated completed graph release",
            ));
        }
    }

    for (path_key, digest_key) in [
        ("raw_capture_path", "raw_capture_sha256"),
        ("vcd_capture_path", "vcd_capture_sha256"),
        ("analysis_report_path", "analysis_report_sha256"),
        ("review_notes_path", "review_notes_sha256"),
    ] {
        verify_evidence_asset(root, &record, path_key, digest_key)?;
    }
    let mut asset_paths = BTreeSet::new();
    for key in [
        "fixture_photo_path",
        "annotated_photo_path",
        "http_load_log_path",
        "raw_capture_path",
        "vcd_capture_path",
        "analysis_report_path",
        "review_notes_path",
    ] {
        if !asset_paths.insert(record.string(key)?) {
            return Err(record.error(key, "reuses another evidence asset path"));
        }
    }

    let vcd_value = record.nonempty("vcd_capture_path")?;
    let recorded_vcd_digest = require_hex(&record, "vcd_capture_sha256", 64)?;
    let (decoded_vcd_digest, decoded_analysis) =
        tinybee_graph_vcd::analyze(root, Path::new(&vcd_value))?;
    if decoded_vcd_digest != recorded_vcd_digest {
        return Err(record.error(
            "vcd_capture_path",
            "does not reproduce the recorded VCD digest",
        ));
    }
    if decoded_analysis != expected_analysis {
        return Err(record.error(
            "vcd_capture_path",
            "does not exactly reproduce the recorded decoded measurements",
        ));
    }

    let report_value = record.nonempty("analysis_report_path")?;
    let report_path = repository_path(
        root,
        Path::new(&report_value),
        EVIDENCE_PREFIX,
        "analysis report",
    )?;
    let (reported_vcd_digest, reported_analysis) =
        TinyBeeGraphVcdAnalysis::read_report(&report_path)?;
    if reported_vcd_digest != recorded_vcd_digest {
        return Err(record.error(
            "analysis_report_path",
            "does not bind the recorded VCD digest",
        ));
    }
    if reported_analysis != expected_analysis {
        return Err(record.error(
            "analysis_report_path",
            "does not exactly reproduce the recorded decoded measurements",
        ));
    }

    Ok(TinyBeeGraphRunSummary {
        run_id,
        disposition,
        sample_rate_hz,
        capture_milliseconds,
        release_pulse_count: expected_analysis.release_pulse_count,
        input_to_sink_ps: expected_analysis.input_to_sink_ps,
    })
}

fn validate_photo(record: &Record<'_>, root: &Path, prefix: &str) -> Result<(), String> {
    let path_key = format!("{prefix}_photo_path");
    let digest_key = format!("{prefix}_photo_sha256");
    verify_evidence_asset(root, record, &path_key, &digest_key)?;
    let license_key = format!("{prefix}_photo_license");
    if !matches!(
        record.string(&license_key)?.as_str(),
        "CC0-1.0" | "CC-BY-4.0"
    ) {
        return Err(record.error(&license_key, "must be CC0-1.0 or CC-BY-4.0"));
    }
    record.nonempty(&format!("{prefix}_photo_attribution"))?;
    Ok(())
}

fn bounded_nonzero(record: &Record<'_>, key: &str, maximum: u64) -> Result<u64, String> {
    let value = record.u64(key)?;
    if value == 0 || value > maximum {
        Err(record.error(key, "must be nonzero and no wider than the reviewed policy"))
    } else {
        Ok(value)
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "both observed endpoints and their named policy fields remain explicit"
)]
fn require_symmetric_bound(
    record: &Record<'_>,
    minimum_key: &str,
    maximum_key: &str,
    minimum: u64,
    maximum: u64,
    expected: u64,
    admitted_error: u64,
) -> Result<(), String> {
    let admitted_minimum = expected
        .checked_sub(admitted_error)
        .ok_or_else(|| record.error(minimum_key, "policy underflowed"))?;
    let admitted_maximum = expected
        .checked_add(admitted_error)
        .ok_or_else(|| record.error(maximum_key, "policy overflowed"))?;
    if minimum < admitted_minimum || minimum > admitted_maximum {
        return Err(record.error(minimum_key, "is outside the admitted interval"));
    }
    if maximum < admitted_minimum || maximum > admitted_maximum || maximum < minimum {
        return Err(record.error(maximum_key, "is outside the admitted interval"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use core::fmt::Write as _;
    use core::sync::atomic::{AtomicU64, Ordering};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use crate::hil_record::sha256_file;

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
                "alumina-tinybee-graph-record-{}-{suffix}",
                std::process::id()
            ));
            let run = root.join(EVIDENCE_PREFIX).join("run-1");
            fs::create_dir_all(&run).unwrap();
            fs::create_dir_all(root.join(ARTIFACT_PATH).parent().unwrap()).unwrap();
            for relative in [
                "docs/hil/runs/run-1/fixture.webp",
                "docs/hil/runs/run-1/annotated.webp",
                "docs/hil/runs/run-1/http-load.log",
                "docs/hil/runs/run-1/capture.sr",
                "docs/hil/runs/run-1/review.md",
                ARTIFACT_PATH,
            ] {
                fs::write(root.join(relative), relative.as_bytes()).unwrap();
            }
            fs::write(run.join("capture.vcd"), valid_vcd()).unwrap();
            let vcd_digest = digest(&root, "docs/hil/runs/run-1/capture.vcd");
            fs::write(
                run.join("analysis.toml"),
                valid_analysis().report(&vcd_digest),
            )
            .unwrap();
            let record = run.join("record.toml");
            fs::write(&record, valid_record(&root)).unwrap();
            Self { root, record }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn valid_analysis() -> TinyBeeGraphVcdAnalysis {
        TinyBeeGraphVcdAnalysis {
            capture_duration_ps: 301_000_000_000,
            release_pulse_count: 300,
            pre_assertion_release_count: 50,
            post_sink_release_count: 247,
            raw_input_transition_count: 1,
            sink_transition_count: 1,
            first_input_assertion_ps: 50_501_100_000,
            qualifying_input_assertion_ps: 50_501_100_000,
            sink_assertion_ps: 53_001_100_000,
            input_to_sink_ps: 2_500_000_000,
            sink_after_release_ps: 100_000,
            minimum_release_pulse_ps: 1_000_000,
            maximum_release_pulse_ps: 1_000_000,
            minimum_release_period_ps: 1_000_000_000,
            maximum_release_period_ps: 1_000_000_000,
        }
    }

    fn valid_vcd() -> String {
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
        for release in 1_u64..=300 {
            let rise = release * 1_000_000;
            let fall = rise + 1_000;
            writeln!(vcd, "#{rise}\n1!\n#{fall}\n0!").unwrap();
            if release == 50 {
                writeln!(vcd, "#50501100\n0\"").unwrap();
            }
            if release == 53 {
                writeln!(vcd, "#53001100\n1#").unwrap();
            }
        }
        vcd.push_str("#301000000\n");
        vcd
    }

    fn digest(root: &Path, relative: &str) -> String {
        sha256_file(&root.join(relative)).unwrap()
    }

    fn quoted(value: &str) -> String {
        format!("\"{value}\"")
    }

    fn valid_record(root: &Path) -> String {
        let analysis = valid_analysis();
        let mut fields: BTreeMap<String, String> = BTreeMap::new();
        {
            let mut set = |key: &str, value: String| {
                fields.insert(key.to_owned(), value);
            };
            set("schema", "1".to_owned());
            set("fixture_id", quoted(FIXTURE_ID));
            set("run_id", quoted("run-1"));
            set("started_utc", quoted("2026-08-12T12:00:00Z"));
            set("operator", quoted("operator"));
            set("reviewer", quoted("reviewer"));
            set("disposition", quoted("pass"));
            set("aluminafw_commit", quoted(&"1".repeat(40)));
            set("board_id", quoted(BOARD_ID));
            set("pcb_marking", quoted(PCB_MARKING));
            set("fixture_serial", quoted("tinybee-1"));
            set("esp_mac", quoted("c4dee2f8c4ac"));
            set(
                "flash_bytes",
                board_mks_tinybee::PRIMARY_FLASH_BYTES.to_string(),
            );
            set("artifact_path", quoted(ARTIFACT_PATH));
            set("artifact_sha256", quoted(&digest(root, ARTIFACT_PATH)));
            for key in [
                "usb_logic_power_only",
                "main_power_disconnected",
                "motor_connectors_empty",
                "stepper_driver_sockets_empty",
                "heater_fan_connectors_empty",
                "exp1_display_disconnected",
                "x_endstop_initially_open",
                "x_endstop_signal_shunted_to_ground_only",
                "meter_verified_main_power_absent",
            ] {
                set(key, "true".to_owned());
            }
            set("measured_x_signal_open_mv", "3300".to_owned());
            set("measured_timing_marker_high_mv", "5000".to_owned());
            set("measured_sink_marker_high_mv", "5000".to_owned());
            for (prefix, filename) in [("fixture", "fixture.webp"), ("annotated", "annotated.webp")]
            {
                let relative = format!("docs/hil/runs/run-1/{filename}");
                set(&format!("{prefix}_photo_path"), quoted(&relative));
                set(
                    &format!("{prefix}_photo_sha256"),
                    quoted(&digest(root, &relative)),
                );
                set(&format!("{prefix}_photo_license"), quoted("CC0-1.0"));
                set(&format!("{prefix}_photo_attribution"), quoted("operator"));
            }
            set("wifi_associated", "true".to_owned());
            set("ap_ssid", quoted(AP_SSID));
            set("http_load_path", quoted(HTTP_LOAD_PATH));
            set("http_load_client", quoted("curl 8"));
            set("http_load_requests", "100".to_owned());
            set("http_load_failures", "0".to_owned());
            set(
                "http_load_log_path",
                quoted("docs/hil/runs/run-1/http-load.log"),
            );
            set(
                "http_load_log_sha256",
                quoted(&digest(root, "docs/hil/runs/run-1/http-load.log")),
            );
            set("analyzer_model", quoted(ANALYZER_MODEL));
            set("analyzer_serial", quoted("slogic-1"));
            set("analyzer_firmware", quoted("fixture"));
            set("capture_software", quoted("PulseView"));
            set("capture_software_version", quoted("fixture"));
            set("sample_rate_hz", "400000000".to_owned());
            set("threshold_mv", "1600".to_owned());
            set("active_channel_count", "3".to_owned());
            set("timing_channel", "0".to_owned());
            set("input_channel", "1".to_owned());
            set("sink_channel", "2".to_owned());
            set("ground_lead_count", "1".to_owned());
            set("trigger", quoted("x-endstop-falling"));
            set("pretrigger_samples", "20000000".to_owned());
            set("sample_count", "120000000".to_owned());
            set(
                "expected_release_period_ps",
                EXPECTED_RELEASE_PERIOD_PS.to_string(),
            );
            set("admitted_release_period_error_ps", "100000000".to_owned());
            set("expected_debounce_ps", EXPECTED_DEBOUNCE_PS.to_string());
            set("admitted_max_release_pulse_ps", "300000000".to_owned());
            set("admitted_max_input_to_sink_ps", "3200000000".to_owned());
            set("admitted_max_sink_after_release_ps", "10000000".to_owned());
            for (key, value) in [
                ("capture_duration_ps", analysis.capture_duration_ps),
                ("release_pulse_count", analysis.release_pulse_count),
                (
                    "pre_assertion_release_count",
                    analysis.pre_assertion_release_count,
                ),
                ("post_sink_release_count", analysis.post_sink_release_count),
                (
                    "raw_input_transition_count",
                    analysis.raw_input_transition_count,
                ),
                ("sink_transition_count", analysis.sink_transition_count),
                (
                    "first_input_assertion_ps",
                    analysis.first_input_assertion_ps,
                ),
                (
                    "qualifying_input_assertion_ps",
                    analysis.qualifying_input_assertion_ps,
                ),
                ("sink_assertion_ps", analysis.sink_assertion_ps),
                ("input_to_sink_ps", analysis.input_to_sink_ps),
                ("sink_after_release_ps", analysis.sink_after_release_ps),
                (
                    "minimum_release_pulse_ps",
                    analysis.minimum_release_pulse_ps,
                ),
                (
                    "maximum_release_pulse_ps",
                    analysis.maximum_release_pulse_ps,
                ),
                (
                    "minimum_release_period_ps",
                    analysis.minimum_release_period_ps,
                ),
                (
                    "maximum_release_period_ps",
                    analysis.maximum_release_period_ps,
                ),
            ] {
                set(key, value.to_string());
            }
            for (key, relative) in [
                ("raw_capture_path", "docs/hil/runs/run-1/capture.sr"),
                ("vcd_capture_path", "docs/hil/runs/run-1/capture.vcd"),
                ("analysis_report_path", "docs/hil/runs/run-1/analysis.toml"),
                ("review_notes_path", "docs/hil/runs/run-1/review.md"),
            ] {
                set(key, quoted(relative));
                let digest_key = key.replace("_path", "_sha256");
                set(&digest_key, quoted(&digest(root, relative)));
            }
        }

        let mut source = String::new();
        for key in FIELDS {
            source.push_str(key);
            source.push_str(" = ");
            source.push_str(fields.get(*key).unwrap());
            source.push('\n');
        }
        source
    }

    #[test]
    fn complete_graph_input_record_passes_exact_gates() {
        let fixture = Fixture::new();
        let summary = validate(&fixture.root, &fixture.record).unwrap();
        assert_eq!(summary.disposition, "pass");
        assert_eq!(summary.release_pulse_count, 300);
        assert_eq!(summary.input_to_sink_ps, 2_500_000_000);
    }

    #[test]
    fn widened_policy_and_tampered_analysis_fail_closed() {
        let fixture = Fixture::new();
        let original = fs::read_to_string(&fixture.record).unwrap();
        fs::write(
            &fixture.record,
            original.replace(
                "admitted_max_release_pulse_ps = 300000000",
                "admitted_max_release_pulse_ps = 300000001",
            ),
        )
        .unwrap();
        assert!(validate(&fixture.root, &fixture.record).is_err());

        fs::write(
            &fixture.record,
            original.replace(
                "maximum_release_pulse_ps = 1000000",
                "maximum_release_pulse_ps = 1200000",
            ),
        )
        .unwrap();
        let error = validate(&fixture.root, &fixture.record).unwrap_err();
        assert!(error.contains("does not exactly reproduce"));
    }
}
