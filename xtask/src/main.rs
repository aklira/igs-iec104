// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Generate `crates/igs-iec104-codec/src/generated/` from the licensed data
//! files of IMPLEMENTATION.md section 3. `cargo xtask codegen` writes them;
//! `cargo xtask codegen --check` fails when the committed files are stale.
//! Inputs are data (type IDs, sizes, cause lists); no standard text is copied.
//! `docs/spec/` is absent in CI, so codegen runs only on a dev machine.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

mod data;
mod emit;

use data::{validate, Formats, Profile};
use emit::{module_txt, profile_txt};

type Fail = Box<dyn std::error::Error>;

fn main() {
    let mut args = std::env::args().skip(1);
    let command = args.next();
    let check = args.any(|a| a == "--check");
    let result = match command.as_deref() {
        Some("codegen") => run(check),
        _ => {
            println!("usage: cargo xtask codegen [--check]");
            Ok(())
        }
    };
    if let Err(e) = result {
        eprintln!("xtask: {e}");
        std::process::exit(1);
    }
}

fn workspace_root() -> Result<PathBuf, Fail> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask must live inside the workspace".into())
}

fn read_yaml<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, Fail> {
    let text = fs::read_to_string(path).map_err(|e| {
        if e.kind() == ErrorKind::NotFound {
            format!(
                "input {} missing: the data files of IMPLEMENTATION.md section 3 \
                 (docs/spec symlink) must be present to run codegen",
                path.display()
            )
        } else {
            e.to_string()
        }
    })?;
    Ok(serde_yaml::from_str(&text)?)
}

fn run(check: bool) -> Result<(), Fail> {
    let root = workspace_root()?;
    let profile: Profile = read_yaml(&root.join("docs/spec/104/data/profile_104.yaml"))?;
    let formats: Formats = read_yaml(&root.join("docs/spec/5-4/data/formats.yaml"))?;
    validate(&profile, &formats)?;

    let out = root.join("crates/igs-iec104-codec/src/generated");
    let texts = [
        ("mod.rs", fmt(&module_txt())),
        ("profile.rs", fmt(&profile_txt(&profile))),
    ];

    if check {
        for (name, expect) in &texts {
            let path = out.join(name);
            let have = fs::read_to_string(&path).map_err(|_| {
                format!(
                    "{} missing: run `cargo xtask codegen` and commit",
                    path.display()
                )
            })?;
            if &have != expect {
                return Err(format!(
                    "{} is stale: run `cargo xtask codegen` and commit",
                    path.display()
                )
                .into());
            }
        }
        println!("codegen: committed files are up to date");
        return Ok(());
    }

    fs::create_dir_all(&out)?;
    for (name, text) in &texts {
        fs::write(out.join(name), text)?;
        println!("wrote {}", out.join(name).display());
    }
    Ok(())
}

fn fmt(text: &str) -> String {
    let dir = std::env::temp_dir().join("igs-iec104-codegen");
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("g.rs");
    let ok = fs::write(&path, text).is_ok()
        && Command::new("rustfmt")
            .arg("--edition=2021")
            .arg(&path)
            .output()
            .is_ok_and(|o| o.status.success());
    let out = if ok {
        fs::read_to_string(&path).unwrap_or_else(|_| text.to_owned())
    } else {
        text.to_owned()
    };
    let _ = fs::remove_dir_all(&dir);
    out
}
