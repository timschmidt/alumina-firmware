use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Clone, Debug, Eq, PartialEq)]
struct Board {
    source: PathBuf,
    id: String,
    display_name: String,
    revision: String,
    chip: String,
    target: String,
    cores: u8,
    qualification: String,
    implementation: String,
    hardware_available: bool,
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
            print_capabilities(find_board(&boards, id)?, false);
            Ok(())
        }
        [command, flag, id, json]
            if command == "capabilities" && flag == "--board" && json == "--json" =>
        {
            print_capabilities(find_board(&boards, id)?, true);
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
        qualification: required_string(&source, path, "qualification")?,
        implementation: required_string(&source, path, "implementation")?,
        hardware_available: required_bool(&source, path, "hardware_available")?,
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

fn required_integer(source: &str, path: &Path, key: &str) -> Result<u8, String> {
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
    Ok(())
}

fn find_board<'a>(boards: &'a [Board], id: &str) -> Result<&'a Board, String> {
    boards
        .iter()
        .find(|board| board.id == id)
        .ok_or_else(|| format!("unknown board `{id}`"))
}

fn print_capabilities(board: &Board, json: bool) {
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
        println!("  \"qualification\": \"{}\",", board.qualification);
        println!("  \"implementation\": \"{}\",", board.implementation);
        println!("  \"hardware_available\": {}", board.hardware_available);
        println!("}}");
    } else {
        println!("board: {} ({})", board.display_name, board.id);
        println!("revision: {}", board.revision);
        println!("chip/target: {} / {}", board.chip, board.target);
        println!("application cores: {}", board.cores);
        println!("qualification: {}", board.qualification);
        println!("implementation: {}", board.implementation);
        println!("hardware available: {}", board.hardware_available);
    }
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
            qualification: "described".to_owned(),
            implementation: "planned".to_owned(),
            hardware_available: true,
        };
        assert!(
            validate_board(&board)
                .unwrap_err()
                .contains("requires target")
        );
    }
}
