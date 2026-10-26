# 以 `base:` 明確控制零命中行為

狀態：**提案，未實作、未採納**。日期：2026-10-21。
目前仍使用 [I34 保留的預設政策](../implementation/case-evaluation-unification.md)；
以下語法不可直接當成今日可執行的 `.lang`。

## 建議規則

讓 `base:` 成為「沒有普通分支命中時」唯一的控制位置。普通分支的判定或結果發生錯誤，
立即報錯，永遠不會進入 base。命中但結果不合型別也不會重新挑選。

| 寫法 | 提議的統一行為 |
|---|---|
| `base: <expression>` | 求值該 expression；結果必須符合此位置的預期型別 |
| 空白 `base:` | 保留進入此 case 時的基底；不創造原本不存在的值 |
| `base: error "原因"` | 主動回報錯誤；`error` 是本提案的新語法 |
| 沒寫 `base` 且零命中 | 統一回報 `CASE_NO_MATCH`；不再隱式使用外部基底 |

`base` 最多一個、必須最後。巢狀 case 各自遵守這張表；內層錯誤不會轉到外層 base。
缺省 base 不等同空白 base：前者表示沒有提供預設行為，後者明確要求保留基底。

```lang
# 提案：未匹配時保留目前 Sign。
case:
    $self.syn.number == plural:
        Plural(item: {$self})
    base:

# 提案：未匹配時明確拒絕。
case:
    $self.syn.number == plural:
        Plural(item: {$self})
    base: error "number 沒有對應的形式"
```

第一個例子的空白 base 已是現有語法；第二個例子的 `error` 尚未實作。
也可將 base 寫成已支援的 `base: SomeSign(...)`，明確指定預設結果。

## 基底的型別

「保留」是同一個操作，能否成功由該位置是否已有合法基底決定：

| 位置 | 空白 base 可以保留的內容 |
|---|---|
| Sign／context | 進入 case 時的 Sign；context 相當於不增加 fragment |
| feature | 既有值，包含未定案候選集合；無值時報錯 |
| role | 既有 binding；optional／未飽和的合法缺席仍由 role 型別契約決定 |
| phon | 已有的 deep/default phon |
| Paradigm | 本身沒有預設的成員 Sign，因此空白 base 仍不合法；應指定成員或明確報錯 |

不將 `$self` 自動當成 Paradigm 的預設成員，因為呼叫者不一定是其成員，甚至可能只是
一組 feature context。這個限制來自「沒有可保留的基底」，不是另一套 branch 規則。

## `when` 如何一致

若目標是連 default 關鍵字都統一，後續可讓 `when` 也使用最後的 `base:`：
所有普通條件仍讀同一份 frozen snapshot；至少一支命中就合併所有命中 fragment；
零命中才執行 base，缺省則 `CASE_NO_MATCH`。
空白 base 表示保持現值，非空 base 只接受此 context 允許的 fragment 或明確 error。

舊 `when ... else:` 需先提供遷移診斷／機械改寫為 `base:`。本次不改 `else`，
也不改 `when` 現有的零命中無操作行為。

## 與目前行為的差異

| 情況 | 目前 | 提案 |
|---|---|---|
| 有明確 base 結果 | 零命中時求值 base | 相同 |
| 空白 base | 使用此位置的基底 | 相同，依基底是否存在判定 |
| Sign／feature／role／phon 沒有 base、零命中 | 依位置保留現值或報缺值 | 一律 `CASE_NO_MATCH` |
| Paradigm 缺 base | 解析錯誤 | 可省略，但零命中時 `CASE_NO_MATCH`；可另做覆蓋警告 |
| 主動拒絕預設情況 | 沒有通用語法 | `base: error "原因"` |
| `when` 的零命中分支 | `else:`；可省略而保持現值 | `base:`；省略而零命中則報錯 |
| 命中結果失敗 | I34 起直接報錯 | 相同，base 不接收錯誤 |

此方案含有相容性變更，必須另外授權後實作。遷移工具應在原本依賴隱式保留的位置補上
空白 `base:`，並將 `when` 的 `else:` 改名；不能僅修改 runtime 讓舊文法靜默改變。

實作時沿用同一 `TypedCase` 與 I34 evaluator：由明確 base branch 控制預設動作，
新增的 error 結果具有可套入任意預期型別的失敗語義；同時更新 parser、型別檢查、
printer、identity／四原語、診斷及 cache version。應以各結果型別及三種入口的同一矩陣驗證。
