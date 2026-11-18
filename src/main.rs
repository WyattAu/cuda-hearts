//! Binary entry point.

#[cfg(feature = "gui")]
fn main() {
    cuda_hearts::game::run();
}

#[cfg(not(feature = "gui"))]
fn main() {
    eprintln!(
        "cuda-hearts: the story engine is available as a library, but the game window needs `--features gui`"
    );
    eprintln!("  cargo run --features gui");
}
