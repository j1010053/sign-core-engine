/* 範例 8.6 — Bantu 三連發(Scan;D3/D5/D16/D18/D20)
   底層 H 由 tonogenesis 生成(規則自足);(a) 倒數第二音節指派 H
   (b) Meeussen:掃描軸相鄰 H…H 後者變 L (c) 指派 H 到第一個無調莫拉 */

Feature voice(+voice, -voice)

Symbol p [-voice]
Symbol b [+voice]
Symbol a

Class vowel {a}

Melody tone {H, L} anchor mora

tonogenesis: insert H floating near mora / onset&[-voice] _
dock-tone: dock tone&floating strategy nearest

Scan tone along syllable within pword from right:
    associate H -> <syl>[2]

Scan tone along mora within pword from left:
    H => L / H _
    associate H -> mora&Ø[first]
