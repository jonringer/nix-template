//! Lua rockspec parsing and version detection.
//!
//! This module reads Lua rockspec files to:
//! 1. Determine if the package is a library or application
//! 2. Parse Lua version requirements
//! 3. Extract build type from rockspec
//!
//! Rockspec files are Lua scripts that define package metadata using a
//! specific schema. Common fields include:
//! - package: package name
//! - version: package version
//! - build.type: build type ("builtin", "make", "cmake", etc.)
//! - build.modules: for libraries (presence indicates Package variant)
//! - build.install.bin: for applications (presence indicates Application variant)
//! - dependencies: including Lua version requirements

use crate::templates::types::{LuaVariant, LuaVersion};
use log::debug;
use std::path::Path;

const LOG_TARGET: &str = "nix-template::lua_deps";

/// Detect whether a Lua project is a Package (library) or Application (executable).
///
/// Heuristics:
/// - If build.install.bin is present → Application
/// - If build.modules is present → Package
/// - Default to Package if unclear
pub fn detect_lua_variant(rockspec_path: &Path) -> LuaVariant {
    let contents = match std::fs::read_to_string(rockspec_path) {
        Ok(c) => c,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "failed to read rockspec: {}", e
            );
            return LuaVariant::Package; // Default to Package
        }
    };

    // Check for application indicators (build.install.bin)
    if contents.contains("build.install.bin") || contents.contains("install = {") && contents.contains("bin = {") {
        debug!(target: LOG_TARGET, "detected Application variant (build.install.bin found)");
        return LuaVariant::Application;
    }

    // Check for library indicators (build.modules)
    if contents.contains("build.modules") || contents.contains("modules = {") {
        debug!(target: LOG_TARGET, "detected Package variant (build.modules found)");
        return LuaVariant::Package;
    }

    // Default to Package
    debug!(target: LOG_TARGET, "defaulting to Package variant");
    LuaVariant::Package
}

/// Infer Lua version from rockspec dependencies.
///
/// Rockspec files specify Lua version in the dependencies field:
/// ```lua
/// dependencies = {
///   "lua >= 5.1",
///   "lua >= 5.3, < 5.5",
/// }
/// ```
///
/// Returns the detected Lua version, defaulting to Lua 5.4 (latest stable).
pub fn infer_lua_version(rockspec_path: &Path) -> LuaVersion {
    let contents = match std::fs::read_to_string(rockspec_path) {
        Ok(c) => c,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "failed to read rockspec: {}", e
            );
            return LuaVersion::Lua54; // Default to 5.4
        }
    };

    // Look for Lua version in dependencies
    // Common patterns: "lua >= 5.1", "lua >= 5.3, < 5.5", "luajit"
    let contents_lower = contents.to_lowercase();

    // Check for LuaJIT first (most specific)
    if contents_lower.contains("luajit") {
        debug!(target: LOG_TARGET, "detected LuaJIT requirement");
        return LuaVersion::LuaJIT;
    }

    // Check for Lua 5.4
    if contents_lower.contains("lua >= 5.4") || contents_lower.contains("lua = 5.4") {
        debug!(target: LOG_TARGET, "detected Lua 5.4 requirement");
        return LuaVersion::Lua54;
    }

    // Check for Lua 5.3
    if contents_lower.contains("lua >= 5.3") || contents_lower.contains("lua = 5.3") {
        debug!(target: LOG_TARGET, "detected Lua 5.3 requirement");
        return LuaVersion::Lua53;
    }

    // Check for Lua 5.2
    if contents_lower.contains("lua >= 5.2") || contents_lower.contains("lua = 5.2") {
        debug!(target: LOG_TARGET, "detected Lua 5.2 requirement");
        return LuaVersion::Lua52;
    }

    // Check for Lua 5.1
    if contents_lower.contains("lua >= 5.1") || contents_lower.contains("lua = 5.1") {
        debug!(target: LOG_TARGET, "detected Lua 5.1 requirement");
        return LuaVersion::Lua51;
    }

    // Default to Lua 5.4 (latest stable)
    debug!(target: LOG_TARGET, "no specific Lua version found, defaulting to 5.4");
    LuaVersion::Lua54
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_detect_application_variant() {
        let temp_dir = TempDir::new().unwrap();
        let rockspec = temp_dir.path().join("myapp-1.0-1.rockspec");
        fs::write(
            &rockspec,
            r#"
package = "myapp"
version = "1.0-1"
build = {
  type = "builtin",
  install = {
    bin = {
      myapp = "src/main.lua"
    }
  }
}
"#,
        )
        .unwrap();

        let variant = detect_lua_variant(&rockspec);
        assert_eq!(variant, LuaVariant::Application);
    }

    #[test]
    fn test_detect_package_variant() {
        let temp_dir = TempDir::new().unwrap();
        let rockspec = temp_dir.path().join("mylib-1.0-1.rockspec");
        fs::write(
            &rockspec,
            r#"
package = "mylib"
version = "1.0-1"
build = {
  type = "builtin",
  modules = {
    ["mylib"] = "src/mylib.lua",
    ["mylib.utils"] = "src/utils.lua"
  }
}
"#,
        )
        .unwrap();

        let variant = detect_lua_variant(&rockspec);
        assert_eq!(variant, LuaVariant::Package);
    }

    #[test]
    fn test_infer_lua_54() {
        let temp_dir = TempDir::new().unwrap();
        let rockspec = temp_dir.path().join("test-1.0-1.rockspec");
        fs::write(
            &rockspec,
            r#"
dependencies = {
  "lua >= 5.4"
}
"#,
        )
        .unwrap();

        let version = infer_lua_version(&rockspec);
        assert_eq!(version, LuaVersion::Lua54);
    }

    #[test]
    fn test_infer_lua_51() {
        let temp_dir = TempDir::new().unwrap();
        let rockspec = temp_dir.path().join("test-1.0-1.rockspec");
        fs::write(
            &rockspec,
            r#"
dependencies = {
  "lua >= 5.1"
}
"#,
        )
        .unwrap();

        let version = infer_lua_version(&rockspec);
        assert_eq!(version, LuaVersion::Lua51);
    }

    #[test]
    fn test_infer_luajit() {
        let temp_dir = TempDir::new().unwrap();
        let rockspec = temp_dir.path().join("test-1.0-1.rockspec");
        fs::write(
            &rockspec,
            r#"
dependencies = {
  "luajit >= 2.0"
}
"#,
        )
        .unwrap();

        let version = infer_lua_version(&rockspec);
        assert_eq!(version, LuaVersion::LuaJIT);
    }

    #[test]
    fn test_default_version() {
        let temp_dir = TempDir::new().unwrap();
        let rockspec = temp_dir.path().join("test-1.0-1.rockspec");
        fs::write(
            &rockspec,
            r#"
package = "test"
version = "1.0-1"
"#,
        )
        .unwrap();

        let version = infer_lua_version(&rockspec);
        assert_eq!(version, LuaVersion::Lua54); // Default
    }
}
