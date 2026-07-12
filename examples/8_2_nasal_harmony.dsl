/* 範例 8.2 — 鼻化和諧:私有特徵 + 阻塞(D8/D12)
   +nasal 由規則自鼻音聲母產生(insert near → dock),再擴散 */

Feature sonorant(+sonorant, -sonorant)
Feature nasstop(+nasstop)

Symbol m [+sonorant +nasstop]
Symbol a [+sonorant]
Symbol t [-sonorant]

Class vowel {a}

Melody nasal {+nasal} anchor segment

nasal-source: insert +nasal floating near segment / [+nasstop] _
dock-nasal: dock nasal&floating strategy nearest
nasal-spread: spread +nasal rightward blocked-by [-sonorant] within pword
