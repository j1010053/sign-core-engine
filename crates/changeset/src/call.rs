//! 層②③④ 的**統一呼叫**降階:`name(位置參數, key: value, …)` → 四原語。
//!
//! 層級**由名字解析決定**,不靠關鍵字。現階段只有 P16 的 12 個內建 Atomic Rewrite
//! 解析得出來(**封閉內建集**——使用者/plugin 不得自行新增);Recipe/Goal
//! (步驟 16–17,可自行撰寫)落地後沿用同一文法與同一個降階入口,不必改 parser。
//!
//! 與 `clone` 同構:呼叫只活在**未解析層**,`resolve` 時就降成 `Vec<PrimitiveEdit>`,
//! `ResolvedChangeSet` 維持 primitive-only(步驟 14 已封板的契約)。

use crate::rewrite::{
    expand, AdoptSource, AtomicRewrite, DonorScope, ReanalysisTarget, RuleHome, ServiceContext,
};
use crate::{PrimitiveEdit, ReplayError};
use conlang_language::{DerivationKind, Language, LanguageDocument, SignDef};

/// 一次呼叫的實參。
pub(crate) struct Call<'a> {
    pub name: &'a str,
    pub positional: Option<&'a str>,
    pub named: &'a [(String, String)],
    pub block: Option<&'a str>,
}

impl Call<'_> {
    fn named(&self, key: &str) -> Option<&str> {
        self.named
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    fn require(&self, key: &str) -> Result<&str, ReplayError> {
        self.named(key).ok_or_else(|| {
            ReplayError::Parse(format!("{}(…) requires the argument {key}:", self.name))
        })
    }

    fn require_positional(&self) -> Result<&str, ReplayError> {
        self.positional.ok_or_else(|| {
            ReplayError::Parse(format!("{}(…) requires a positional target", self.name))
        })
    }

    fn require_f64(&self, key: &str) -> Result<f64, ReplayError> {
        let raw = self.require(key)?;
        raw.parse().map_err(|_| {
            ReplayError::Parse(format!("{}(…) argument {key}: must be a number", self.name))
        })
    }

    fn require_usize(&self, key: &str) -> Result<usize, ReplayError> {
        let raw = self.require(key)?;
        raw.parse().map_err(|_| {
            ReplayError::Parse(format!(
                "{}(…) argument {key}: must be a whole number",
                self.name
            ))
        })
    }

    /// 位置參數是 `sign("x")` 形 selector;取出其中的 sign 名。
    fn require_sign(&self) -> Result<String, ReplayError> {
        sign_name(self.require_positional()?).ok_or_else(|| {
            ReplayError::Parse(format!(
                "{}(…) expects a sign target like sign(\"x\")",
                self.name
            ))
        })
    }

    /// `adopt` 的目標:**`<donor 別名>.sign("<名字>")`**。
    ///
    /// 別名與 selector 之間以第一個 `.` 分隔。左半必須是**裸識別字**——若它含
    /// `(` 或 `"`,代表作者寫的是普通 selector(舊寫法),明確告知需要 donor 前綴,
    /// 而不是含糊地報「解析失敗」。
    fn require_donor_sign(&self) -> Result<(String, String), ReplayError> {
        let target = self.require_positional()?;
        // 沒有 `.`,或左半看起來就是 selector(含 `(` / `"`)⇒ 作者用的是舊寫法。
        // 這是最可能的誤用,故給**指名的訊息**而非泛用的「解析失敗」。
        let missing_alias = || {
            ReplayError::Parse(format!(
                "adopt(…) needs a donor alias before the selector, like fr.sign(\"x\"); got {target:?}"
            ))
        };
        let (alias, selector) = target.split_once('.').ok_or_else(missing_alias)?;
        if alias.is_empty() || alias.contains('(') || alias.contains('"') {
            return Err(missing_alias());
        }
        let name = sign_name(selector).ok_or_else(|| {
            ReplayError::Parse(format!(
                "adopt(…) expects <donor>.sign(\"x\"), got {target:?}"
            ))
        })?;
        Ok((alias.to_owned(), name))
    }

    /// `.lang` block 解析成恰好一個 sign。
    fn require_sign_block(&self) -> Result<SignDef, ReplayError> {
        let block = self.block.ok_or_else(|| {
            ReplayError::Parse(format!("{}(…): requires an indented sign block", self.name))
        })?;
        let language =
            Language::parse(block).map_err(|error| ReplayError::Parse(error.to_string()))?;
        if language.signs.len() != 1 || !language.traits.is_empty() {
            return Err(ReplayError::Parse(format!(
                "{}(…): block must contain exactly one sign",
                self.name
            )));
        }
        Ok(language.signs[0].clone())
    }
}

/// `sign("x")` → `x`。
fn sign_name(selector: &str) -> Option<String> {
    let rest = selector.trim().strip_prefix("sign(")?.strip_suffix(')')?;
    let rest = rest.trim();
    Some(
        rest.strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .unwrap_or(rest)
            .to_owned(),
    )
}

/// 規則居所:`trait("X")` / `sign("X")`(含 `.block[n]` 等尾段一律忽略)。
fn rule_home(value: &str, call: &str) -> Result<RuleHome, ReplayError> {
    let value = value.trim();
    let head = value.split(").").next().unwrap_or(value);
    let head = if head.ends_with(')') {
        head.to_owned()
    } else {
        format!("{head})")
    };
    if let Some(name) = head
        .strip_prefix("trait(")
        .and_then(|r| r.strip_suffix(')'))
    {
        let name = name.trim().trim_matches('"');
        return Ok(RuleHome::Trait(name.to_owned()));
    }
    if let Some(name) = sign_name(&head) {
        return Ok(RuleHome::Sign(name));
    }
    Err(ReplayError::Parse(format!(
        "{call}(…) expects a rule home like trait(\"X\") or sign(\"X\"), got {value:?}"
    )))
}

/// `adopt` 的兩種形式:
///
/// - **A 逐詞指名**:`adopt(fr.sign("eau"), source: loan)`
/// - **B 清單**(語法糖):`adopt(from: fr, source: loan):` + 每行一個名字
///
/// B **語意上就是 A 的批次寫法**——降階後兩者無從分辨(都是 N 個 Insert),
/// 故不新增本體、不需要新的 `AtomicRewrite` 變體。
fn lower_adopt(
    call: &Call<'_>,
    document: &LanguageDocument,
    donors: &DonorScope<'_>,
) -> Result<Vec<PrimitiveEdit>, ReplayError> {
    let source = adopt_source(call.require("source")?)?;
    let (donor, signs) =
        match (call.positional, call.named("from")) {
            // 兩種形式**不得混用**:同時給了會讓「以哪個為準」變成任意的。
            (Some(_), Some(_)) => return Err(ReplayError::Parse(
                "adopt(…): use either <donor>.sign(\"x\") or from: <donor> with a list, not both"
                    .to_owned(),
            )),
            (Some(_), None) => {
                let (donor, sign) = call.require_donor_sign()?;
                (donor, vec![sign])
            }
            (None, Some(alias)) => (alias.to_owned(), adopt_list(call)?),
            (None, None) => {
                return Err(ReplayError::Parse(
                    "adopt(…) requires <donor>.sign(\"x\") or from: <donor>".to_owned(),
                ))
            }
        };
    let mut edits = Vec::new();
    for sign in signs {
        edits.extend(
            expand(
                &AtomicRewrite::Adopt {
                    donor: donor.clone(),
                    sign,
                    source,
                },
                document,
                &ServiceContext::offline(),
                donors,
            )
            .map_err(|error| ReplayError::Parse(error.to_string()))?,
        );
    }
    Ok(edits)
}

/// 清單形的 block:**每行一個 sign 名字**。
///
/// 重複名字硬錯——會借入兩份同名 sign(之後名字唯一性也會擋,但在這裡說離成因近得多)。
///
/// **誠實標記**:空清單的檢查**從表面走不到**——空白 block 在更早的語句解析就被
/// 視為「沒有 block」而擋下。留著是對日後 parser 變動的防線,但目前無法以突變測試觀測。
fn adopt_list(call: &Call<'_>) -> Result<Vec<String>, ReplayError> {
    let block = call.block.ok_or_else(|| {
        ReplayError::Parse("adopt(…) with from: requires an indented list of sign names".to_owned())
    })?;
    let mut names: Vec<String> = Vec::new();
    for line in block.lines() {
        let name = line.trim();
        if name.is_empty() {
            continue;
        }
        // 含空白 ⇒ 多半是誤把 `.lang` 片段(`sign eau:`)寫進來了。
        if name.split_whitespace().count() != 1 {
            return Err(ReplayError::Parse(format!(
                "adopt(…) list takes one sign name per line, got {name:?}"
            )));
        }
        if names.iter().any(|existing| existing == name) {
            return Err(ReplayError::Parse(format!(
                "adopt(…) list repeats {name:?}"
            )));
        }
        names.push(name.to_owned());
    }
    if names.is_empty() {
        return Err(ReplayError::Parse(
            "adopt(…) list must name at least one sign".to_owned(),
        ));
    }
    Ok(names)
}

fn adopt_source(value: &str) -> Result<AdoptSource, ReplayError> {
    match value {
        "loan" => Ok(AdoptSource::Loan),
        "dialect" => Ok(AdoptSource::Dialect),
        "ancestor" => Ok(AdoptSource::Ancestor),
        other => Err(ReplayError::Parse(format!(
            "adopt(…) source: must be loan|dialect|ancestor, got {other:?}"
        ))),
    }
}

/// 把一次呼叫降階為四原語。未知名字 → 明確錯誤(封閉內建集)。
pub(crate) fn lower(
    call: &Call<'_>,
    document: &LanguageDocument,
    donors: &DonorScope<'_>,
) -> Result<Vec<PrimitiveEdit>, ReplayError> {
    // `adopt` 有兩種形式(單詞指名 / 清單),會展開成**多個**原語,故不走單一 rewrite 的
    // 路徑。清單形是**語法糖**——與 `clone` 同構:只活在未解析層,`resolve` 就降成
    // N 個 Insert,`ResolvedChangeSet` 維持 primitive-only(步驟 14 契約)。
    if call.name == "adopt" {
        return lower_adopt(call, document, donors);
    }
    let rewrite = match call.name {
        // ── form ──
        "sound_change" => AtomicRewrite::SoundChange {
            home: rule_home(call.require_positional()?, call.name)?,
            body: call.require("body")?.to_owned(),
        },

        // ── sem ──
        "drift" => AtomicRewrite::Drift {
            sign: call.require_sign()?,
            sense: call.require("sense")?.to_owned(),
            gloss: call.require("gloss")?.to_owned(),
        },
        "derive_sense" => AtomicRewrite::DeriveSense {
            sign: call.require_sign()?,
            from: call.require("from")?.to_owned(),
            name: call.require("name")?.to_owned(),
            gloss: call.require("gloss")?.to_owned(),
            kind: DerivationKind::parse(call.require("kind")?).ok_or_else(|| {
                ReplayError::Parse(
                    "derive_sense(…) kind: must be metaphor|metonymy|narrow|broaden".to_owned(),
                )
            })?,
        },
        "lexicalize_sense" => AtomicRewrite::LexicalizeSense {
            sign: call.require_sign()?,
            edge: call.require_usize("edge")?,
        },

        // ── syn ──
        "reanalyze" => AtomicRewrite::Reanalyze {
            sign: call.require_sign()?,
            target: match call.require("target")? {
                "valence" => ReanalysisTarget::Valence,
                "category" => ReanalysisTarget::Category,
                "slot" => ReanalysisTarget::Slot,
                "boundary" => ReanalysisTarget::Boundary,
                other => {
                    return Err(ReplayError::Parse(format!(
                        "reanalyze(…) target: must be valence|category|slot|boundary, got {other:?}"
                    )))
                }
            },
            to: call.require("to")?.to_owned(),
        },

        // ── usage ──
        "entrench" => AtomicRewrite::Entrench {
            sign: call.require_sign()?,
            delta: call.require_f64("delta")?,
        },
        "attrit" => AtomicRewrite::Attrit {
            sign: call.require_sign()?,
            delta: call.require_f64("delta")?,
        },
        "lexicalize" => AtomicRewrite::Lexicalize {
            sign: call.require_sign_block()?,
        },

        // ── 結構 ──
        "create" => AtomicRewrite::Create {
            sign: call.require_sign_block()?,
        },
        "delete" => AtomicRewrite::Delete {
            selector: call.require_positional()?.to_owned(),
        },
        "split" => AtomicRewrite::Split {
            sign: call.require_sign()?,
            new_name: call.require("name")?.to_owned(),
            // 逗號是參數分隔符,故義項清單以空白分隔。
            senses: call
                .require("senses")?
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
        },
        "merge" => AtomicRewrite::Merge {
            into: call.require_sign()?,
            from: sign_name(call.require("from")?).ok_or_else(|| {
                ReplayError::Parse("merge(…) from: expects sign(\"x\")".to_owned())
            })?,
        },
        "fuse" => AtomicRewrite::Fuse {
            left: call.require_sign()?,
            right: sign_name(call.require("right")?).ok_or_else(|| {
                ReplayError::Parse("fuse(…) right: expects sign(\"x\")".to_owned())
            })?,
            name: call.require("name")?.to_owned(),
            gloss: call.require("gloss")?.to_owned(),
        },

        // ── 接觸 ──
        "adopt" => {
            let (donor, sign) = call.require_donor_sign()?;
            AtomicRewrite::Adopt {
                donor,
                sign,
                source: match call.require("source")? {
                    "loan" => AdoptSource::Loan,
                    "dialect" => AdoptSource::Dialect,
                    "ancestor" => AdoptSource::Ancestor,
                    other => {
                        return Err(ReplayError::Parse(format!(
                            "adopt(…) source: must be loan|dialect|ancestor, got {other:?}"
                        )))
                    }
                },
            }
        }

        // ── 居所 ──
        "fossilize" => AtomicRewrite::Fossilize {
            rule: call.require_positional()?.to_owned(),
            to: rule_home(call.require("to")?, call.name)?,
        },
        "generalize" => AtomicRewrite::Generalize {
            rule: call.require_positional()?.to_owned(),
            to: rule_home(call.require("to")?, call.name)?,
        },

        // 封閉內建集:未知名字不得默默略過。Recipe/Goal 落地後在此加解析分支。
        other => {
            return Err(ReplayError::Parse(format!(
                "unknown rewrite {other:?}: the 12 atomic rewrites are a closed built-in set \
                 (user-defined recipes/goals are steps 16–17)"
            )))
        }
    };
    // P53:`.chg` 降階目前一律離線(無 live 服務);replay 走 History 時由
    // 呼叫端改傳 `ServiceContext::from_history`。
    expand(&rewrite, document, &ServiceContext::offline(), donors)
        .map_err(|error| ReplayError::Parse(error.to_string()))
}
