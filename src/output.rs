use std::path::Path;

/// Helper to write a generated artifact, refusing to clobber any pre-existing file.
/// Creates parent directories as needed.
pub fn write_new(path: &Path, content: &str, label: &str) {
    if path.exists() {
        eprintln!(
            "Refusing to overwrite existing file: {}",
            path.display()
        );
        std::process::exit(1);
    }
    if let Some(parent) = path.parent() {
        if parent.to_str() != Some("") && !parent.exists() {
            println!("Creating directory: {}", parent.display());
            std::fs::create_dir_all(parent).unwrap_or_else(|_| {
                panic!("Was unable to create directory {}", parent.display())
            });
        }
    }
    std::fs::write(path, content).unwrap_or_else(|_| {
        panic!("Was unable to write to file: {}", path.display())
    });
    println!(
        "Generated {} at {}",
        label,
        path.canonicalize().unwrap().display()
    );
}

/// Write a file, creating parent directories as needed (allows overwriting).
pub fn write_file(path: &Path, content: &str) {
    // ensure directory to file exists
    if let Some(p) = path.parent() {
        // TODO: better way to determine that file will be written PWD
        if p.to_str() != Some("") && !p.exists() {
            println!("Creating directory: {}", p.display());
            std::fs::create_dir_all(p).unwrap_or_else(|_| {
                panic!("Was unable to create directory {}", p.display())
            });
        }
    }
    // write file
    std::fs::write(path, content)
        .unwrap_or_else(|_| panic!("Was unable to write to file: {}", &path.display()));
}
