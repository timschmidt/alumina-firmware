use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::hil_record::{
    ARTIFACT_PREFIX, EVIDENCE_PREFIX, Record, parse_fields, repository_path, require_hex,
    valid_utc_timestamp, verify_digest, verify_evidence_asset,
};
use crate::tinybee_pcm_log::{self, TinyBeePcmSoftwareAttestation};
use crate::tinybee_pcm_vcd::TinyBeePcmVcdAnalysis;

const SCHEMA: u64 = 2;
const FIXTURE_ID: &str = "mks-tinybee-pcm-short-slogic16u3";
const BOARD_ID: &str = board_mks_tinybee::BOARD_ID;
const ANALYZER_MODEL: &str = "Sipeed SLogic16U3";
const PCB_MARKING: &str = "MKS TinyBee v1.0";
const ARTIFACT_PATH: &str =
    "target/xtensa-esp32-none-elf/release/alumina-hil-mks-tinybee-pcm-short-safe";
const SAFE_IMAGE: u64 = board_mks_tinybee::DESCRIBED_SAFE_I2S_IMAGE as u64;
const EXPECTED_BCLK_PERIOD_PS: u64 = 62_500;
const EXPECTED_FRAME_PERIOD_PS: u64 = 4_000_000;
const MINIMUM_SAMPLE_RATE_HZ: u64 = 200_000_000;
const MAXIMUM_SAMPLE_RATE_HZ: u64 = 800_000_000;
const MINIMUM_CAPTURE_MILLISECONDS: u64 = 350;
const MAXIMUM_RTT_LOG_BYTES: u64 = 1_048_576;
const EXPECTED_DMA_FRAMES: u64 = 256;
const EXPECTED_FRAME_RATE_HZ: u64 = 250_000;
const EXPECTED_FRAME_CYCLES: u64 = 4;
const EXPECTED_REFILLS: u64 = 50_000;
const EXIT_COMPLETE: u64 = 1;
const OWNER_STATE_SAFE_REWRITE_ISSUED: u64 = 8;

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
    "rtt_log_path",
    "rtt_log_sha256",
    "usb_logic_power_only",
    "main_power_disconnected",
    "motor_connectors_empty",
    "stepper_driver_sockets_empty",
    "heater_fan_connectors_empty",
    "exp1_display_disconnected",
    "meter_verified_main_power_absent",
    "measured_u1_vcc_mv",
    "measured_marker_high_mv",
    "fixture_photo_path",
    "fixture_photo_sha256",
    "fixture_photo_license",
    "fixture_photo_attribution",
    "annotated_photo_path",
    "annotated_photo_sha256",
    "annotated_photo_license",
    "annotated_photo_attribution",
    "analyzer_model",
    "analyzer_serial",
    "analyzer_firmware",
    "capture_software",
    "capture_software_version",
    "sample_rate_hz",
    "threshold_mv",
    "active_channel_count",
    "bclk_channel",
    "ws_channel",
    "data_channel",
    "marker_channel",
    "ground_lead_count",
    "trigger",
    "pretrigger_samples",
    "sample_count",
    "expected_safe_image",
    "expected_refills",
    "admitted_bclk_period_error_ps",
    "admitted_frame_period_error_ps",
    "required_min_data_setup_ps",
    "required_min_data_hold_ps",
    "decoded_marker_code",
    "decoded_static_image",
    "decoded_stream_image",
    "decoded_live_frames",
    "decoded_live_tail_bclk_count",
    "admitted_max_live_tail_bclk_count",
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
pub struct TinyBeePcmRunSummary {
    pub run_id: String,
    pub disposition: String,
    pub sample_rate_hz: u64,
    pub capture_milliseconds: u64,
    pub decoded_live_frames: u64,
    pub software_attested: bool,
}

pub fn validate(root: &Path, record_path: &Path) -> Result<TinyBeePcmRunSummary, String> {
    let record_path = repository_path(root, record_path, EVIDENCE_PREFIX, "run record")?;
    let source = fs::read_to_string(&record_path)
        .map_err(|error| format!("cannot read {}: {error}", record_path.display()))?;
    let fields = parse_fields(&source, &record_path, FIELDS)?;
    let record = Record::new(&fields, &record_path);

    if record.u64("schema")? != SCHEMA {
        return Err(record.error("schema", "is not the exact supported schema"));
    }
    if record.string("fixture_id")? != FIXTURE_ID {
        return Err(record.error("fixture_id", "is not the TinyBee SLogic fixture"));
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
        return Err(record.error("artifact_path", "does not name the exact HIL release image"));
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
        "meter_verified_main_power_absent",
    ] {
        if !record.boolean(field)? {
            return Err(record.error(field, "must be true for this disconnected-load fixture"));
        }
    }
    if !(3_000..=3_600).contains(&record.u64("measured_u1_vcc_mv")?) {
        return Err(record.error("measured_u1_vcc_mv", "must be within 3.0–3.6 V"));
    }
    if !(3_000..=5_500).contains(&record.u64("measured_marker_high_mv")?) {
        return Err(record.error(
            "measured_marker_high_mv",
            "must be within the reviewed 3.0–5.5 V analyzer range",
        ));
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

    if record.string("analyzer_model")? != ANALYZER_MODEL {
        return Err(record.error("analyzer_model", "does not match the fixture analyzer"));
    }
    record.nonempty("analyzer_serial")?;
    record.nonempty("analyzer_firmware")?;
    record.nonempty("capture_software")?;
    record.nonempty("capture_software_version")?;
    let sample_rate_hz = record.u64("sample_rate_hz")?;
    if !(MINIMUM_SAMPLE_RATE_HZ..=MAXIMUM_SAMPLE_RATE_HZ).contains(&sample_rate_hz) {
        return Err(record.error("sample_rate_hz", "must be within 200–800 MHz"));
    }
    if !(900..=1_600).contains(&record.u64("threshold_mv")?) {
        return Err(record.error("threshold_mv", "must be within 0.9–1.6 V"));
    }
    if record.u64("active_channel_count")? != 4 {
        return Err(record.error("active_channel_count", "must be exactly four"));
    }
    for (field, expected) in [
        ("bclk_channel", 0),
        ("ws_channel", 1),
        ("data_channel", 2),
        ("marker_channel", 3),
    ] {
        if record.u64(field)? != expected {
            return Err(record.error(field, "does not match the reviewed D0–D3 map"));
        }
    }
    if !(1..=4).contains(&record.u64("ground_lead_count")?) {
        return Err(record.error("ground_lead_count", "must be within one to four"));
    }
    if record.string("trigger")? != "bclk-rising" {
        return Err(record.error("trigger", "must capture the first rising BCLK edge"));
    }
    let pretrigger_samples = record.u64("pretrigger_samples")?;
    if u128::from(pretrigger_samples) * 1_000 < u128::from(sample_rate_hz) {
        return Err(record.error("pretrigger_samples", "must retain at least 1 ms"));
    }
    let sample_count = record.u64("sample_count")?;
    if u128::from(sample_count) * 1_000
        < u128::from(sample_rate_hz) * u128::from(MINIMUM_CAPTURE_MILLISECONDS)
    {
        return Err(record.error("sample_count", "must retain at least 350 ms"));
    }
    let capture_milliseconds =
        u64::try_from(u128::from(sample_count) * 1_000 / u128::from(sample_rate_hz))
            .map_err(|_| record.error("sample_count", "duration overflowed"))?;

    if record.u64("expected_safe_image")? != SAFE_IMAGE {
        return Err(record.error("expected_safe_image", "is not canonical 0x001249"));
    }
    if record.u64("expected_refills")? != EXPECTED_REFILLS {
        return Err(record.error("expected_refills", "does not match the artifact"));
    }
    let admitted_bclk_error = nonzero(&record, "admitted_bclk_period_error_ps")?;
    let admitted_frame_error = nonzero(&record, "admitted_frame_period_error_ps")?;
    let required_setup = nonzero(&record, "required_min_data_setup_ps")?;
    let required_hold = nonzero(&record, "required_min_data_hold_ps")?;

    let marker_code = record.u64("decoded_marker_code")?;
    let static_image = record.u64("decoded_static_image")?;
    let stream_image = record.u64("decoded_stream_image")?;
    let decoded_live_frames = record.u64("decoded_live_frames")?;
    let live_tail_bclk_count = record.u64("decoded_live_tail_bclk_count")?;
    let admitted_live_tail = record.u64("admitted_max_live_tail_bclk_count")?;
    let decoded_post_stop_frames = record.u64("decoded_post_stop_complete_frames")?;
    let decoded_post_stop_latches = record.u64("decoded_post_stop_safe_latches")?;
    let invalid_images = record.u64("decoded_invalid_image_count")?;
    let non64_frames = record.u64("decoded_non64_frame_count")?;
    let minimum_bclk = record.u64("minimum_bclk_period_ps")?;
    let maximum_bclk = record.u64("maximum_bclk_period_ps")?;
    let minimum_frame = record.u64("minimum_frame_period_ps")?;
    let maximum_frame = record.u64("maximum_frame_period_ps")?;
    let minimum_setup = record.u64("minimum_data_setup_ps")?;
    let minimum_hold = record.u64("minimum_data_hold_ps")?;
    let expected_analysis = TinyBeePcmVcdAnalysis {
        decoded_marker_code: marker_code,
        decoded_static_image: static_image,
        decoded_stream_image: stream_image,
        decoded_live_frames,
        decoded_live_tail_bclk_count: live_tail_bclk_count,
        decoded_post_stop_complete_frames: decoded_post_stop_frames,
        decoded_post_stop_safe_latches: decoded_post_stop_latches,
        decoded_invalid_image_count: invalid_images,
        decoded_non64_frame_count: non64_frames,
        minimum_bclk_period_ps: minimum_bclk,
        maximum_bclk_period_ps: maximum_bclk,
        minimum_frame_period_ps: minimum_frame,
        maximum_frame_period_ps: maximum_frame,
        minimum_data_setup_ps: minimum_setup,
        minimum_data_hold_ps: minimum_hold,
    };
    let software_attestation = read_software_attestation(root, &record)?;
    if let Some(attestation) = software_attestation {
        validate_software_structure(&record, attestation, marker_code)?;
    }

    if disposition == "pass" {
        let attestation = software_attestation.ok_or_else(|| {
            record.error(
                "rtt_log_path",
                "contains no HIL_PCM_ATTEST_V2 record for a pass disposition",
            )
        })?;
        validate_success_attestation(&record, attestation)?;
        if marker_code != 1 {
            return Err(record.error(
                "decoded_marker_code",
                "does not report complete/stop/rewrite",
            ));
        }
        if static_image != SAFE_IMAGE || stream_image != SAFE_IMAGE {
            return Err(record.error(
                "decoded_stream_image",
                "does not retain canonical safe image",
            ));
        }
        if decoded_live_frames < 50_000 {
            return Err(record.error("decoded_live_frames", "is below the target refill horizon"));
        }
        if admitted_live_tail > 64 || live_tail_bclk_count > admitted_live_tail {
            return Err(record.error(
                "decoded_live_tail_bclk_count",
                "exceeds the admitted final partial-or-complete frame",
            ));
        }
        if decoded_post_stop_frames < 2 {
            return Err(record.error(
                "decoded_post_stop_complete_frames",
                "does not contain both safe rewrites",
            ));
        }
        if decoded_post_stop_latches == 0 {
            return Err(record.error(
                "decoded_post_stop_safe_latches",
                "does not physically latch a post-stop safe frame",
            ));
        }
        if invalid_images != 0 || non64_frames != 0 {
            return Err(record.error("decoded_invalid_image_count", "contains a frame anomaly"));
        }
        require_symmetric_bound(
            &record,
            "minimum_bclk_period_ps",
            "maximum_bclk_period_ps",
            minimum_bclk,
            maximum_bclk,
            EXPECTED_BCLK_PERIOD_PS,
            admitted_bclk_error,
        )?;
        require_symmetric_bound(
            &record,
            "minimum_frame_period_ps",
            "maximum_frame_period_ps",
            minimum_frame,
            maximum_frame,
            EXPECTED_FRAME_PERIOD_PS,
            admitted_frame_error,
        )?;
        if minimum_setup < required_setup {
            return Err(record.error("minimum_data_setup_ps", "is below the admitted minimum"));
        }
        if minimum_hold < required_hold {
            return Err(record.error("minimum_data_hold_ps", "is below the admitted minimum"));
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
        "rtt_log_path",
        "raw_capture_path",
        "vcd_capture_path",
        "analysis_report_path",
        "review_notes_path",
    ] {
        if !asset_paths.insert(record.string(key)?) {
            return Err(record.error(key, "reuses another evidence asset path"));
        }
    }

    let report_value = record.nonempty("analysis_report_path")?;
    let report_path = repository_path(
        root,
        Path::new(&report_value),
        EVIDENCE_PREFIX,
        "analysis report",
    )?;
    let (reported_vcd_digest, reported_analysis) =
        TinyBeePcmVcdAnalysis::read_report(&report_path)?;
    let recorded_vcd_digest = require_hex(&record, "vcd_capture_sha256", 64)?;
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

    Ok(TinyBeePcmRunSummary {
        run_id,
        disposition,
        sample_rate_hz,
        capture_milliseconds,
        decoded_live_frames,
        software_attested: software_attestation.is_some(),
    })
}

fn read_software_attestation(
    root: &Path,
    record: &Record<'_>,
) -> Result<Option<TinyBeePcmSoftwareAttestation>, String> {
    verify_evidence_asset(root, record, "rtt_log_path", "rtt_log_sha256")?;
    let value = record.nonempty("rtt_log_path")?;
    let path = repository_path(root, Path::new(&value), EVIDENCE_PREFIX, "RTT log")?;
    let length = fs::metadata(&path)
        .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?
        .len();
    if length > MAXIMUM_RTT_LOG_BYTES {
        return Err(record.error("rtt_log_path", "exceeds the 1 MiB evidence limit"));
    }
    let source = fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {} as UTF-8: {error}", path.display()))?;
    tinybee_pcm_log::parse(&source)
        .map_err(|error| record.error("rtt_log_path", &format!("is invalid: {error}")))
}

fn validate_software_structure(
    record: &Record<'_>,
    report: TinyBeePcmSoftwareAttestation,
    decoded_marker_code: u64,
) -> Result<(), String> {
    if report.model_epoch != report.start_before {
        return Err(record.error(
            "rtt_log_path",
            "reports a model epoch different from the pre-start observation",
        ));
    }
    if report.start_after < report.start_before
        || report.stop_before < report.start_after
        || report.stop_after < report.stop_before
        || report.rewrite_before < report.stop_after
        || report.rewrite_after < report.rewrite_before
    {
        return Err(record.error(
            "rtt_log_path",
            "reports a nonmonotonic start/stop/rewrite interval",
        ));
    }
    if report.frames != EXPECTED_DMA_FRAMES || report.rate_hz != EXPECTED_FRAME_RATE_HZ {
        return Err(record.error(
            "rtt_log_path",
            "does not match the artifact's 256-frame 250 kHz stream",
        ));
    }
    let expected_marker = report.exit
        | if report.stop_ok { 0 } else { 1 << 3 }
        | if report.rewrite_ok { 0 } else { 1 << 4 };
    if report.marker_code != expected_marker || report.marker_code != decoded_marker_code {
        return Err(record.error(
            "rtt_log_path",
            "does not correlate its outcome with the decoded hardware marker",
        ));
    }
    if report.safe_reclaimed {
        return Err(record.error(
            "rtt_log_path",
            "claims physical safe reclaim from a software-only HIL path",
        ));
    }
    Ok(())
}

fn validate_success_attestation(
    record: &Record<'_>,
    report: TinyBeePcmSoftwareAttestation,
) -> Result<(), String> {
    if report.exit != EXIT_COMPLETE
        || report.accepted_refills != EXPECTED_REFILLS
        || !report.stop_ok
        || !report.rewrite_ok
        || report.owner_state != OWNER_STATE_SAFE_REWRITE_ISSUED
        || report.owner_fault
    {
        return Err(record.error(
            "rtt_log_path",
            "does not report complete refills, stop/rewrite success, and an unfaulted SafeRewriteIssued owner",
        ));
    }
    let total_frames = EXPECTED_DMA_FRAMES
        .checked_add(report.accepted_refills)
        .ok_or_else(|| record.error("rtt_log_path", "overflows the sealed frame count"))?;
    let expected_horizon = total_frames
        .checked_mul(EXPECTED_FRAME_CYCLES)
        .and_then(|cycles| report.model_epoch.checked_add(cycles))
        .ok_or_else(|| record.error("rtt_log_path", "overflows the sealed horizon"))?;
    if report.sealed_horizon != expected_horizon {
        return Err(record.error(
            "rtt_log_path",
            "does not report the exact model epoch plus prefill/refill horizon",
        ));
    }
    Ok(())
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

fn nonzero(record: &Record<'_>, key: &str) -> Result<u64, String> {
    let value = record.u64(key)?;
    if value == 0 {
        Err(record.error(key, "must be nonzero"))
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
    if admitted_error >= expected {
        return Err(record.error(
            minimum_key,
            "policy is not narrower than the nominal period",
        ));
    }
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
                "alumina-tinybee-pcm-record-{}-{suffix}",
                std::process::id()
            ));
            let evidence = root.join(EVIDENCE_PREFIX).join("run-1");
            fs::create_dir_all(&evidence).unwrap();
            fs::create_dir_all(root.join(ARTIFACT_PATH).parent().unwrap()).unwrap();
            for relative in [
                "docs/hil/runs/run-1/fixture.webp",
                "docs/hil/runs/run-1/annotated.webp",
                "docs/hil/runs/run-1/rtt.log",
                "docs/hil/runs/run-1/capture.sr",
                "docs/hil/runs/run-1/capture.vcd",
                "docs/hil/runs/run-1/review.md",
                ARTIFACT_PATH,
            ] {
                let path = root.join(relative);
                fs::write(path, relative.as_bytes()).unwrap();
            }
            fs::write(evidence.join("rtt.log"), valid_rtt()).unwrap();
            let vcd_digest = digest(&root, "docs/hil/runs/run-1/capture.vcd");
            fs::write(
                evidence.join("analysis.toml"),
                valid_analysis().report(&vcd_digest),
            )
            .unwrap();
            let record = evidence.join("record.toml");
            fs::write(&record, valid_record(&root)).unwrap();
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
            ("schema", SCHEMA.to_string()),
            ("fixture_id", quoted(FIXTURE_ID)),
            ("run_id", quoted("run-1")),
            ("started_utc", quoted("2026-08-11T20:00:00Z")),
            ("operator", quoted("operator")),
            ("reviewer", quoted("reviewer")),
            ("disposition", quoted("pass")),
            ("aluminafw_commit", quoted(&"1".repeat(40))),
            ("board_id", quoted(BOARD_ID)),
            ("pcb_marking", quoted("MKS TinyBee v1.0")),
            ("fixture_serial", quoted("tinybee-fixture")),
            ("esp_mac", quoted("c4dee2f8c4ac")),
            (
                "flash_bytes",
                board_mks_tinybee::PRIMARY_FLASH_BYTES.to_string(),
            ),
            ("artifact_path", quoted(ARTIFACT_PATH)),
            ("artifact_sha256", quoted(&digest(root, ARTIFACT_PATH))),
            ("rtt_log_path", quoted("docs/hil/runs/run-1/rtt.log")),
            (
                "rtt_log_sha256",
                quoted(&digest(root, "docs/hil/runs/run-1/rtt.log")),
            ),
            ("usb_logic_power_only", "true".to_owned()),
            ("main_power_disconnected", "true".to_owned()),
            ("motor_connectors_empty", "true".to_owned()),
            ("stepper_driver_sockets_empty", "true".to_owned()),
            ("heater_fan_connectors_empty", "true".to_owned()),
            ("exp1_display_disconnected", "true".to_owned()),
            ("meter_verified_main_power_absent", "true".to_owned()),
            ("measured_u1_vcc_mv", "3300".to_owned()),
            ("measured_marker_high_mv", "5000".to_owned()),
            (
                "fixture_photo_path",
                quoted("docs/hil/runs/run-1/fixture.webp"),
            ),
            (
                "fixture_photo_sha256",
                quoted(&digest(root, "docs/hil/runs/run-1/fixture.webp")),
            ),
            ("fixture_photo_license", quoted("CC0-1.0")),
            ("fixture_photo_attribution", quoted("operator")),
            (
                "annotated_photo_path",
                quoted("docs/hil/runs/run-1/annotated.webp"),
            ),
            (
                "annotated_photo_sha256",
                quoted(&digest(root, "docs/hil/runs/run-1/annotated.webp")),
            ),
            ("annotated_photo_license", quoted("CC0-1.0")),
            ("annotated_photo_attribution", quoted("operator")),
            ("analyzer_model", quoted(ANALYZER_MODEL)),
            ("analyzer_serial", quoted("slogic-fixture")),
            ("analyzer_firmware", quoted("fixture")),
            ("capture_software", quoted("fixture")),
            ("capture_software_version", quoted("1")),
            ("sample_rate_hz", "200000000".to_owned()),
            ("threshold_mv", "1000".to_owned()),
            ("active_channel_count", "4".to_owned()),
            ("bclk_channel", "0".to_owned()),
            ("ws_channel", "1".to_owned()),
            ("data_channel", "2".to_owned()),
            ("marker_channel", "3".to_owned()),
            ("ground_lead_count", "1".to_owned()),
            ("trigger", quoted("bclk-rising")),
            ("pretrigger_samples", "200000".to_owned()),
            ("sample_count", "80000000".to_owned()),
            ("expected_safe_image", SAFE_IMAGE.to_string()),
            ("expected_refills", "50000".to_owned()),
            ("admitted_bclk_period_error_ps", "10000".to_owned()),
            ("admitted_frame_period_error_ps", "20000".to_owned()),
            ("required_min_data_setup_ps", "15000".to_owned()),
            ("required_min_data_hold_ps", "15000".to_owned()),
            ("decoded_marker_code", "1".to_owned()),
            ("decoded_static_image", SAFE_IMAGE.to_string()),
            ("decoded_stream_image", SAFE_IMAGE.to_string()),
            ("decoded_live_frames", "50000".to_owned()),
            ("decoded_live_tail_bclk_count", "32".to_owned()),
            ("admitted_max_live_tail_bclk_count", "64".to_owned()),
            ("decoded_post_stop_complete_frames", "2".to_owned()),
            ("decoded_post_stop_safe_latches", "1".to_owned()),
            ("decoded_invalid_image_count", "0".to_owned()),
            ("decoded_non64_frame_count", "0".to_owned()),
            ("minimum_bclk_period_ps", "60000".to_owned()),
            ("maximum_bclk_period_ps", "65000".to_owned()),
            ("minimum_frame_period_ps", "3995000".to_owned()),
            ("maximum_frame_period_ps", "4005000".to_owned()),
            ("minimum_data_setup_ps", "25000".to_owned()),
            ("minimum_data_hold_ps", "25000".to_owned()),
            ("raw_capture_path", quoted("docs/hil/runs/run-1/capture.sr")),
            (
                "raw_capture_sha256",
                quoted(&digest(root, "docs/hil/runs/run-1/capture.sr")),
            ),
            (
                "vcd_capture_path",
                quoted("docs/hil/runs/run-1/capture.vcd"),
            ),
            (
                "vcd_capture_sha256",
                quoted(&digest(root, "docs/hil/runs/run-1/capture.vcd")),
            ),
            (
                "analysis_report_path",
                quoted("docs/hil/runs/run-1/analysis.toml"),
            ),
            (
                "analysis_report_sha256",
                quoted(&digest(root, "docs/hil/runs/run-1/analysis.toml")),
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

    fn valid_analysis() -> TinyBeePcmVcdAnalysis {
        TinyBeePcmVcdAnalysis {
            decoded_marker_code: 1,
            decoded_static_image: SAFE_IMAGE,
            decoded_stream_image: SAFE_IMAGE,
            decoded_live_frames: 50_000,
            decoded_live_tail_bclk_count: 32,
            decoded_post_stop_complete_frames: 2,
            decoded_post_stop_safe_latches: 1,
            decoded_invalid_image_count: 0,
            decoded_non64_frame_count: 0,
            minimum_bclk_period_ps: 60_000,
            maximum_bclk_period_ps: 65_000,
            minimum_frame_period_ps: 3_995_000,
            maximum_frame_period_ps: 4_005_000,
            minimum_data_setup_ps: 25_000,
            minimum_data_hold_ps: 25_000,
        }
    }

    fn valid_rtt() -> &'static str {
        "[INFO  12.345] HIL_PCM_ATTEST_V2 model_epoch=100 start_before=100 start_after=102 frames=256 rate_hz=250000 exit=1 accepted_refills=50000 sealed_horizon=201124 stop_before=201120 stop_after=201122 stop_ok=1 rewrite_before=201123 rewrite_after=201125 rewrite_ok=1 owner_state=8 owner_fault=0 safe_reclaimed=0 marker_code=1\nHIL_RESULT capture complete; waveform review is still required\n"
    }

    fn quoted(value: &str) -> String {
        format!("\"{value}\"")
    }

    #[test]
    fn complete_disconnected_capture_record_passes_exact_gates() {
        let fixture = Fixture::new();
        let summary = validate(&fixture.root, &fixture.record).unwrap();
        assert_eq!(summary.run_id, "run-1");
        assert_eq!(summary.sample_rate_hz, 200_000_000);
        assert_eq!(summary.capture_milliseconds, 400);
        assert_eq!(summary.decoded_live_frames, 50_000);
        assert!(summary.software_attested);
    }

    #[test]
    fn software_attestation_binds_lifecycle_horizon_and_hardware_marker() {
        for (from, to, expected) in [
            (
                "model_epoch=100 start_before=100",
                "start_before=100 model_epoch=100",
                "expected `model_epoch`",
            ),
            (
                "sealed_horizon=201124",
                "sealed_horizon=201125",
                "exact model epoch",
            ),
            ("owner_state=8", "owner_state=9", "SafeRewriteIssued"),
            ("safe_reclaimed=0", "safe_reclaimed=1", "software-only"),
            ("marker_code=1", "marker_code=2", "decoded hardware marker"),
        ] {
            let fixture = Fixture::new();
            let log_path = fixture.root.join("docs/hil/runs/run-1/rtt.log");
            fs::write(&log_path, valid_rtt().replace(from, to)).unwrap();
            fs::write(&fixture.record, valid_record(&fixture.root)).unwrap();
            assert!(
                validate(&fixture.root, &fixture.record)
                    .unwrap_err()
                    .contains(expected)
            );
        }
    }

    #[test]
    fn missing_post_stop_attestation_is_allowed_only_for_nonpass_evidence() {
        let fixture = Fixture::new();
        let log_path = fixture.root.join("docs/hil/runs/run-1/rtt.log");
        fs::write(
            &log_path,
            "HIL_ABORT circular safe transfer did not start\n",
        )
        .unwrap();
        let pass = valid_record(&fixture.root);
        fs::write(&fixture.record, &pass).unwrap();
        assert!(
            validate(&fixture.root, &fixture.record)
                .unwrap_err()
                .contains("contains no HIL_PCM_ATTEST_V2")
        );

        fs::write(
            &fixture.record,
            pass.replace("disposition = \"pass\"", "disposition = \"inconclusive\""),
        )
        .unwrap();
        let summary = validate(&fixture.root, &fixture.record).unwrap();
        assert!(!summary.software_attested);
    }

    #[test]
    fn connected_load_channel_swap_and_short_capture_fail_closed() {
        for (from, to, expected) in [
            (
                "main_power_disconnected = true",
                "main_power_disconnected = false",
                "disconnected-load",
            ),
            ("ws_channel = 1", "ws_channel = 2", "D0–D3"),
            (
                "sample_count = 80000000",
                "sample_count = 1000000",
                "350 ms",
            ),
        ] {
            let fixture = Fixture::new();
            let source = fs::read_to_string(&fixture.record)
                .unwrap()
                .replace(from, to);
            fs::write(&fixture.record, source).unwrap();
            assert!(
                validate(&fixture.root, &fixture.record)
                    .unwrap_err()
                    .contains(expected)
            );
        }
    }

    #[test]
    fn pass_disposition_requires_safe_images_timing_and_marker_success() {
        for (from, to, expected) in [
            (
                "decoded_marker_code = 1",
                "decoded_marker_code = 9",
                "decoded hardware marker",
            ),
            (
                "decoded_invalid_image_count = 0",
                "decoded_invalid_image_count = 1",
                "frame anomaly",
            ),
            (
                "minimum_data_setup_ps = 25000",
                "minimum_data_setup_ps = 10000",
                "admitted minimum",
            ),
            (
                "maximum_bclk_period_ps = 65000",
                "maximum_bclk_period_ps = 80000",
                "admitted interval",
            ),
        ] {
            let fixture = Fixture::new();
            let source = fs::read_to_string(&fixture.record)
                .unwrap()
                .replace(from, to);
            fs::write(&fixture.record, source).unwrap();
            assert!(
                validate(&fixture.root, &fixture.record)
                    .unwrap_err()
                    .contains(expected)
            );
        }
    }

    #[test]
    fn raw_capture_digest_and_evidence_paths_are_owned() {
        let fixture = Fixture::new();
        fs::write(
            fixture.root.join("docs/hil/runs/run-1/capture.sr"),
            b"tampered",
        )
        .unwrap();
        assert!(
            validate(&fixture.root, &fixture.record)
                .unwrap_err()
                .contains("raw_capture_sha256")
        );

        let fixture = Fixture::new();
        fs::write(
            fixture.root.join("docs/hil/runs/run-1/rtt.log"),
            b"tampered",
        )
        .unwrap();
        assert!(
            validate(&fixture.root, &fixture.record)
                .unwrap_err()
                .contains("rtt_log_sha256")
        );

        let fixture = Fixture::new();
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
    fn analysis_report_must_bind_vcd_and_replay_every_measurement() {
        let fixture = Fixture::new();
        let analysis_path = fixture.root.join("docs/hil/runs/run-1/analysis.toml");
        let mismatched = TinyBeePcmVcdAnalysis {
            decoded_live_frames: 50_001,
            ..valid_analysis()
        };
        fs::write(
            &analysis_path,
            mismatched.report(&digest(&fixture.root, "docs/hil/runs/run-1/capture.vcd")),
        )
        .unwrap();
        fs::write(&fixture.record, valid_record(&fixture.root)).unwrap();
        assert!(
            validate(&fixture.root, &fixture.record)
                .unwrap_err()
                .contains("exactly reproduce")
        );

        fs::write(&analysis_path, valid_analysis().report(&"f".repeat(64))).unwrap();
        fs::write(&fixture.record, valid_record(&fixture.root)).unwrap();
        assert!(
            validate(&fixture.root, &fixture.record)
                .unwrap_err()
                .contains("bind the recorded VCD")
        );
    }
}
