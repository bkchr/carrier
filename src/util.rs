use std::{fmt::Display, path::PathBuf, result};

use glob;

pub fn glob_for_certificates<T: Display>(
    path: &T,
) -> result::Result<Vec<PathBuf>, glob::PatternError> {
    glob::glob(&format!("{}/*.pem", path)).map(|v| v.filter_map(|v| v.ok()).collect())
}
