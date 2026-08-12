use std::{env, path::PathBuf};

struct BoardSelection {
    feature_env: &'static str,
    id: &'static str,
    target: &'static str,
}

const BOARDS: &[BoardSelection] = &[
    BoardSelection {
        feature_env: "CARGO_FEATURE_BOARD_MKS_ESP32_FOC_V1",
        id: "mks-esp32-foc-v1",
        target: "xtensa-esp32-none-elf",
    },
    BoardSelection {
        feature_env: "CARGO_FEATURE_BOARD_MKS_TINYBEE",
        id: "mks-tinybee-v1",
        target: "xtensa-esp32-none-elf",
    },
    BoardSelection {
        feature_env: "CARGO_FEATURE_BOARD_MKS_TINYBEE_4MB",
        id: "mks-tinybee-v1-4mb",
        target: "xtensa-esp32-none-elf",
    },
    BoardSelection {
        feature_env: "CARGO_FEATURE_BOARD_T_DECK_PRO",
        id: "t-deck-pro",
        target: "xtensa-esp32s3-none-elf",
    },
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=ld/alumina-esp32-linkall.x");
    println!("cargo:rerun-if-env-changed=ALUMINA_AP_PASSWORD");
    for board in BOARDS {
        println!("cargo:rerun-if-env-changed={}", board.feature_env);
    }

    let mut selected = BOARDS
        .iter()
        .filter(|board| env::var_os(board.feature_env).is_some());
    let board = selected.next().unwrap_or_else(|| {
        panic!(
            "select exactly one board feature; use `cargo xtask build --board \
             mks-tinybee-v1`, `mks-tinybee-v1-4mb`, `mks-esp32-foc-v1`, or \
             `t-deck-pro`"
        )
    });
    if selected.next().is_some() {
        panic!(
            "multiple board features selected; Alumina images contain exactly one board package"
        );
    }

    let target = env::var("TARGET").expect("Cargo did not provide TARGET");
    if target != board.target {
        panic!(
            "board `{}` requires target `{}`, but Cargo selected `{target}`",
            board.id, board.target
        );
    }
    println!("cargo:rustc-env=ALUMINA_BOARD_ID={}", board.id);

    // Keep the ESP-HAL aggregate section composition last so it composes with
    // target-wide scripts such as defmt.x. esp-hal 1.0's classic-ESP32 pre-init
    // shim uses a range-limited direct call, so that target uses the equivalent
    // local composition which places the shim and its private target together.
    if target == "xtensa-esp32-none-elf" {
        let linker_dir = PathBuf::from(
            env::var_os("CARGO_MANIFEST_DIR").expect("Cargo did not provide CARGO_MANIFEST_DIR"),
        )
        .join("ld");
        println!("cargo:rustc-link-search={}", linker_dir.display());
        println!("cargo:rustc-link-arg=-Talumina-esp32-linkall.x");
    } else {
        println!("cargo:rustc-link-arg=-Tlinkall.x");
    }
}
