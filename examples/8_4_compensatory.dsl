/* 範例 8.4(docs/03 §8.4 / docs/02 §12)— 補償性延長
   coda 脫落 → 空莫拉存活(keep-empty,lazy reparse)→ dominate 向下重掛 → 長元音
   weight-by-position 的莫拉由 Parse 宣告承擔(步驟 6+),測試端手動建構 */

Symbol a
Symbol k

Class vowel {a}

coda-loss: @coda => * / @vowel _ .
repair-mora: dominate <mora>&empty -> @vowel leftward
