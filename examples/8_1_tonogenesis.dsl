/* 範例 8.1(docs/02 §12)— tonogenesis 規則檔
   宣告貼合 Lexurgy(Feature/Symbol/Class);Melody 為本 DSL 擴充 */

Feature voice(+voice, -voice)

Symbol p [-voice]
Symbol b [+voice]
Symbol a

Class vowel {a}

Melody tone {H, M, L} anchor mora

tonogenesis:
    insert H floating near mora / onset&[-voice] _
    insert L floating near mora / onset&[+voice] _

dock-tone: dock tone&floating strategy nearest

devoicing:
    level: word
    [+voice]&onset => [-voice]

fill-default: fill tone Ø => M within pword

ocp-cleanup: merge adjacent-equal
