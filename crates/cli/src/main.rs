//! 薄殼:把 argv 交給 [`conlang_cli::run`],把輸出印出去。
//!
//! **所有邏輯都在 lib**,故整條路可以被 Rust 整合測試直接呼叫,
//! 不必為了測試而開子程序(§0.2:每個階段都要有測試出口)。
fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = String::new();
    match conlang_cli::run(&args, &mut out) {
        Ok(()) => {
            print!("{out}");
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            print!("{out}");
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
