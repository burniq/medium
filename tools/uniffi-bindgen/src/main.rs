use std::path::PathBuf;

use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use uniffi_bindgen::bindings::{KotlinBindingGenerator, SwiftBindingGenerator};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let udl = args
        .next()
        .context("expected UDL path as the first argument")?;
    let language = args
        .next()
        .context("expected target language as the second argument")?;
    let out_dir = args
        .next()
        .context("expected output directory as the third argument")?;

    let udl = Utf8PathBuf::from_path_buf(PathBuf::from(udl))
        .map_err(|_| anyhow::anyhow!("UDL path must be valid UTF-8"))?;
    let out_dir = Utf8PathBuf::from_path_buf(PathBuf::from(out_dir))
        .map_err(|_| anyhow::anyhow!("output directory path must be valid UTF-8"))?;

    match language.as_str() {
        "swift" => uniffi_bindgen::generate_bindings(
            &udl,
            None,
            SwiftBindingGenerator,
            Some(&out_dir),
            None,
            None,
            false,
        ),
        "kotlin" => uniffi_bindgen::generate_bindings(
            &udl,
            None,
            KotlinBindingGenerator,
            Some(&out_dir),
            None,
            None,
            false,
        ),
        other => anyhow::bail!("unsupported language: {other}"),
    }
    .context("failed to generate UniFFI bindings")?;

    Ok(())
}
