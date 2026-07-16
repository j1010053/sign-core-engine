//! P20 依賴方向守衛:`dsl`/`core` 不得依賴 language/changeset(CI 檢查的測試版)。
//! 依賴方向恆為 `changeset → language → dsl`;dsl 只知 Word,不知 sign。

#[test]
fn dsl_and_core_must_not_depend_on_language_or_changeset_p20() {
    for manifest in [
        concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"),
        concat!(env!("CARGO_MANIFEST_DIR"), "/../core/Cargo.toml"),
    ] {
        let s = std::fs::read_to_string(manifest).unwrap();
        for forbidden in ["conlang-language", "conlang-changeset", "conlang-sign"] {
            assert!(
                !s.contains(forbidden),
                "P20 violation: {manifest} depends on {forbidden}"
            );
        }
    }
}
