/* 範例 8.4 — 補償性延長
   coda 脫落 → 空莫拉存活(keep-empty,lazy reparse)→ dominate 向下重掛 → 長元音
   weight-by-position 由 Parse 宣告承擔(D24 子集;步驟 7 取代測試手造) */

Symbol a
Symbol k

Class vowel {a}
Class cons {k}

Parse mora: @vowel | @vowel :: @cons
Parse syllable: @cons? :: @vowel :: @cons?

coda-loss: @coda => * / @vowel _ .
repair-mora: dominate <mora>&empty -> @vowel leftward
