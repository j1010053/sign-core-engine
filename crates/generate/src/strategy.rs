//! 可插拔的三個 Strategy(本體論 §0/§3)。
//!
//! Builder **純協調**:良構、阻擋、衝突都不是它的內臟,而是委派出去的策略
//! ——仿 DSL 的 D28 與 `DistributionProvider` 的既有形狀。規格 §0 的紅線寫得
//! 很直接:「Builder 開始懂語言學」就該拆出去。

use conlang_language::{LanguageDocument, SignDef};

use crate::Need;

#[derive(Debug, thiserror::Error)]
pub enum StrategyError {
    #[error("proposal is not well formed: {0}")]
    Invalid(String),
    /// 折磨 11:`thief` 已佔據該語意位置,`stealer` 被擋。
    #[error("blocked by existing sign {existing:?}: {reason}")]
    Blocked { existing: String, reason: String },
    #[error("unresolved conflict: {0}")]
    Conflict(String),
}

/// 良構檢查。
pub trait ValidateStrategy: std::fmt::Debug {
    fn check(&self, sign: &SignDef, document: &LanguageDocument) -> Result<(), StrategyError>;
}

/// 阻擋:既有 sign 是否已佔住這個位置。
pub trait BlockingStrategy: std::fmt::Debug {
    fn check(
        &self,
        need: &Need,
        sign: &SignDef,
        document: &LanguageDocument,
    ) -> Result<(), StrategyError>;
}

/// 衝突解決。可改寫 sign(例如改名)後放行。
pub trait ResolveStrategy: std::fmt::Debug {
    fn resolve(
        &self,
        sign: SignDef,
        document: &LanguageDocument,
    ) -> Result<SignDef, StrategyError>;
}

/// 三個策略的集合。
#[derive(Debug)]
pub struct Strategies {
    pub validate: Box<dyn ValidateStrategy>,
    pub blocking: Box<dyn BlockingStrategy>,
    pub resolve: Box<dyn ResolveStrategy>,
}

impl Default for Strategies {
    /// 最小可用組合:拒絕重名、不阻擋、不改寫。
    ///
    /// **刻意不預設任何語言學阻擋規則**——阻擋是語言學判斷,該由呼叫端選,
    /// 引擎不替使用者決定「什麼詞擋得住什麼詞」。
    fn default() -> Self {
        Self {
            validate: Box::new(RejectDuplicateName),
            blocking: Box::new(NoBlocking),
            resolve: Box::new(KeepAsIs),
        }
    }
}

/// 重名即不良構——`Language` 的 sign 名在同一文件內唯一。
#[derive(Debug)]
pub struct RejectDuplicateName;

impl ValidateStrategy for RejectDuplicateName {
    fn check(&self, sign: &SignDef, document: &LanguageDocument) -> Result<(), StrategyError> {
        if document.language().sign_named(&sign.name).is_some() {
            return Err(StrategyError::Invalid(format!(
                "sign {:?} already exists",
                sign.name
            )));
        }
        Ok(())
    }
}

/// 不阻擋任何東西。
#[derive(Debug)]
pub struct NoBlocking;

impl BlockingStrategy for NoBlocking {
    fn check(&self, _: &Need, _: &SignDef, _: &LanguageDocument) -> Result<(), StrategyError> {
        Ok(())
    }
}

/// 不改寫。
#[derive(Debug)]
pub struct KeepAsIs;

impl ResolveStrategy for KeepAsIs {
    fn resolve(&self, sign: SignDef, _: &LanguageDocument) -> Result<SignDef, StrategyError> {
        Ok(sign)
    }
}
