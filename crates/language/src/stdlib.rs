//! Backward-compatible standard-library facade over [`crate::library`].

use crate::library::{self, LibraryKind, LibrarySpec};
use crate::Language;

pub type StdExportKind = library::LibraryExportKind;
pub type StdExport = library::LibraryExport;
pub type StdPackage = library::LibraryPackage;
pub type StdLoadError = library::LibraryLoadError;

/// Parse and validate enabled embedded standard packages in deterministic
/// priority/name order.
pub fn packages() -> Result<Vec<StdPackage>, StdLoadError> {
    Ok(library::embedded_catalog()?
        .packages()
        .iter()
        .filter(|package| package.id.legacy_kind() == Some(LibraryKind::Std))
        .cloned()
        .collect())
}

/// Resolve an unqualified standard-library export alias.
pub fn resolve_export(alias: &str) -> Result<StdExport, StdLoadError> {
    library::embedded_catalog()?.resolve_export(LibraryKind::Std, alias)
}

/// Load enabled std packages without natural-language or plugin overlays.
pub fn load_default() -> Result<Language, StdLoadError> {
    Ok(library::embedded_catalog()?
        .select(&LibrarySpec::default())?
        .standard)
}
