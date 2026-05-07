//! Template type system and utilities for nix-template.
//!
//! This module provides a hierarchical type system for templates, where each
//! language or framework has its own configuration struct with variant-specific
//! settings.
//!
//! # Examples
//!
//! ```
//! use nix_template::templates::types::{Template, PythonConfig, PythonVariant, PythonFormat};
//!
//! // Parse from CLI string
//! let template: Template = "python_package".parse().unwrap();
//! assert!(template.is_python());
//!
//! // Access Python-specific config
//! if let Some(config) = template.python_config() {
//!     println!("Format: {}", config.format.as_str());
//! }
//! ```

pub mod types;

// Re-export all template types for convenient access
