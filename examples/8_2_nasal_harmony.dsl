/* 範例 8.2(docs/03 §8.2)— 鼻化和諧:私有特徵 + 阻塞
   +nasal 來源為詞彙給定(docs/03 §3.5),測試端注入 */

Feature sonorant(+sonorant, -sonorant)

Symbol m [+sonorant]
Symbol a [+sonorant]
Symbol t [-sonorant]

Class vowel {a}

Melody nasal {+nasal} anchor segment

nasal-spread: spread +nasal rightward blocked-by [-sonorant] within pword
