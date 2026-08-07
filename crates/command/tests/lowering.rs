//! 步驟 21-2 出口:Command 層的三個主張各有出口。
//!
//! 1. **每個 `LanguageCommand` 都降階為四原語**,且結果進得了 replay(C1);
//! 2. **Command ≠ ChangeSet**:多個 command 可堆成一個 statement 序列,
//!    提交邊界是整份 `.chg`;
//! 3. **View/ProjectData command 不產生任何原語**——型別上就沒有降階路徑。

use conlang_changeset::rewrite::{AdoptSource, AtomicRewrite, DonorScope, RuleHome, ServiceContext};
use conlang_changeset::{
    change_set_prelude, ChangeInterpreter, PrimitiveEdit, ResolvedChangeSet, UnresolvedChangeSet,
};
use conlang_command::{
    discard_last, lower, stage, LanguageCommand, ProjectDataCommand, ViewCommand,
};
use conlang_generate::{Need, NeedOrigin, Proposal, Strategies};
use conlang_language::{LanguageDocument, LibrarySpec};

const BASE: &str = "Symbol k\nSymbol a\nSymbol t\nSymbol u\n\nClass vowel {a, u}\n\n\
global trait Core:\n\nsign old:\n    belongs Noun\n    phon:\n        /kat/\n";

fn document() -> LanguageDocument {
    LanguageDocument::import_new_root(BASE, "cmd:root").expect("base parses")
}

fn need() -> Need {
    Need {
        name: "coined".to_owned(),
        categories: vec!["Noun".to_owned()],
        gloss: Some("THING".to_owned()),
        origin: NeedOrigin::Coined,
    }
}

fn proposal() -> Proposal {
    Proposal {
        phon: "/kut/".to_owned(),
        score: 1.0,
        rationale: "測試用".to_owned(),
    }
}

/// 空的 pending buffer = 只有 prelude 的 `.chg`(合法,statements 為零條)。
fn pending(document: &LanguageDocument) -> ResolvedChangeSet {
    let spec = LibrarySpec::default();
    let prelude = change_set_prelude(document, &spec, "cmd:next").expect("prelude");
    UnresolvedChangeSet::parse(&prelude)
        .expect("parses")
        .resolve(document, &spec)
        .expect("resolves")
}

// ── ① 降階為四原語,且可 replay ────────────────────────────────────────

/// 🔑 造詞 command 降階為四原語,堆成 statement 後 replay 得出新語言。
#[test]
fn adopting_a_proposal_lowers_to_primitives_that_replay() {
    let document = document();
    let need = need();
    let chosen = proposal();
    let edits = lower(
        &LanguageCommand::AdoptProposal {
            need: &need,
            chosen: &chosen,
        },
        &document,
        &Strategies::default(),
        &ServiceContext::offline(),
        &DonorScope::new(),
    )
    .expect("降階");

    assert!(!edits.is_empty(), "造一個詞至少要有一條原語");
    // 前提:降階產物**只有**四原語——型別上已保證,這裡確認確實有 Insert
    assert!(
        edits.iter().any(|e| matches!(e, PrimitiveEdit::Insert { .. })),
        "造詞應含 Insert:{edits:?}"
    );

    let mut buffer = pending(&document);
    assert_eq!(stage(&mut buffer, edits), 0, "第一個 statement 序號為 0");

    let source = replay(&document, &buffer);
    assert!(source.contains("coined"), "replay 後新詞應在:{source}");
    assert!(source.contains("kut"), "底層形應在:{source}");
}

/// 音變 command 走同一條降階路。
#[test]
fn applying_a_rewrite_lowers_through_the_same_entry_point() {
    let document = document();
    let rewrite = AtomicRewrite::SoundChange {
        home: RuleHome::Global("Core".to_owned()),
        body: "t => k".to_owned(),
    };
    let edits = lower(
        &LanguageCommand::ApplyRewrite(&rewrite),
        &document,
        &Strategies::default(),
        &ServiceContext::offline(),
        &DonorScope::new(),
    )
    .expect("降階");

    assert!(!edits.is_empty());
    let mut buffer = pending(&document);
    stage(&mut buffer, edits);
    assert!(replay(&document, &buffer).contains("t => k"));
}

// ── ② Command ≠ ChangeSet:多個 command 併成一份 ────────────────────────

/// 🔑 **兩個 command → 兩個 statement → 一個節點**。
///
/// 判別性:若 command 與 changeset 一對一,這裡會需要兩份 `.chg`、兩個節點,
/// 而「設計期加 50 個詞」就會變成 50 個演化事件。
#[test]
fn several_commands_accumulate_into_one_changeset() {
    let document = document();
    let need = need();
    let chosen = proposal();
    let rewrite = AtomicRewrite::SoundChange {
        home: RuleHome::Global("Core".to_owned()),
        body: "t => k".to_owned(),
    };
    let mut buffer = pending(&document);

    for (index, command) in [
        LanguageCommand::AdoptProposal {
            need: &need,
            chosen: &chosen,
        },
        LanguageCommand::ApplyRewrite(&rewrite),
    ]
    .into_iter()
    .enumerate()
    {
        let edits = lower(
            &command,
            &document,
            &Strategies::default(),
            &ServiceContext::offline(),
            &DonorScope::new(),
        )
        .expect("降階");
        assert_eq!(stage(&mut buffer, edits), index as u64, "序號依序");
    }

    assert_eq!(buffer.statements.len(), 2, "兩個 command,兩個 statement");
    let source = replay(&document, &buffer);
    assert!(source.contains("coined"), "第一個 command 的效果在");
    assert!(source.contains("t => k"), "第二個也在");
}

/// 未提交的編輯可以逐條丟棄——(A) 專案編輯的 undo 就是編輯這份 `.chg`。
#[test]
fn the_pending_buffer_can_drop_its_last_statement() {
    let document = document();
    let need = need();
    let chosen = proposal();
    let mut buffer = pending(&document);
    assert!(discard_last(&mut buffer).is_none(), "空的時候回 None 不是錯誤");

    let edits = lower(
        &LanguageCommand::AdoptProposal {
            need: &need,
            chosen: &chosen,
        },
        &document,
        &Strategies::default(),
        &ServiceContext::offline(),
        &DonorScope::new(),
    )
    .expect("降階");
    stage(&mut buffer, edits);
    assert_eq!(buffer.statements.len(), 1);

    let dropped = discard_last(&mut buffer).expect("丟得掉");
    assert_eq!(dropped.ordinal, 0);
    assert!(buffer.statements.is_empty());

    // 丟掉之後 replay 回到原狀——確認真的沒留下痕跡
    assert!(!replay(&document, &buffer).contains("coined"));
}

/// 零條 statement 的 `.chg` 合法(§5.2:pending buffer 不需要新格式)。
#[test]
fn an_empty_pending_buffer_replays_to_the_unchanged_language() {
    let document = document();
    let buffer = pending(&document);
    assert!(buffer.statements.is_empty());
    let source = replay(&document, &buffer);
    assert!(source.contains("old"), "原有的詞還在");
    assert!(!source.contains("coined"));
}

// ── ③ View / ProjectData 不產生原語 ──────────────────────────────────────

/// 🔑 **型別上就沒有降階路徑。**
///
/// 這條斷言是靠編譯器成立的:`lower` 只接受 `&LanguageCommand`,
/// 把 `ViewCommand` 傳進去**不會編譯**。此處以建構它們並確認它們與
/// `PrimitiveEdit` 無任何關聯來記錄這個意圖。
///
/// 判別性:若哪天有人給 `ViewCommand` 加上 `lower()`,本檔的模組文件與
/// 這個測試的存在會立刻與之矛盾——而 `演化圖本體論` §19 的鐵律
/// 「派生視圖永不回寫資料層」正是靠這條界線。
#[test]
fn view_and_project_data_commands_have_no_lowering_at_all() {
    let view = ViewCommand::AssignGroup {
        view: "political".to_owned(),
        node: "cmd:root".to_owned(),
        group: "bulgarian".to_owned(),
    };
    let label = ViewCommand::LabelGroup {
        view: "political".to_owned(),
        group: "bulgarian".to_owned(),
        label: "保加利亞語群".to_owned(),
    };
    let data = ProjectDataCommand::SetWeight {
        table: "segments".to_owned(),
        key: "k".to_owned(),
        weight: 0.9,
    };

    // 它們是**資料描述**,由 app 寫進 views/ 與 data/;此處只確認可建構、可比較。
    assert_ne!(view, label, "指派與標籤是兩件事(D-f3:身分 vs 顯示)");
    assert_eq!(view.clone(), view);
    assert_eq!(data.clone(), data);

    // 而 pending buffer 完全不受它們影響
    let document = document();
    let mut buffer = pending(&document);
    let before = buffer.statements.len();
    let _ = (&view, &label, &data);
    assert_eq!(buffer.statements.len(), before, "視圖/資料 command 不進 .chg");
    assert!(discard_last(&mut buffer).is_none());
}

fn replay(document: &LanguageDocument, changeset: &ResolvedChangeSet) -> String {
    ChangeInterpreter::new(document.clone(), LibrarySpec::default(), "cmd:next".to_owned())
        .expect("interpreter")
        .run(changeset)
        .expect("replay")
        .document
        .source()
        .to_owned()
}

// ── 注入不是裝飾:呼叫端的策略必須真的被用 ──────────────────────────────

/// 擋掉一切的阻擋策略——只為證明「注入的東西真的走到了 `build`」。
#[derive(Debug)]
struct BlockEverything;

impl conlang_generate::BlockingStrategy for BlockEverything {
    fn check(
        &self,
        _need: &Need,
        sign: &conlang_language::SignDef,
        _document: &LanguageDocument,
    ) -> Result<(), conlang_generate::StrategyError> {
        Err(conlang_generate::StrategyError::Blocked {
            existing: sign.name.clone(),
            reason: "測試用:一律擋下".to_owned(),
        })
    }
}

/// 🔑 **`lower` 必須用呼叫端給的 `Strategies`,不得換成 `default()`。**
///
/// 這條補的是一個活過首輪突變的漏洞:先前所有測試都傳 `Strategies::default()`,
/// 於是「`lower` 忽略參數、自己造一個 default」完全測不出來——而那正是步驟 18
/// 的紅線(三個 Strategy **委派不內建**;引擎不替使用者決定「什麼詞擋得住什麼詞」)。
///
/// 判別性靠一對:同一個 command,注入擋人策略必須失敗、預設策略必須成功。
/// 少了正向那半,「一律失敗」的實作也會綠。
#[test]
fn lowering_uses_the_injected_strategies_rather_than_its_own_defaults() {
    let document = document();
    let need = need();
    let chosen = proposal();
    let command = LanguageCommand::AdoptProposal {
        need: &need,
        chosen: &chosen,
    };

    let blocking = Strategies {
        blocking: Box::new(BlockEverything),
        ..Strategies::default()
    };
    let error = lower(
        &command,
        &document,
        &blocking,
        &ServiceContext::offline(),
        &DonorScope::new(),
    )
    .expect_err("注入的阻擋策略必須生效");
    assert!(
        format!("{error}").contains("一律擋下"),
        "錯誤要來自注入的那個策略:{error}"
    );

    // 正向控制組:換回預設就過——證明上面不是「什麼都失敗」
    lower(
        &command,
        &document,
        &Strategies::default(),
        &ServiceContext::offline(),
        &DonorScope::new(),
    )
    .expect("預設策略下應成功");
}

/// 🔑 **`lower` 必須用呼叫端給的 `DonorScope`,不得換成空的。**
///
/// 與上一條同型的漏洞:先前所有測試都傳 `DonorScope::new()`(空),於是
/// 「`lower` 忽略參數、自己造一個空的」測不出來。借入是**唯一**會讀 donor 的
/// 路徑,不釘住它,`AtomicRewrite::Adopt` 就等於沒接上。
#[test]
fn lowering_uses_the_injected_donor_scope() {
    let document = document();
    let donor = LanguageDocument::import_new_root(
        "sign kaffe:\n    belongs Noun\n    sem:\n        senses:\n            core = COFFEE\n",
        "fr",
    )
    .expect("donor parses");
    let mut donors = DonorScope::new();
    donors.insert("fr", &donor);

    let rewrite = AtomicRewrite::Adopt {
        donor: "fr".to_owned(),
        sign: "kaffe".to_owned(),
        source: AdoptSource::Loan,
    };
    let command = LanguageCommand::ApplyRewrite(&rewrite);

    let edits = lower(
        &command,
        &document,
        &Strategies::default(),
        &ServiceContext::offline(),
        &donors,
    )
    .expect("注入 donor 後應借得進來");
    let mut buffer = pending(&document);
    stage(&mut buffer, edits);
    let source = replay(&document, &buffer);
    assert!(source.contains("kaffe"), "借來的詞應在:{source}");
    assert!(source.contains("COFFEE"), "義項一併帶過來:{source}");

    // 判別對的另一半:**沒給** donor 就必須失敗,而非靜默借不到
    lower(
        &command,
        &document,
        &Strategies::default(),
        &ServiceContext::offline(),
        &DonorScope::new(),
    )
    .expect_err("donor 查無時不得靜默成功");
}
