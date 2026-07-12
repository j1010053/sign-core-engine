/* 範例 8.5(docs/03 §8.5 / docs/02 §12)— ATR 雙向和諧(D10/D11)
   由詞根向兩側,先到先佔;within stem 需型態括號(測試端注入);
   anchor 依 I13 附註以 mora 等價表達(自然類 anchor 延後) */

Symbol p
Symbol a

Class vowel {a}

Melody atr {+ATR} anchor mora

atr-spread: spread +ATR bidirectional within stem on-conflict stop
