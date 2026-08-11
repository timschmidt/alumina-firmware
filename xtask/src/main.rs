use std::collections::BTreeSet;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitCode};

use alumina_board::{BoardPackage, BusKind, DeviceRoute, OwnerDomain, ResourceId, SafeValue};
use alumina_capability::{calculate_identity, verify_declared_identity};

mod hil_record;

#[derive(Clone, Debug, Eq, PartialEq)]
struct Board {
    source: PathBuf,
    id: String,
    display_name: String,
    revision: String,
    chip: String,
    target: String,
    cores: u8,
    flash_bytes: usize,
    psram_bytes: usize,
    service_core: u8,
    realtime_core: u8,
    qualification: String,
    implementation: String,
    hardware_available: bool,
    firmware_feature: Option<String>,
    armable: bool,
}

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: Vec<String>) -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("xtask must be inside the repository")?;
    let boards = load_boards(root)?;

    match arguments.as_slice() {
        [group, command] if group == "board" && command == "list" => {
            for board in &boards {
                println!(
                    "{:<22} {:<9} cores={} {:<10} {}",
                    board.id, board.chip, board.cores, board.qualification, board.implementation
                );
            }
            Ok(())
        }
        [group, command, id] if group == "board" && command == "check" => {
            let board = find_board(&boards, id)?;
            validate_board(board)?;
            println!("{}: valid ({})", board.id, board.source.display());
            Ok(())
        }
        [command, flag, id] if command == "capabilities" && flag == "--board" => {
            let board = find_board(&boards, id)?;
            validate_board(board)?;
            print_capabilities(board, false);
            Ok(())
        }
        [command, flag, id, json]
            if command == "capabilities" && flag == "--board" && json == "--json" =>
        {
            let board = find_board(&boards, id)?;
            validate_board(board)?;
            print_capabilities(board, true);
            Ok(())
        }
        [command, flag, id] if (command == "check" || command == "build") && flag == "--board" => {
            let board = find_board(&boards, id)?;
            run_board_cargo(root, board, command, None)
        }
        [command, flag, id, profile_flag, profile]
            if command == "build" && flag == "--board" && profile_flag == "--profile" =>
        {
            let board = find_board(&boards, id)?;
            run_board_cargo(root, board, command, Some(profile))
        }
        [group, command] if group == "hil" && command == "list" => {
            println!("mks-tinybee-pcm-short-safe  mks-tinybee-v1  build-only, disconnected-load");
            Ok(())
        }
        [group, command, id] if group == "hil" && command == "build" => {
            run_hil_build(root, &boards, id)
        }
        [group, command, path] if group == "hil" && command == "validate-record" => {
            let summary = hil_record::validate(root, Path::new(path))?;
            println!(
                "HIL record {}: {} ({}, captured={} ns, admitted={} ns)",
                summary.run_id,
                summary.disposition,
                summary.wifi_condition,
                summary.captured_edge_spread_ns,
                summary.predicted_max_edge_spread_ns
            );
            Ok(())
        }
        [] => {
            print_help();
            Ok(())
        }
        [single] if single == "help" || single == "--help" => {
            print_help();
            Ok(())
        }
        _ => Err("unknown command; run `cargo xtask help`".to_owned()),
    }
}

fn print_help() {
    println!("aluminafw repository tasks");
    println!();
    println!("  cargo xtask board list");
    println!("  cargo xtask board check <board-id>");
    println!("  cargo xtask capabilities --board <board-id> [--json]");
    println!("  cargo xtask check --board <board-id>");
    println!("  cargo xtask build --board <board-id> [--profile <name>]");
    println!("  cargo xtask hil list");
    println!("  cargo xtask hil build mks-tinybee-pcm-short-safe");
    println!("  cargo xtask hil validate-record <repository-relative-record.toml>");
}

fn repository_registry(root: &Path) -> PathBuf {
    root.join("boards/registry.toml")
}

fn load_boards(root: &Path) -> Result<Vec<Board>, String> {
    let registry_path = repository_registry(root);
    let registry = fs::read_to_string(&registry_path)
        .map_err(|error| format!("cannot read {}: {error}", registry_path.display()))?;
    let mut paths = Vec::new();
    for (line_index, raw_line) in registry.lines().enumerate() {
        let line = without_comment(raw_line).trim();
        if let Some(value) = line.strip_prefix("path") {
            let value = value.trim_start().strip_prefix('=').ok_or_else(|| {
                format!(
                    "{}:{}: expected `=`",
                    registry_path.display(),
                    line_index + 1
                )
            })?;
            paths.push(parse_string(value, &registry_path, line_index + 1)?);
        }
    }
    if paths.is_empty() {
        return Err(format!(
            "{} contains no board paths",
            registry_path.display()
        ));
    }

    let mut boards = Vec::with_capacity(paths.len());
    for relative in paths {
        let source = root.join("boards").join(relative).join("board.toml");
        boards.push(parse_board(&source)?);
    }

    let mut ids = BTreeSet::new();
    for board in &boards {
        validate_board(board)?;
        if !ids.insert(board.id.clone()) {
            return Err(format!("duplicate board id `{}`", board.id));
        }
    }
    boards.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(boards)
}

fn parse_board(path: &Path) -> Result<Board, String> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;

    Ok(Board {
        source: path.to_owned(),
        id: required_string(&source, path, "id")?,
        display_name: required_string(&source, path, "display_name")?,
        revision: required_string(&source, path, "revision")?,
        chip: required_string(&source, path, "chip")?,
        target: required_string(&source, path, "target")?,
        cores: required_integer(&source, path, "cores")?,
        flash_bytes: required_usize(&source, path, "flash_bytes")?,
        psram_bytes: required_usize(&source, path, "psram_bytes")?,
        service_core: required_integer(&source, path, "service_core")?,
        realtime_core: required_integer(&source, path, "realtime_core")?,
        qualification: required_string(&source, path, "qualification")?,
        implementation: required_string(&source, path, "implementation")?,
        hardware_available: required_bool(&source, path, "hardware_available")?,
        firmware_feature: optional_string(&source, path, "firmware_feature")?,
        armable: required_bool(&source, path, "armable")?,
    })
}

fn required_value<'a>(source: &'a str, path: &Path, key: &str) -> Result<(&'a str, usize), String> {
    for (line_index, raw_line) in source.lines().enumerate() {
        let line = without_comment(raw_line).trim();
        let Some((found_key, value)) = line.split_once('=') else {
            continue;
        };
        if found_key.trim() == key {
            return Ok((value.trim(), line_index + 1));
        }
    }
    Err(format!("{}: missing `{key}`", path.display()))
}

fn required_string(source: &str, path: &Path, key: &str) -> Result<String, String> {
    let (value, line) = required_value(source, path, key)?;
    parse_string(value, path, line)
}

fn optional_string(source: &str, path: &Path, key: &str) -> Result<Option<String>, String> {
    match optional_value(source, key) {
        Some((value, line)) => parse_string(value, path, line).map(Some),
        None => Ok(None),
    }
}

fn optional_value<'a>(source: &'a str, key: &str) -> Option<(&'a str, usize)> {
    source
        .lines()
        .enumerate()
        .find_map(|(line_index, raw_line)| {
            let line = without_comment(raw_line).trim();
            let (found_key, value) = line.split_once('=')?;
            (found_key.trim() == key).then_some((value.trim(), line_index + 1))
        })
}

fn required_integer(source: &str, path: &Path, key: &str) -> Result<u8, String> {
    let (value, line) = required_value(source, path, key)?;
    value
        .parse()
        .map_err(|error| format!("{}:{line}: invalid `{key}`: {error}", path.display()))
}

fn required_usize(source: &str, path: &Path, key: &str) -> Result<usize, String> {
    let (value, line) = required_value(source, path, key)?;
    value
        .parse()
        .map_err(|error| format!("{}:{line}: invalid `{key}`: {error}", path.display()))
}

fn required_bool(source: &str, path: &Path, key: &str) -> Result<bool, String> {
    let (value, line) = required_value(source, path, key)?;
    value
        .parse()
        .map_err(|error| format!("{}:{line}: invalid `{key}`: {error}", path.display()))
}

fn parse_string(value: &str, path: &Path, line: usize) -> Result<String, String> {
    let value = value.trim();
    if value.len() < 2 || !value.starts_with('"') || !value.ends_with('"') {
        return Err(format!(
            "{}:{line}: expected a simple quoted string",
            path.display()
        ));
    }
    Ok(value[1..value.len() - 1].to_owned())
}

fn without_comment(line: &str) -> &str {
    line.split_once('#').map_or(line, |(before, _)| before)
}

fn validate_board(board: &Board) -> Result<(), String> {
    if board.id.is_empty() || board.display_name.is_empty() || board.revision.is_empty() {
        return Err(format!(
            "{}: identity fields cannot be empty",
            board.source.display()
        ));
    }
    if board.cores < 2 {
        return Err(format!(
            "{}: `{}` has {} core(s); aluminafw requires two",
            board.source.display(),
            board.id,
            board.cores
        ));
    }
    if board.service_core == board.realtime_core
        || board.service_core >= board.cores
        || board.realtime_core >= board.cores
    {
        return Err(format!(
            "{}: invalid service/realtime core assignment {}/{} for {} cores",
            board.source.display(),
            board.service_core,
            board.realtime_core,
            board.cores
        ));
    }
    let expected_target = match board.chip.as_str() {
        "esp32" => "xtensa-esp32-none-elf",
        "esp32s3" => "xtensa-esp32s3-none-elf",
        other => {
            return Err(format!(
                "{}: unsupported chip `{other}`",
                board.source.display()
            ));
        }
    };
    if board.target != expected_target {
        return Err(format!(
            "{}: chip `{}` requires target `{expected_target}`, found `{}`",
            board.source.display(),
            board.chip,
            board.target
        ));
    }
    if !matches!(
        board.qualification.as_str(),
        "described" | "compiles" | "bench" | "motion-qualified" | "production-qualified"
    ) {
        return Err(format!(
            "{}: invalid qualification `{}`",
            board.source.display(),
            board.qualification
        ));
    }
    if !matches!(
        board.implementation.as_str(),
        "planned" | "late-stub" | "implemented"
    ) {
        return Err(format!(
            "{}: invalid implementation `{}`",
            board.source.display(),
            board.implementation
        ));
    }
    if board.implementation == "implemented" && board.firmware_feature.is_none() {
        return Err(format!(
            "{}: implemented board lacks `firmware_feature`",
            board.source.display()
        ));
    }
    if let Some(feature) = &board.firmware_feature {
        if !feature.starts_with("board-")
            || !feature
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err(format!(
                "{}: invalid firmware feature `{feature}`",
                board.source.display()
            ));
        }
        let package = package_for(&board.id).ok_or_else(|| {
            format!(
                "{}: firmware-selected board lacks a Rust package",
                board.source.display()
            )
        })?;
        package
            .validate()
            .map_err(|error| format!("{}: package error: {error:?}", board.source.display()))?;
        verify_declared_identity(package).map_err(|error| {
            format!(
                "{}: canonical capability identity error: {error:?}",
                board.source.display()
            )
        })?;
        validate_visual_assets(board, package)?;
        if package.board.id != board.id
            || chip_name(package.board.chip) != board.chip
            || package.board.application_cores != board.cores
            || qualification_name(package.board.qualification) != board.qualification
            || package.memory.flash_bytes != board.flash_bytes
            || package.memory.psram_bytes != board.psram_bytes
            || package.cores.service_core != board.service_core
            || package.cores.realtime_core != board.realtime_core
            || package.armable != board.armable
        {
            return Err(format!(
                "{}: board.toml identity/memory/core facts differ from its Rust package",
                board.source.display()
            ));
        }
        let expected_feature = expected_feature_for(&board.id).ok_or_else(|| {
            format!(
                "{}: implemented board lacks a registered firmware feature",
                board.source.display()
            )
        })?;
        if feature != expected_feature {
            return Err(format!(
                "{}: expected firmware feature `{expected_feature}`, found `{feature}`",
                board.source.display()
            ));
        }
    }
    Ok(())
}

fn validate_visual_assets(board: &Board, package: &BoardPackage<'_>) -> Result<(), String> {
    let repository_root = board
        .source
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or_else(|| {
            format!(
                "{}: cannot determine repository root",
                board.source.display()
            )
        })?;
    for visual in package.visuals {
        let relative = Path::new(visual.asset_path);
        if relative.is_absolute()
            || !relative
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
        {
            return Err(format!(
                "{}: visual `{}` has unsafe repository-relative path `{}`",
                board.source.display(),
                visual.id,
                visual.asset_path
            ));
        }
        let asset = repository_root.join(relative);
        if !asset.is_file() {
            return Err(format!(
                "{}: visual `{}` asset does not exist at {}",
                board.source.display(),
                visual.id,
                asset.display()
            ));
        }
    }
    Ok(())
}

fn expected_feature_for(id: &str) -> Option<&'static str> {
    match id {
        board_mks_tinybee::BOARD_ID => Some("board-mks-tinybee"),
        board_t_deck_pro::BOARD_ID => Some("board-t-deck-pro"),
        _ => None,
    }
}

fn package_for(id: &str) -> Option<&'static BoardPackage<'static>> {
    match id {
        board_mks_tinybee::BOARD_ID => Some(&board_mks_tinybee::PACKAGE),
        board_t_deck_pro::BOARD_ID => Some(&board_t_deck_pro::PACKAGE),
        _ => None,
    }
}

fn find_board<'a>(boards: &'a [Board], id: &str) -> Result<&'a Board, String> {
    let canonical = match id {
        "mks-tinybee" => "mks-tinybee-v1",
        other => other,
    };
    boards
        .iter()
        .find(|board| board.id == canonical)
        .ok_or_else(|| format!("unknown board `{id}`"))
}

fn run_board_cargo(
    root: &Path,
    board: &Board,
    action: &str,
    profile: Option<&String>,
) -> Result<(), String> {
    validate_board(board)?;
    let feature = board.firmware_feature.as_ref().ok_or_else(|| {
        format!(
            "board `{}` has no firmware composition root yet (status: {})",
            board.id, board.implementation
        )
    })?;
    let profile = profile.map_or(
        if action == "build" { "release" } else { "dev" },
        String::as_str,
    );
    if !profile
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(format!("invalid Cargo profile `{profile}`"));
    }

    println!(
        "{} {} for {} ({}, feature {})",
        if action == "build" {
            "building"
        } else {
            "checking"
        },
        profile,
        board.id,
        board.target,
        feature
    );
    let mut command = Command::new("cargo");
    command
        .current_dir(root)
        .arg("+esp")
        .arg(action)
        .args(["-p", "alumina-firmware", "--bin", "alumina-firmware"])
        .arg("--no-default-features")
        .args(["--features", feature])
        .args(["--target", &board.target])
        .arg("--locked");
    if profile != "dev" {
        command.args(["--profile", profile]);
    }
    if action == "build" {
        configure_esp_linker_path(&mut command, board)?;
    }
    let status = command
        .status()
        .map_err(|error| format!("failed to start Cargo: {error}"))?;
    if !status.success() {
        return Err(format!(
            "Cargo {action} failed for board `{}` with {status}",
            board.id
        ));
    }
    Ok(())
}

fn run_hil_build(root: &Path, boards: &[Board], id: &str) -> Result<(), String> {
    if id != "mks-tinybee-pcm-short-safe" {
        return Err(format!(
            "unknown HIL fixture `{id}`; run `cargo xtask hil list`"
        ));
    }
    let board = find_board(boards, "mks-tinybee-v1")?;
    validate_board(board)?;
    println!(
        "building release-only safe-image capture for {}; this command never flashes hardware",
        board.id
    );
    println!(
        "the resulting binary still requires all motor and process loads to be physically disconnected"
    );
    let mut command = Command::new("cargo");
    command
        .current_dir(root)
        .arg("+esp")
        .arg("build")
        .args([
            "-p",
            "alumina-firmware",
            "--bin",
            "alumina-hil-mks-tinybee-pcm-short-safe",
        ])
        .arg("--no-default-features")
        .args(["--features", "hil-mks-tinybee-pcm-short-safe"])
        .args(["--target", &board.target])
        .args(["--profile", "release"])
        .args(["--locked", "--offline"]);
    configure_esp_linker_path(&mut command, board)?;
    let status = command
        .status()
        .map_err(|error| format!("failed to start Cargo: {error}"))?;
    if !status.success() {
        return Err(format!("HIL build failed for `{id}` with {status}"));
    }
    println!(
        "artifact: target/{}/release/alumina-hil-mks-tinybee-pcm-short-safe",
        board.target
    );
    Ok(())
}

fn configure_esp_linker_path(command: &mut Command, board: &Board) -> Result<(), String> {
    let linker = match board.chip.as_str() {
        "esp32" => "xtensa-esp32-elf-gcc",
        "esp32s3" => "xtensa-esp32s3-elf-gcc",
        _ => return Err(format!("unsupported ESP linker chip `{}`", board.chip)),
    };
    if command_succeeds(linker, "--version") {
        return Ok(());
    }

    let rustc = Command::new("rustup")
        .args(["which", "--toolchain", "esp", "rustc"])
        .output()
        .map_err(|error| format!("cannot locate the `esp` Rust toolchain: {error}"))?;
    if !rustc.status.success() {
        return Err(
            "cannot locate the `esp` Rust toolchain; install it with espup before building"
                .to_owned(),
        );
    }
    let rustc = String::from_utf8(rustc.stdout)
        .map_err(|_| "rustup returned a non-UTF-8 rustc path".to_owned())?;
    let toolchain = Path::new(rustc.trim())
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "rustup returned an invalid esp rustc path".to_owned())?;
    let compiler_root = toolchain.join("xtensa-esp-elf");
    let mut candidates = fs::read_dir(&compiler_root)
        .map_err(|error| {
            format!(
                "cannot find the espup GCC bundle at {}: {error}",
                compiler_root.display()
            )
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("xtensa-esp-elf/bin"))
        .filter(|path| path.join(linker).is_file())
        .collect::<Vec<_>>();
    candidates.sort();
    let bin = candidates.pop().ok_or_else(|| {
        format!("the espup toolchain contains no `{linker}`; reinstall the Xtensa GCC bundle")
    })?;

    let inherited = env::var_os("PATH").unwrap_or_default();
    let path = env::join_paths(
        core::iter::once(bin.as_os_str().to_owned())
            .chain(env::split_paths(&inherited).map(OsString::from)),
    )
    .map_err(|error| format!("cannot construct ESP linker PATH: {error}"))?;
    command.env("PATH", path);
    Ok(())
}

fn command_succeeds(program: &str, argument: &str) -> bool {
    Command::new(program)
        .arg(argument)
        .status()
        .is_ok_and(|status| status.success())
}

fn print_capabilities(board: &Board, json: bool) {
    let package = package_for(&board.id);
    let calculated_identity = package.and_then(|package| calculate_identity(package).ok());
    if json {
        println!("{{");
        println!("  \"schema\": 1,");
        println!("  \"id\": \"{}\",", json_escape(&board.id));
        println!(
            "  \"display_name\": \"{}\",",
            json_escape(&board.display_name)
        );
        println!("  \"revision\": \"{}\",", json_escape(&board.revision));
        println!("  \"chip\": \"{}\",", board.chip);
        println!("  \"target\": \"{}\",", board.target);
        println!("  \"cores\": {},", board.cores);
        println!("  \"service_core\": {},", board.service_core);
        println!("  \"realtime_core\": {},", board.realtime_core);
        println!("  \"flash_bytes\": {},", board.flash_bytes);
        println!(
            "  \"internal_sram_bytes\": {},",
            package.map_or(0, |package| package.memory.internal_sram_bytes)
        );
        println!("  \"psram_bytes\": {},", board.psram_bytes);
        println!(
            "  \"realtime_psram_allowed\": {},",
            package.is_some_and(|package| package.memory.realtime_psram_allowed)
        );
        println!(
            "  \"capability_digest\": \"{}\",",
            package.map_or_else(
                || "00".repeat(32),
                |package| bytes_hex(&package.board.capability_digest.0)
            )
        );
        println!(
            "  \"calculated_capability_digest\": \"{}\",",
            calculated_identity
                .map_or_else(|| "00".repeat(32), |identity| bytes_hex(&identity.digest.0))
        );
        println!(
            "  \"capability_document_bytes\": {},",
            calculated_identity.map_or(0, |identity| identity.byte_len)
        );
        println!(
            "  \"capability_digest_verified\": {},",
            package
                .zip(calculated_identity)
                .is_some_and(|(package, identity)| {
                    !package.board.capability_digest.is_zero()
                        && package.board.capability_digest == identity.digest
                })
        );
        println!("  \"qualification\": \"{}\",", board.qualification);
        println!("  \"implementation\": \"{}\",", board.implementation);
        println!("  \"hardware_available\": {},", board.hardware_available);
        println!(
            "  \"firmware_feature\": {},",
            board.firmware_feature.as_ref().map_or_else(
                || "null".to_owned(),
                |feature| format!("\"{}\"", json_escape(feature))
            )
        );
        println!("  \"armable\": {},", board.armable);
        println!("  \"resources\": [");
        if let Some(package) = package {
            for (index, resource) in package.board.resources.iter().enumerate() {
                println!("    {{");
                println!("      \"id\": {},", resource_id_json(resource.id));
                println!("      \"owner\": \"{}\",", owner_name(resource.owner));
                println!(
                    "      \"safe_value\": \"{}\",",
                    safe_value_name(resource.safe_value)
                );
                println!("      \"hazardous_output\": {}", resource.hazardous_output);
                println!(
                    "    }}{}",
                    if index + 1 == package.board.resources.len() {
                        ""
                    } else {
                        ","
                    }
                );
            }
        }
        println!("  ],");
        println!("  \"aliases\": [");
        if let Some(package) = package {
            for (index, alias) in package.aliases.iter().enumerate() {
                println!(
                    "    {{\"name\": \"{}\", \"resource\": {}}}{}",
                    json_escape(alias.name),
                    resource_id_json(alias.resource),
                    if index + 1 == package.aliases.len() {
                        ""
                    } else {
                        ","
                    }
                );
            }
        }
        println!("  ],");
        println!("  \"buses\": [");
        if let Some(package) = package {
            for (index, bus) in package.buses.iter().enumerate() {
                println!("    {{");
                println!("      \"resource\": {},", resource_id_json(bus.resource));
                println!("      \"kind\": \"{}\",", bus_kind_name(bus.kind));
                println!("      \"owner\": \"{}\",", owner_name(bus.owner));
                println!(
                    "      \"maximum_frequency_hz\": {},",
                    bus.maximum_frequency_hz
                );
                println!("      \"pins\": [");
                for (pin_index, pin) in bus.pins.iter().enumerate() {
                    println!(
                        "        {}{}",
                        resource_id_json(*pin),
                        if pin_index + 1 == bus.pins.len() {
                            ""
                        } else {
                            ","
                        }
                    );
                }
                println!("      ]");
                println!(
                    "    }}{}",
                    if index + 1 == package.buses.len() {
                        ""
                    } else {
                        ","
                    }
                );
            }
        }
        println!("  ],");
        println!("  \"devices\": [");
        if let Some(package) = package {
            for (index, device) in package.devices.iter().enumerate() {
                println!("    {{");
                println!("      \"resource\": {},", resource_id_json(device.resource));
                println!("      \"owner\": \"{}\",", owner_name(device.owner));
                println!(
                    "      \"bus\": {},",
                    device
                        .bus
                        .map_or_else(|| "null".to_owned(), resource_id_json)
                );
                println!("      \"route\": {},", device_route_json(device.route));
                println!("      \"auxiliary_resources\": [");
                for (auxiliary_index, auxiliary) in device.auxiliary_resources.iter().enumerate() {
                    println!(
                        "        {}{}",
                        resource_id_json(*auxiliary),
                        if auxiliary_index + 1 == device.auxiliary_resources.len() {
                            ""
                        } else {
                            ","
                        }
                    );
                }
                println!("      ],");
                println!("      \"support\": \"{}\"", support_name(device.support));
                println!(
                    "    }}{}",
                    if index + 1 == package.devices.len() {
                        ""
                    } else {
                        ","
                    }
                );
            }
        }
        println!("  ],");
        println!("  \"flash_regions\": [");
        if let Some(package) = package {
            for (index, region) in package.flash_regions.iter().enumerate() {
                println!("    {{");
                println!("      \"name\": \"{}\",", json_escape(region.name));
                println!("      \"offset\": {},", region.offset);
                println!("      \"length\": {},", region.length);
                println!("      \"kind\": \"{}\",", flash_kind_name(region.kind));
                println!(
                    "      \"writable_while_armed\": {},",
                    region.writable_while_armed
                );
                println!("      \"support\": \"{}\"", support_name(region.support));
                println!(
                    "    }}{}",
                    if index + 1 == package.flash_regions.len() {
                        ""
                    } else {
                        ","
                    }
                );
            }
        }
        println!("  ],");
        println!("  \"clocks\": [");
        if let Some(package) = package {
            for (index, clock) in package.clocks.iter().enumerate() {
                println!("    {{");
                println!("      \"name\": \"{}\",", json_escape(clock.name));
                println!("      \"source\": \"{}\",", clock_source_name(clock.source));
                println!("      \"nominal_hz\": {},", clock.nominal_hz);
                println!(
                    "      \"maximum_error_ppm\": {},",
                    clock
                        .maximum_error_ppm
                        .map_or_else(|| "null".to_owned(), |error| error.to_string())
                );
                println!("      \"domain\": \"{}\",", clock_domain_name(clock.domain));
                println!("      \"support\": \"{}\"", support_name(clock.support));
                println!(
                    "    }}{}",
                    if index + 1 == package.clocks.len() {
                        ""
                    } else {
                        ","
                    }
                );
            }
        }
        println!("  ],");
        println!("  \"electrical_constraints\": [");
        if let Some(package) = package {
            for (index, constraint) in package.electrical_constraints.iter().enumerate() {
                println!("    {{");
                println!("      \"id\": \"{}\",", json_escape(constraint.id));
                println!(
                    "      \"kind\": \"{}\",",
                    electrical_constraint_name(constraint.kind)
                );
                println!("      \"resources\": [");
                for (resource_index, resource) in constraint.resources.iter().enumerate() {
                    println!(
                        "        {}{}",
                        resource_id_json(*resource),
                        if resource_index + 1 == constraint.resources.len() {
                            ""
                        } else {
                            ","
                        }
                    );
                }
                println!("      ],");
                println!("      \"note\": \"{}\",", json_escape(constraint.note));
                println!(
                    "      \"support\": \"{}\"",
                    support_name(constraint.support)
                );
                println!(
                    "    }}{}",
                    if index + 1 == package.electrical_constraints.len() {
                        ""
                    } else {
                        ","
                    }
                );
            }
        }
        println!("  ],");
        println!("  \"interrupts\": [");
        if let Some(package) = package {
            for (index, interrupt) in package.interrupts.iter().enumerate() {
                println!("    {{");
                println!("      \"source\": {},", resource_id_json(interrupt.source));
                println!("      \"owner\": \"{}\",", owner_name(interrupt.owner));
                println!(
                    "      \"trigger\": \"{}\",",
                    interrupt_trigger_name(interrupt.trigger)
                );
                println!(
                    "      \"maximum_latency_cycles\": {},",
                    interrupt
                        .maximum_latency_cycles
                        .map_or_else(|| "null".to_owned(), |latency| latency.to_string())
                );
                println!("      \"support\": \"{}\"", support_name(interrupt.support));
                println!(
                    "    }}{}",
                    if index + 1 == package.interrupts.len() {
                        ""
                    } else {
                        ","
                    }
                );
            }
        }
        println!("  ],");
        println!("  \"safe_output_images\": [");
        if let Some(package) = package {
            for (index, safe_image) in package.safe_output_images.iter().enumerate() {
                println!("    {{");
                println!("      \"engine\": {},", safe_image.engine);
                println!("      \"defined_mask\": {},", safe_image.defined_mask);
                println!("      \"safe_bits\": {},", safe_image.safe_bits);
                println!("      \"bench_verified\": {}", safe_image.bench_verified);
                println!(
                    "    }}{}",
                    if index + 1 == package.safe_output_images.len() {
                        ""
                    } else {
                        ","
                    }
                );
            }
        }
        println!("  ],");
        println!("  \"visuals\": [");
        if let Some(package) = package {
            for (index, visual) in package.visuals.iter().enumerate() {
                println!("    {{");
                println!("      \"id\": \"{}\",", json_escape(visual.id));
                println!(
                    "      \"asset_path\": \"{}\",",
                    json_escape(visual.asset_path)
                );
                println!(
                    "      \"media_type\": \"{}\",",
                    json_escape(visual.media_type)
                );
                println!("      \"pixel_width\": {},", visual.pixel_width);
                println!("      \"pixel_height\": {},", visual.pixel_height);
                println!(
                    "      \"asset_digest\": \"{}\",",
                    bytes_hex(&visual.asset_digest.0)
                );
                println!("      \"license\": \"{}\",", json_escape(visual.license));
                println!(
                    "      \"attribution\": \"{}\",",
                    json_escape(visual.attribution)
                );
                println!("      \"hotspots\": [");
                for (hotspot_index, hotspot) in visual.hotspots.iter().enumerate() {
                    println!("        {{");
                    println!("          \"id\": \"{}\",", json_escape(hotspot.id));
                    println!(
                        "          \"resource\": {},",
                        resource_id_json(hotspot.resource)
                    );
                    println!("          \"polygon\": [");
                    for (point_index, point) in hotspot.polygon.iter().enumerate() {
                        println!(
                            "            [{}, {}]{}",
                            point.x,
                            point.y,
                            if point_index + 1 == hotspot.polygon.len() {
                                ""
                            } else {
                                ","
                            }
                        );
                    }
                    println!("          ]");
                    println!(
                        "        }}{}",
                        if hotspot_index + 1 == visual.hotspots.len() {
                            ""
                        } else {
                            ","
                        }
                    );
                }
                println!("      ]");
                println!(
                    "    }}{}",
                    if index + 1 == package.visuals.len() {
                        ""
                    } else {
                        ","
                    }
                );
            }
        }
        println!("  ],");
        println!("  \"hil_requirements\": [");
        if let Some(package) = package {
            for (index, requirement) in package.hil_requirements.iter().enumerate() {
                println!("    {{");
                println!("      \"id\": \"{}\",", json_escape(requirement.id));
                println!("      \"kind\": \"{}\",", hil_kind_name(requirement.kind));
                println!("      \"resources\": [");
                for (resource_index, resource) in requirement.resources.iter().enumerate() {
                    println!(
                        "        {}{}",
                        resource_id_json(*resource),
                        if resource_index + 1 == requirement.resources.len() {
                            ""
                        } else {
                            ","
                        }
                    );
                }
                println!("      ],");
                println!(
                    "      \"required_for\": \"{}\"",
                    qualification_name(requirement.required_for)
                );
                println!(
                    "    }}{}",
                    if index + 1 == package.hil_requirements.len() {
                        ""
                    } else {
                        ","
                    }
                );
            }
        }
        println!("  ]");
        println!("}}");
    } else {
        println!("board: {} ({})", board.display_name, board.id);
        println!("revision: {}", board.revision);
        println!("chip/target: {} / {}", board.chip, board.target);
        println!("application cores: {}", board.cores);
        println!(
            "core domains: service={} realtime={}",
            board.service_core, board.realtime_core
        );
        println!(
            "memory: flash={} internal={} psram={}",
            board.flash_bytes,
            package.map_or(0, |package| package.memory.internal_sram_bytes),
            board.psram_bytes
        );
        println!("qualification: {}", board.qualification);
        println!("implementation: {}", board.implementation);
        println!("hardware available: {}", board.hardware_available);
        if let Some(package) = package {
            println!("armable: {}", package.armable);
            println!("typed resources: {}", package.board.resources.len());
            println!(
                "aliases/buses/devices: {}/{}/{}",
                package.aliases.len(),
                package.buses.len(),
                package.devices.len()
            );
            println!(
                "flash/clocks/constraints/interrupts: {}/{}/{}/{}",
                package.flash_regions.len(),
                package.clocks.len(),
                package.electrical_constraints.len(),
                package.interrupts.len()
            );
            println!(
                "safe-images/visuals/HIL: {}/{}/{}",
                package.safe_output_images.len(),
                package.visuals.len(),
                package.hil_requirements.len()
            );
        }
    }
}

fn owner_name(owner: OwnerDomain) -> &'static str {
    match owner {
        OwnerDomain::Service => "service",
        OwnerDomain::Realtime => "realtime",
    }
}

fn chip_name(chip: alumina_board::Chip) -> &'static str {
    match chip {
        alumina_board::Chip::Esp32 => "esp32",
        alumina_board::Chip::Esp32S3 => "esp32s3",
    }
}

fn qualification_name(qualification: alumina_board::Qualification) -> &'static str {
    match qualification {
        alumina_board::Qualification::Described => "described",
        alumina_board::Qualification::Compiles => "compiles",
        alumina_board::Qualification::Bench => "bench",
        alumina_board::Qualification::MotionQualified => "motion-qualified",
        alumina_board::Qualification::ProductionQualified => "production-qualified",
    }
}

fn safe_value_name(value: SafeValue) -> &'static str {
    match value {
        SafeValue::NotApplicable => "not-applicable",
        SafeValue::HighImpedance => "high-impedance",
        SafeValue::Low => "low",
        SafeValue::High => "high",
        SafeValue::EngineImage => "engine-image",
    }
}

fn flash_kind_name(kind: alumina_board::FlashRegionKind) -> &'static str {
    match kind {
        alumina_board::FlashRegionKind::Bootloader => "bootloader",
        alumina_board::FlashRegionKind::PartitionTable => "partition-table",
        alumina_board::FlashRegionKind::Application => "application",
        alumina_board::FlashRegionKind::Configuration => "configuration",
        alumina_board::FlashRegionKind::WebBundle => "web-bundle",
        alumina_board::FlashRegionKind::UpdateSlot => "update-slot",
        alumina_board::FlashRegionKind::CrashLog => "crash-log",
    }
}

fn clock_source_name(source: alumina_board::ClockSource) -> &'static str {
    match source {
        alumina_board::ClockSource::Crystal => "crystal",
        alumina_board::ClockSource::Pll => "pll",
        alumina_board::ClockSource::PeripheralBus => "peripheral-bus",
        alumina_board::ClockSource::Rtc => "rtc",
        alumina_board::ClockSource::External => "external",
    }
}

fn clock_domain_name(domain: alumina_board::ClockDomain) -> &'static str {
    match domain {
        alumina_board::ClockDomain::Chip => "chip",
        alumina_board::ClockDomain::Service => "service",
        alumina_board::ClockDomain::Realtime => "realtime",
    }
}

fn electrical_constraint_name(kind: alumina_board::ElectricalConstraintKind) -> &'static str {
    match kind {
        alumina_board::ElectricalConstraintKind::InputOnly => "input-only",
        alumina_board::ElectricalConstraintKind::OutputOnly => "output-only",
        alumina_board::ElectricalConstraintKind::BootStrap => "boot-strap",
        alumina_board::ElectricalConstraintKind::SharedRoute => "shared-route",
        alumina_board::ElectricalConstraintKind::ActiveHigh => "active-high",
        alumina_board::ElectricalConstraintKind::ActiveLow => "active-low",
        alumina_board::ElectricalConstraintKind::NotPwm => "not-pwm",
        alumina_board::ElectricalConstraintKind::Logic3v3 => "logic-3v3",
        alumina_board::ElectricalConstraintKind::ResetStateUnverified => "reset-state-unverified",
    }
}

fn interrupt_trigger_name(trigger: alumina_board::InterruptTrigger) -> &'static str {
    match trigger {
        alumina_board::InterruptTrigger::Rising => "rising",
        alumina_board::InterruptTrigger::Falling => "falling",
        alumina_board::InterruptTrigger::AnyEdge => "any-edge",
        alumina_board::InterruptTrigger::LowLevel => "low-level",
        alumina_board::InterruptTrigger::HighLevel => "high-level",
        alumina_board::InterruptTrigger::Configurable => "configurable",
    }
}

fn hil_kind_name(kind: alumina_board::HilKind) -> &'static str {
    match kind {
        alumina_board::HilKind::BoardIdentity => "board-identity",
        alumina_board::HilKind::SafeState => "safe-state",
        alumina_board::HilKind::PeripheralSmoke => "peripheral-smoke",
        alumina_board::HilKind::CoreIsolation => "core-isolation",
        alumina_board::HilKind::Timing => "timing",
        alumina_board::HilKind::FaultInjection => "fault-injection",
        alumina_board::HilKind::VisualReconciliation => "visual-reconciliation",
    }
}

fn bytes_hex(bytes: &[u8]) -> String {
    use core::fmt::Write as _;

    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

fn bus_kind_name(kind: BusKind) -> &'static str {
    match kind {
        BusKind::I2c => "i2c",
        BusKind::Spi => "spi",
        BusKind::Uart => "uart",
    }
}

fn support_name(level: alumina_board::SupportLevel) -> &'static str {
    match level {
        alumina_board::SupportLevel::Described => "described",
        alumina_board::SupportLevel::Compiles => "compiles",
        alumina_board::SupportLevel::Bench => "bench",
        alumina_board::SupportLevel::Qualified => "qualified",
    }
}

fn device_route_json(route: DeviceRoute) -> String {
    match route {
        DeviceRoute::Dedicated => "{\"kind\":\"dedicated\"}".to_owned(),
        DeviceRoute::I2cAddress(address) => {
            format!("{{\"kind\":\"i2c-address\",\"address\":{address}}}")
        }
        DeviceRoute::SpiChipSelect(chip_select) => format!(
            "{{\"kind\":\"spi-chip-select\",\"resource\":{}}}",
            resource_id_json(chip_select)
        ),
        DeviceRoute::Uart => "{\"kind\":\"uart\"}".to_owned(),
    }
}

fn resource_id_json(resource: ResourceId) -> String {
    match resource {
        ResourceId::Gpio(index) => tagged_index("gpio", index),
        ResourceId::I2sOut { engine, bit } => {
            format!("{{\"kind\":\"i2s-out\",\"engine\":{engine},\"bit\":{bit}}}")
        }
        ResourceId::Adc { unit, channel } => {
            format!("{{\"kind\":\"adc\",\"unit\":{unit},\"channel\":{channel}}}")
        }
        ResourceId::Timer { group, index } => {
            format!("{{\"kind\":\"timer\",\"group\":{group},\"index\":{index}}}")
        }
        ResourceId::I2s(index) => tagged_index("i2s", index),
        ResourceId::Rmt(index) => tagged_index("rmt", index),
        ResourceId::TimedOutput { engine, channel } => {
            format!("{{\"kind\":\"timed-output\",\"engine\":{engine},\"channel\":{channel}}}")
        }
        ResourceId::I2c(index) => tagged_index("i2c", index),
        ResourceId::Spi(index) => tagged_index("spi", index),
        ResourceId::Uart(index) => tagged_index("uart", index),
        ResourceId::Pcnt(index) => tagged_index("pcnt", index),
        ResourceId::Dma(index) => tagged_index("dma", index),
        ResourceId::Twai(index) => tagged_index("twai", index),
        ResourceId::Storage(index) => tagged_index("storage", index),
        ResourceId::Radio(index) => tagged_index("radio", index),
        ResourceId::SafetyInput(index) => tagged_index("safety-input", index),
        ResourceId::Device(index) => {
            format!("{{\"kind\":\"device\",\"index\":{index}}}")
        }
    }
}

fn tagged_index(kind: &str, index: u8) -> String {
    format!("{{\"kind\":\"{kind}\",\"index\":{index}}}")
}

fn json_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comment_removal_and_simple_string_parser_are_deterministic() {
        assert_eq!(without_comment("id = \"board\" # note"), "id = \"board\" ");
        assert_eq!(
            parse_string("\"mks-tinybee-v1\"", Path::new("fixture"), 1),
            Ok("mks-tinybee-v1".to_owned())
        );
    }

    #[test]
    fn target_must_match_chip() {
        let board = Board {
            source: PathBuf::from("fixture"),
            id: "fixture".to_owned(),
            display_name: "Fixture".to_owned(),
            revision: "1".to_owned(),
            chip: "esp32".to_owned(),
            target: "xtensa-esp32s3-none-elf".to_owned(),
            cores: 2,
            flash_bytes: 0,
            psram_bytes: 0,
            service_core: 0,
            realtime_core: 1,
            qualification: "described".to_owned(),
            implementation: "planned".to_owned(),
            hardware_available: true,
            firmware_feature: None,
            armable: false,
        };
        assert!(
            validate_board(&board)
                .unwrap_err()
                .contains("requires target")
        );
    }
}
