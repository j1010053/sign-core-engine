/* 範例 8.3(docs/03 §8.3 / docs/02 §12)— 聲調穩定 + 重新著陸(D6/D14)
   詞尾元音脫落 → 調浮游留原位 → dock 左掛鄰莫拉(連調) */

Symbol t
Symbol p
Symbol a

Class vowel {a}

Melody tone {H, L} anchor mora

final-vowel-loss: @vowel => * / _ #
redock: dock tone&floating strategy nearest prefer-left
