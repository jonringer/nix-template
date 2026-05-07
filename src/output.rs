use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

/// Attempt to canonicalize a path for display, falling back to the original
/// path if canonicalization fails due to permissions or filesystem limitations.
///
/// This ensures users always see a valid path in success messages, even when
/// canonicalize fails after successfully writing a file.
fn display_path(path: &Path) -> std::path::PathBuf {
    match path.canonicalize() {
        Ok(canonical) => canonical,
        Err(e) => {
            log::debug!(
                "Could not canonicalize path '{}': {}. Using original path.",
                path.display(),
                e
            );
            path.to_path_buf()
        }
    }
}

/// Public wrapper for display_path, for use outside this module.
pub fn display_path_pub(path: &Path) -> std::path::PathBuf {
    display_path(path)
}

/// Helper to write a generated artifact, refusing to clobber any pre-existing file.
/// Creates parent directories as needed.
///
/// Uses atomic create_new to prevent TOCTOU race conditions and symlink attacks.
pub fn write_new(path: &Path, content: &str, label: &str) {
    // Create parent directories first
    if let Some(parent) = path.parent() {
        if parent.to_str() != Some("") && !parent.exists() {
            println!("Creating directory: {}", parent.display());
            std::fs::create_dir_all(parent)
                .unwrap_or_else(|_| panic!("Was unable to create directory {}", parent.display()));
        }
    }

    // Use create_new for atomic check-and-create operation
    // This prevents TOCTOU race conditions and symlink attacks
    match OpenOptions::new()
        .write(true)
        .create_new(true) // Atomic: fails if file exists
        .open(path)
    {
        Ok(mut file) => {
            file.write_all(content.as_bytes()).unwrap_or_else(|e| {
                panic!("Was unable to write to file '{}': {}", path.display(), e)
            });
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            eprintln!("Refusing to overwrite existing file: {}", path.display());
            std::process::exit(1);
        }
        Err(e) => {
            panic!("Was unable to create file '{}': {}", path.display(), e);
        }
    }

    println!("Generated {} at {}", label, display_path(path).display());
}

/// Write a file, creating parent directories as needed (allows overwriting).
pub fn write_file(path: &Path, content: &str) {
    // ensure directory to file exists
    if let Some(p) = path.parent() {
        // TODO: better way to determine that file will be written PWD
        if p.to_str() != Some("") && !p.exists() {
            println!("Creating directory: {}", p.display());
            std::fs::create_dir_all(p)
                .unwrap_or_else(|_| panic!("Was unable to create directory {}", p.display()));
        }
    }
    // write file
    std::fs::write(path, content)
        .unwrap_or_else(|_| panic!("Was unable to write to file: {}", &path.display()));
}
