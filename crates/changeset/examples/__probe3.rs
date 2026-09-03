use conlang_language::LanguageDocument;

const A: &str = "Symbol a\n\ntrait LocalAnimate:\n\ntrait LocalNoun:\n\nsign x:\n    belongs LocalNoun\n    phon:\n        /a/\n";
const B: &str = "Symbol a\n\ntrait LocalAnimate:\n\ntrait LocalNoun:\n    belongs LocalAnimate\n\nsign x:\n    belongs LocalNoun\n    phon:\n        /a/\n";

fn ids(src: &str) -> Vec<String> {
    let d = LanguageDocument::import_new_root(src, "evo:r").expect("parses");
    d.language()
        .signs
        .iter()
        .map(|s| s.id.0.to_string())
        .collect()
}

fn main() {
    println!("同一份文字匯入兩次,id 相同? {}", ids(A) == ids(A));
    println!("A 的 sign id = {:?}", ids(A));
    println!("B 的 sign id = {:?}", ids(B));
    println!(
        "加了 trait 的 belongs 之後,sign id 變了? {}",
        ids(A) != ids(B)
    );
}
