pub fn run() -> Result<(), u8> {
    eprintln!(
        "`vergil corpus` is a stub. The property catalog lives in\n\
         crates/vergil-properties/templates/. Add a new template by\n\
         creating <id>/manifest.yaml and <id>/halmos.sol; the next\n\
         `vergil verify --intent` run picks it up automatically.\n\
         \n\
         A management UI ships in Phase 4."
    );
    Err(3)
}
