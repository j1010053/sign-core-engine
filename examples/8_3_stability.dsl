/* 範例 8.3 — 聲調穩定 + 重新著陸(D6/D14)
   調由 tonogenesis 產生(insert near → dock);詞尾元音脫落 → 調浮游留原位
   → redock 左掛存活莫拉(連調) */

Feature voice(+voice, -voice)

Symbol t [-voice]
Symbol d [+voice]
Symbol a

Class vowel {a}

Melody tone {H, L} anchor mora

tonogenesis:
    insert H floating near mora / onset&[-voice] _
    insert L floating near mora / onset&[+voice] _
dock-tone: dock tone&floating strategy nearest

final-vowel-loss: @vowel => * / _ #
redock: dock tone&floating strategy nearest prefer-left
