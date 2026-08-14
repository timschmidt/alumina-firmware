//! Exact parser for the TinyBee PCM-short HIL software attestation.
//!
//! Monitor timestamps and severity prefixes are intentionally outside the
//! contract. The firmware-owned suffix is one ordered, numeric, whitespace-
//! separated record so a logging frontend cannot reinterpret debug text.

const PREFIX: &str = "HIL_PCM_ATTEST_V2 ";
const FIELD_COUNT: usize = 18;

/// One post-stop software report emitted by the disconnected-load HIL image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TinyBeePcmSoftwareAttestation {
    pub model_epoch: u64,
    pub start_before: u64,
    pub start_after: u64,
    pub frames: u64,
    pub rate_hz: u64,
    pub exit: u64,
    pub accepted_refills: u64,
    pub sealed_horizon: u64,
    pub stop_before: u64,
    pub stop_after: u64,
    pub stop_ok: bool,
    pub rewrite_before: u64,
    pub rewrite_after: u64,
    pub rewrite_ok: bool,
    pub owner_state: u64,
    pub owner_fault: bool,
    pub safe_reclaimed: bool,
    pub marker_code: u64,
}

/// Finds and parses at most one firmware-owned V2 attestation.
///
/// `Ok(None)` is meaningful for a start failure, where the HIL image cannot
/// reach its common post-stop report. Duplicate or malformed attestations are
/// rejected instead of selecting one occurrence.
pub(crate) fn parse(source: &str) -> Result<Option<TinyBeePcmSoftwareAttestation>, String> {
    let mut matches = source.match_indices(PREFIX);
    let Some((offset, _)) = matches.next() else {
        return Ok(None);
    };
    if matches.next().is_some() {
        return Err("RTT log contains more than one HIL_PCM_ATTEST_V2 record".to_owned());
    }

    let suffix = &source[offset + PREFIX.len()..];
    let line_end = suffix.find(['\r', '\n']).unwrap_or(suffix.len());
    let line = &suffix[..line_end];
    let mut tokens = line.split_ascii_whitespace();
    let values = [
        parse_field(&mut tokens, "model_epoch")?,
        parse_field(&mut tokens, "start_before")?,
        parse_field(&mut tokens, "start_after")?,
        parse_field(&mut tokens, "frames")?,
        parse_field(&mut tokens, "rate_hz")?,
        parse_field(&mut tokens, "exit")?,
        parse_field(&mut tokens, "accepted_refills")?,
        parse_field(&mut tokens, "sealed_horizon")?,
        parse_field(&mut tokens, "stop_before")?,
        parse_field(&mut tokens, "stop_after")?,
        parse_field(&mut tokens, "stop_ok")?,
        parse_field(&mut tokens, "rewrite_before")?,
        parse_field(&mut tokens, "rewrite_after")?,
        parse_field(&mut tokens, "rewrite_ok")?,
        parse_field(&mut tokens, "owner_state")?,
        parse_field(&mut tokens, "owner_fault")?,
        parse_field(&mut tokens, "safe_reclaimed")?,
        parse_field(&mut tokens, "marker_code")?,
    ];
    if tokens.next().is_some() {
        return Err(format!(
            "HIL_PCM_ATTEST_V2 contains more than {FIELD_COUNT} fields"
        ));
    }
    if !(1..=7).contains(&values[5]) {
        return Err("HIL_PCM_ATTEST_V2 exit must be within 1..=7".to_owned());
    }
    if !(1..=10).contains(&values[14]) {
        return Err("HIL_PCM_ATTEST_V2 owner_state must be within 1..=10".to_owned());
    }
    if values[17] > 31 {
        return Err("HIL_PCM_ATTEST_V2 marker_code must be within 0..=31".to_owned());
    }

    Ok(Some(TinyBeePcmSoftwareAttestation {
        model_epoch: values[0],
        start_before: values[1],
        start_after: values[2],
        frames: values[3],
        rate_hz: values[4],
        exit: values[5],
        accepted_refills: values[6],
        sealed_horizon: values[7],
        stop_before: values[8],
        stop_after: values[9],
        stop_ok: parse_bit(values[10], "stop_ok")?,
        rewrite_before: values[11],
        rewrite_after: values[12],
        rewrite_ok: parse_bit(values[13], "rewrite_ok")?,
        owner_state: values[14],
        owner_fault: parse_bit(values[15], "owner_fault")?,
        safe_reclaimed: parse_bit(values[16], "safe_reclaimed")?,
        marker_code: values[17],
    }))
}

fn parse_field<'a>(
    tokens: &mut impl Iterator<Item = &'a str>,
    expected: &str,
) -> Result<u64, String> {
    let token = tokens
        .next()
        .ok_or_else(|| format!("HIL_PCM_ATTEST_V2 is missing `{expected}`"))?;
    let (name, value) = token
        .split_once('=')
        .ok_or_else(|| format!("HIL_PCM_ATTEST_V2 `{expected}` has no equals sign"))?;
    if name != expected {
        return Err(format!(
            "HIL_PCM_ATTEST_V2 expected `{expected}` but found `{name}`"
        ));
    }
    if value.is_empty()
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || value.len() > 1 && value.starts_with('0')
    {
        return Err(format!(
            "HIL_PCM_ATTEST_V2 `{expected}` is not a canonical unsigned decimal integer"
        ));
    }
    value
        .parse()
        .map_err(|error| format!("HIL_PCM_ATTEST_V2 `{expected}` is invalid: {error}"))
}

fn parse_bit(value: u64, name: &str) -> Result<bool, String> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(format!("HIL_PCM_ATTEST_V2 `{name}` must be zero or one")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = "HIL_PCM_ATTEST_V2 model_epoch=100 start_before=100 start_after=102 frames=256 rate_hz=250000 exit=1 accepted_refills=50000 sealed_horizon=201124 stop_before=201120 stop_after=201122 stop_ok=1 rewrite_before=201123 rewrite_after=201125 rewrite_ok=1 owner_state=8 owner_fault=0 safe_reclaimed=0 marker_code=1";

    #[test]
    fn parses_one_ordered_numeric_attestation_behind_monitor_prefixes() {
        let source = format!("[INFO  12.345] {VALID}\r\nHIL_RESULT capture complete\n");
        assert_eq!(
            parse(&source).unwrap(),
            Some(TinyBeePcmSoftwareAttestation {
                model_epoch: 100,
                start_before: 100,
                start_after: 102,
                frames: 256,
                rate_hz: 250_000,
                exit: 1,
                accepted_refills: 50_000,
                sealed_horizon: 201_124,
                stop_before: 201_120,
                stop_after: 201_122,
                stop_ok: true,
                rewrite_before: 201_123,
                rewrite_after: 201_125,
                rewrite_ok: true,
                owner_state: 8,
                owner_fault: false,
                safe_reclaimed: false,
                marker_code: 1,
            })
        );
    }

    #[test]
    fn missing_attestation_is_an_explicit_start_failure_shape() {
        assert_eq!(
            parse("HIL_ABORT circular safe transfer did not start\n"),
            Ok(None)
        );
    }

    #[test]
    fn duplicate_reordered_noncanonical_and_out_of_range_fields_reject() {
        for (source, expected) in [
            (format!("{VALID}\n{VALID}\n"), "more than one"),
            (
                VALID.replace(
                    "model_epoch=100 start_before=100",
                    "start_before=100 model_epoch=100",
                ),
                "expected `model_epoch`",
            ),
            (VALID.replace("stop_ok=1", "stop_ok=01"), "not a canonical"),
            (VALID.replace("rewrite_ok=1", "rewrite_ok=2"), "zero or one"),
            (
                VALID.replace("owner_state=8", "owner_state=11"),
                "within 1..=10",
            ),
            (format!("{VALID} unexpected=1"), "more than 18 fields"),
        ] {
            assert!(parse(&source).unwrap_err().contains(expected));
        }
    }
}
