pub fn run() -> Result<(), u8> {
    eprintln!(
        "`vergil init` is a stub. To scaffold a Vergil-ready Foundry\n\
         project by hand:\n\
         \n\
           1. Create a `properties.yaml` with the `check_*` functions\n\
              to verify (see examples/erc20/properties.yaml for the shape).\n\
           2. Write a `test/Properties.t.sol` with hand-coded check_\n\
              bodies, OR use `vergil verify <project> --intent \"...\"`\n\
              to let the synthesizer propose them.\n\
           3. Run `vergil verify <project>` to dispatch the portfolio.\n\
         \n\
         A real scaffolder ships in Phase 4."
    );
    Err(3)
}
