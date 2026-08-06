# `conlang` CLI 操作教學 v1

本章用一條完整的工作流走過整個工具:**建專案 → 看詞典 → 演化 → 造詞 → 統計 →
分群 → 旁註 → 環境**。每一節都是可以照打的命令。

> **為什麼先有 CLI 而不是 UI**:CLAUDE.md §0.2 要求「每個階段以測試出口收尾」,
> 而 UI 沒有天然的綠燈。CLI 的整合測試是真綠燈,且 UI 之後呼叫的是**同一組**
> `conlang-command`,不是重寫。
>
> 本檔的每一條命令都由 `crates/cli/tests/tutorial.rs` 實際執行。改教學時要同步
> 改那份測試。

---

## 0. 準備一份起始語言

`.lang` 保存**共時**語言知識(P8):`Symbol`/`Class` 是音系 inventory,
`trait` 是可繼承契約,`sign` 是詞。

<!-- conlang-test: tutorial-source -->
```lang
Symbol k
Symbol a
Symbol t
Symbol u
Symbol s

Class vowel {a, u}

global trait Core:

sign kat:
    belongs Noun
    phon:
        /kat/
    sem:
        senses:
            core = STONE

sign tuk:
    belongs Verb
    phon:
        /tuk/
    sem:
        senses:
            core = CARRY
```

存成 `proto.lang`。

---

## 1. 建專案

<!-- conlang-test: tutorial-commands -->
```sh
conlang init ./myproject --from proto.lang --name 教學語 --namespace proto
```

```
created: ./myproject
root: 3f9c…
```

專案目錄長這樣:

```
myproject/
  format              store 的格式版本
  project.toml        專案宣告 + import 表(哪些套件)
  objects/            內容定址的共用物件
  nodes/<id>/         每個語言快照
```

`project.toml` 記的是**這次實際載入的套件組合**。沒有它的話,開啟時只能猜預設,
而用了 `natural:*` 或 plugin 的專案就開不起來——錯誤訊息還會叫你「加進 import 表」,
但那個表不存在。這就是它存在的理由。

## 2. 開起來看看

```sh
conlang open ./myproject
```

```
project: 教學語
declaration: project.toml
packages: std=4 natural=- plugins=0
nodes: 1
active: 3f9c…
```

`open` 一律停在**第一個 root**(依 id 序,故決定性)。要看別的節點用 `--node`。

## 3. 詞典

```sh
conlang lexicon ./myproject
```

```
2 / 2 entries
  kat              kat          STONE
  tuk              tuk          CARRY
```

左邊的數字是**過濾後**、右邊是**過濾前**。加條件就看得出差別:

```sh
conlang lexicon ./myproject --category Nominal
```

```
1 / 2 entries
  kat              kat          STONE
```

注意 `kat` 宣告的是 `belongs Noun`,而我們用 `Nominal` 篩得到它——
過濾走的是 **ontology 閉包**,不是字串相等。

排序只改順序,**不改入選集合**:

```sh
conlang lexicon ./myproject --sort form
```

## 4. 演化

一條音變就是一次演化,產生一個**新節點**——舊的不會被改掉(節點是 immutable
snapshot,P60/P64)。

```sh
conlang evolve ./myproject --rule "t => k"
```

```
committed: 7a21…
nodes: 2
```

底下發生的事:規則 → `AtomicRewrite::SoundChange` → **四原語**(insert/delete/
update/move)→ 一個 statement → 一份 `.chg` → 掛在新節點的主幹邊上。
所以每一次演化都是**可重放**的,不是就地改寫。

## 5. 造詞

造詞分兩步:**列候選**與**採用**。這是刻意的——P70 把「列舉」與「選擇」分開,
手動模式下引擎只排序,選擇權在你。

候選要有一份**分佈**。分佈有三層(手動 > 導入 > E1 先驗),而 E1 目前沒有實際
資料,所以得自己給手動層:

```
segment	weight
k	3.0
a	2.0
t	1.0
u	1.0
```

存成 `weights.tsv`(欄位以 **Tab** 分隔),然後:

```sh
conlang propose ./myproject --name miku --gloss WATER --category Noun \
    --weights weights.tsv --template CVC --count 5
```

```
5 candidates for "miku"
  [0] /kak/ score=1.000
  [1] /kat/ score=1.000
  ...
```

分數都是 `1.000` —— 這是刻意的。引擎**不定義評分合成公式**(統計先驗 §6.4):
候選已依分佈抽樣,彼此之間無進一步高下可言。要排序就自己寫 Generator。

選一個採用:

```sh
conlang propose ./myproject --name miku --gloss WATER --category Noun \
    --weights weights.tsv --template CVC --count 5 --adopt 0
```

```
adopted [0] /kak/ -> 9d04…
```

採用同樣走 Builder → 四原語 → 新節點。**造出來的詞因此自動可 replay**。

> 想看新詞,要指名那個節點:`conlang lexicon ./myproject --node 9d04…`
> ——`open` 停在 root,而新詞在子節點上。

## 6. 統計

```sh
conlang stats ./myproject --weights weights.tsv
```

```
segmentation: longest-match against --weights keys
note: 報表,非抽樣來源(§6.1)
4 distinct / 6 total
  a        2
  k        2
  t        1
  u        1
```

兩件事要注意:

**① 它是報表,不是先驗。** 統計投影曾被設計成抽樣的第三層,§6.1 把它**移出
抽樣棧**了。所以 `stats` 的輸出**不會**自動變成 `propose` 的分佈——你得顯式給
`--weights`。這是刻意的:拿「這個語言現在長怎樣」當「該造什麼樣的新詞」的依據,
是一個該由使用者做的決定,不是引擎偷偷幫你做。

**② 切分依你給的清單。** 不給 `--weights` 就退回逐字元,而那會把 `t͡ʃ` 這種
多字元音段拆成三個:

```sh
conlang stats ./myproject
```

```
segmentation: per-character(未給 --weights;多字元音段會被拆開)
```

## 7. 方言分群

```sh
conlang groups ./myproject
```

```
measure: exploratory_heuristic_v1 threshold: 0.6
  3f9c…: 3f9c…, 7a21…, 9d04…
```

只改了一條規則、造了一個詞,所以三個節點還高度相通,分在同一群。

`measure:` 那行很重要:分數**永遠帶著是誰算的**。`exploratory_heuristic_v1`
的名字就在說它是**一組可探索用的權宜係數**,不是引擎對「互通度」的主張——
「詞彙差異最傷互通」是語言學判斷,不是引擎事實。

閾值調高就切得碎:

```sh
conlang groups ./myproject --threshold 1.1
```

分群只沿**演化樹的主幹邊**切(`演化圖本體論` §6.2:「方言連續體 = **樹上**一組
互通度高於某閾值的**鄰近點**」)。借入邊與合併邊**不算**世系鄰接——否則「借了
三個詞」會和「同一支方言」變成同一回事。

## 8. 旁註

文化說明、隱喻傾向、使用者語料住在**旁註層**,它**正交於本體**(07 §5c):
不參與 replay、不被 diff、不約束生成。

```sh
conlang annotate ./myproject --path culture.md --set "石頭在此文化中象徵盟約"
conlang annotate ./myproject
```

```
node: 3f9c…
annotations: 1
  culture.md
```

寫旁註**不會**改變詞典或統計——它不是語言內容。

## 9. 環境(State)

每個節點有一份**外部環境**:年代、地理、社會、語言接觸。

```sh
conlang state ./myproject --set-time "約 800–1100" --set-region "河谷北岸"
```

```
node: 3f9c…
time: 約 800–1100
region: 河谷北岸
society: -
contacts: 0
```

State 是**雜湊外**的,而且 **replay 永不讀它**(修補04 增修 A)。它影響的是
「**下一次生成什麼**」——記下一段語言接觸,新造的詞就會偏向鄰語的音素。
但已經固化的節點,重放結果逐位元不變。

這個區分很要緊:如果 replay 會讀 State,那同一份 `.chg` 在不同環境下會產生
不同結果,而三道 digest 就失去意義了。

---

## 附:整個流程的一句話

```
init ──► open ──► lexicon        看
             ├──► evolve         改語言 → 新節點(可 replay)
             ├──► propose        造詞 → 新節點(可 replay)
             ├──► stats/groups   派生視圖(唯讀,不回寫)
             └──► annotate/state 雜湊外中繼資料(不進 replay)
```

三條線的差別是本工具的核心:**改語言的會產生節點且可重放;派生視圖只讀不寫;
雜湊外的中繼資料兩者皆非。**
