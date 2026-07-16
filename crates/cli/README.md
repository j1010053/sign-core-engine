# conlang — 獨立音變演化引擎(CLI)

Autosegmental(自體段)音變 DSL:多層音韻表徵(聲調/鼻化/和諧為一等公民)、
歷時音變規則、莫拉級韻律結構。可獨立使用,不依賴工作台其餘部分(P20)。

## 用法

```
conlang <rules.dsl> <words.txt>            # 詞表 → 詞表′(一行一詞)
conlang --trace <rules.dsl> <words.txt>    # 逐規則推導表(每 commit 一行)+ spell-out
```

## 規則檔速查

```text
/* 註解(可跨行) */
Feature voice(+voice, -voice)          /* 特徵宣告(Lexurgy 對齊形) */
Symbol b [+voice]                      /* 符號 = 具名特徵束 */
Class vowel {a, e, i}
Parse mora: @vowel | @vowel :: @cons   /* weight-by-position 莫拉 */
Parse syllable: @cons? :: @vowel :: @cons?
Prosody mora < syllable < foot < pword /* 可自定韻律域(I14) */
Melody tone {H, M, L} anchor mora      /* 旋律層(自體段) */

rule-name:                             /* 具名規則;stage: stem|word|phrase 可標 */
    [+voice]&onset => [-voice]         /* 音段規則:A => B / C _ D;* = 刪除 */
    insert H floating near mora / onset&[-voice] _
dock-t: dock tone&floating strategy nearest
spr:    spread +nasal rightward blocked-by [-sonorant] within pword
fill-t: fill tone Ø => M within pword
ocp:    merge adjacent-equal

Scan tone along syllable within pword from right:
    associate H -> <syl>[2]            /* 位置定址只在 Scan(D3) */

Spell-out:
    order tone
    empty tone => M
    contour tone:{H L} => falling
```

完整語法:`docs/02_語法規格_v0.3.md`;範例:`examples/8_1`–`8_6`(音變教科書六案)。

## 語意要點

- 每條規則一次 commit:凍結快照上匹配 → 一次寫入(parallel 不自我餵食)。
- 浮游自體段合法且有原位記憶;錨點被刪 → 調浮游(stability)。
- 空莫拉是可修復的暫態(補償性延長);spell-out 是純函數,雙莫拉 → `ː`。

## 驗證

Lexurgy 社群測試集黃金子集 8/8 通過(`crates/dsl/tests/lexurgy_golden.rs`);
範例 8.1–8.6 端到端 insta 快照。授權:暫未定案(I7)。
