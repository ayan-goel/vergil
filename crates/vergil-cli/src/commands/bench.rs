pub fn run() -> Result<(), u8> {
    eprintln!(
        "`vergil bench` is a stub. Run the dedicated bench binary instead:\n\
         \n\
           cargo run --release --bin vergilbench -- --corpus vergilbench\n\
         \n\
         See docs/book/src/cli-reference.md for the full bench workflow."
    );
    Err(3)
}
