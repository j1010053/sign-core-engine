//! 編譯服務:`CompiledSystem` 的生命週期與記憶體快取(擁有者裁定 2026-08-04)。
//!
//! # 分工
//!
//! ```text
//! conlang-language     純函數產生 CompiledSystem      ← 唯一的編譯者
//! conlang-app          生命週期 + 記憶體快取           ← 本模組
//! conlang-query        **不編譯、不快取**              ← 只吃現成的 &CompiledSystem
//! conlang-persistence  MVP **不落盤** compiled cache
//! ```
//!
//! 為什麼快取住這裡而不是 query:`query` 是純函數層(§4 可攜性、wasm 綠),
//! 快取是狀態。把狀態塞進純的那半,整個純度論證就垮了。
//!
//! # 快取性質:lazy、memory-only、可丟棄
//!
//! - **lazy**:沒人問就不編譯;
//! - **memory-only**:不落盤(MVP);
//! - **可丟棄**:任何時候 [`clear`](CompileService::clear) 都不影響正確性,
//!   只是下次要重編。這與 P8「Compiled Grammar 是可丟棄重算的編譯產物」一致。
//!
//! # 鍵必須涵蓋**全部**編譯輸入
//!
//! 漏掉任何一項,引擎或套件變動後就會沿用舊的編譯結果——而那是**靜默**的錯。
//! 四項:
//!
//! | 項 | 為什麼 |
//! |---|---|
//! | Language document 內容 | 顯然 |
//! | identity manifest | 同一份源文字配不同 identity ⇒ 不同 sign id ⇒ 不同編譯產物 |
//! | library lock | 載了哪些套件、各是什麼內容(`std:core` 換了 ontology 就全變) |
//! | **compiler semantics version** | 引擎自己改了 compile 語意 |
//!
//! 最後一項是 `conlang_language::COMPILER_SEMANTICS_VERSION`——它是**約定不是
//! 機制**(該檔已誠實記明);把它放進鍵至少讓「有 bump」這件事真的生效。

use conlang_changeset::{identity_manifest_digest, library_lock_digest_with_packages};
use conlang_language::{
    compile_document_with_packages, compile_with_libraries_ref, sha256_hex, CompileSystemError,
    CompiledSystem, LanguageDocument, LibraryId, LibrarySpec, ResolvedPackages,
    COMPILER_SEMANTICS_VERSION,
};
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::cache::ContentDigest;

/// 一次編譯的**完整**輸入指紋。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CompileKey {
    pub document: ContentDigest,
    pub identities: String,
    pub library_lock: String,
    /// Complete resolver-visible export index. Unselected packages can still
    /// affect dependency diagnostics and the compiled ontology registry.
    pub available_exports: String,
    /// `&'static str` 但存成 `String`——鍵要能獨立於編譯期常數存在。
    pub semantics: String,
}

impl CompileKey {
    pub fn of(
        document: &LanguageDocument,
        libraries: &LibrarySpec,
    ) -> Result<CompileKey, CompileServiceError> {
        let catalog = conlang_language::library::embedded_catalog()
            .map_err(|error| CompileServiceError::Digest(error.to_string()))?;
        let packages = catalog
            .resolve_legacy(libraries)
            .map_err(|error| CompileServiceError::Digest(error.to_string()))?;
        Ok(CompileKey {
            document: ContentDigest::of(document),
            identities: identity_manifest_digest(document)
                .map_err(|error| CompileServiceError::Digest(error.to_string()))?,
            library_lock: library_lock_digest_with_packages(&packages),
            available_exports: export_index_digest(packages.available_exports()),
            semantics: COMPILER_SEMANTICS_VERSION.to_owned(),
        })
    }

    /// Build a cache key from one already-resolved package snapshot.
    ///
    /// Unlike [`Self::of`], this path performs no catalog lookup. The lock
    /// digest therefore describes exactly the package bytes that compilation
    /// will consume, including host-provided packages unavailable to the
    /// embedded catalog.
    pub fn of_with_packages(
        document: &LanguageDocument,
        packages: &ResolvedPackages,
    ) -> Result<CompileKey, CompileServiceError> {
        Ok(CompileKey {
            document: ContentDigest::of(document),
            identities: identity_manifest_digest(document)
                .map_err(|error| CompileServiceError::Digest(error.to_string()))?,
            library_lock: packages.lock_digest(),
            available_exports: export_index_digest(packages.available_exports()),
            semantics: COMPILER_SEMANTICS_VERSION.to_owned(),
        })
    }
}

fn export_index_digest(index: &BTreeMap<String, LibraryId>) -> String {
    let mut content = String::new();
    for (alias, package) in index {
        content.push_str(alias);
        content.push('\t');
        content.push_str(&package.to_string());
        content.push('\n');
    }
    sha256_hex(content.as_bytes())
}

#[derive(Debug, thiserror::Error)]
pub enum CompileServiceError {
    #[error("APP_COMPILE_DIGEST: {0}")]
    Digest(String),
    #[error(transparent)]
    Compile(#[from] CompileSystemError),
}

/// 依 [`CompileKey`] 快取 `CompiledSystem` 的服務。
///
/// 產物包在 `Arc` 裡:`CompiledSystem` 很大,而多個面板(辭典、統計、語意場)
/// 會同時持有同一份。
#[derive(Debug, Default)]
pub struct CompileService {
    entries: BTreeMap<CompileKey, Arc<CompiledSystem>>,
    hits: u64,
    misses: u64,
}

impl CompileService {
    pub fn new() -> CompileService {
        CompileService::default()
    }

    /// 取得(必要時編譯)某份文件在某組套件下的編譯產物。
    ///
    /// **編譯失敗不進快取**——否則下次會拿到一個「快取起來的失敗」,
    /// 而使用者修好文件後仍看到舊錯誤。
    pub fn get(
        &mut self,
        document: &LanguageDocument,
        libraries: &LibrarySpec,
    ) -> Result<Arc<CompiledSystem>, CompileServiceError> {
        let key = CompileKey::of(document, libraries)?;
        if let Some(hit) = self.entries.get(&key) {
            self.hits += 1;
            return Ok(Arc::clone(hit));
        }
        self.misses += 1;
        let compiled = Arc::new(compile_with_libraries_ref(document.language(), libraries)?);
        self.entries.insert(key, Arc::clone(&compiled));
        Ok(compiled)
    }

    /// Compile against one immutable resolver result, without rediscovering
    /// packages through the embedded catalog.
    pub fn get_with_packages(
        &mut self,
        document: &LanguageDocument,
        packages: &ResolvedPackages,
    ) -> Result<Arc<CompiledSystem>, CompileServiceError> {
        let key = CompileKey::of_with_packages(document, packages)?;
        if let Some(hit) = self.entries.get(&key) {
            self.hits += 1;
            return Ok(Arc::clone(hit));
        }
        self.misses += 1;
        let compiled = Arc::new(compile_document_with_packages(document, packages)?);
        self.entries.insert(key, Arc::clone(&compiled));
        Ok(compiled)
    }

    /// 只查不編譯。
    pub fn peek(
        &self,
        document: &LanguageDocument,
        libraries: &LibrarySpec,
    ) -> Result<Option<Arc<CompiledSystem>>, CompileServiceError> {
        Ok(self
            .entries
            .get(&CompileKey::of(document, libraries)?)
            .map(Arc::clone))
    }

    /// Inspect an entry keyed by an already-resolved package snapshot.
    pub fn peek_with_packages(
        &self,
        document: &LanguageDocument,
        packages: &ResolvedPackages,
    ) -> Result<Option<Arc<CompiledSystem>>, CompileServiceError> {
        Ok(self
            .entries
            .get(&CompileKey::of_with_packages(document, packages)?)
            .map(Arc::clone))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn stats(&self) -> (u64, u64) {
        (self.hits, self.misses)
    }

    /// **可丟棄**:清空不影響任何答案,只是下次要重編(P8)。
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
