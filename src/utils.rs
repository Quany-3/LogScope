//! Small I/O utilities shared across CLI, TUI, and report modules.

/// Module identifier used for diagnostics and internal logging.
pub const MODULE_NAME: &str = "utils";

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Write through a temporary file in the destination directory before replacing the target.
pub fn write_file_safely(path: impl AsRef<Path>, content: &str) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    // Atomic write: write to a hidden temp file first, then rename over the target.
    // This avoids leaving a partial file if the process crashes mid-write.
    let file_name = path
        .file_name()
        .context("output path must include a file name")?
        .to_string_lossy();
    let temp_path = path.with_file_name(format!(".{file_name}.{}.tmp", std::process::id()));
    fs::write(&temp_path, content)
        .with_context(|| format!("failed to write temporary file {}", temp_path.display()))?;

    if path.exists() {
        fs::remove_file(path)
            .with_context(|| format!("failed to replace output file {}", path.display()))?;
    }
    if let Err(error) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(error)
            .with_context(|| format!("failed to finalize output file {}", path.display()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::write_file_safely;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn safely_writes_and_replaces_nested_file() {
        let root = temp_path();
        let path = root.join("nested").join("report.md");

        write_file_safely(&path, "first").unwrap();
        write_file_safely(&path, "second").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
        fs::remove_dir_all(root).unwrap();
    }

    fn temp_path() -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("logscope-safe-write-{suffix}"))
    }
}
