/* 範例 8.5 — ATR 雙向和諧(D10/D11)
   +ATR 由規則自 [+atrsrc] 核心產生(insert near → dock),再自詞根向兩側擴散;
   within stem 需型態括號(詞條載入層);anchor 依 I13 附註以 mora 等價表達 */

Feature atrsrc(+atrsrc)

Symbol p
Symbol a
Symbol i [+atrsrc]

Class vowel {a, i}

Melody atr {+ATR} anchor mora

atr-source: insert +ATR floating near mora / [+atrsrc] _
dock-atr: dock atr&floating strategy nearest
atr-spread: spread +ATR bidirectional within stem on-conflict stop
