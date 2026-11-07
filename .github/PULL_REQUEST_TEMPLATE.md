## What and why

<!-- What does this change, and why? Link the issue it closes, e.g. "Closes #12". -->

## How it was tested

<!-- Platforms you tried it on (macOS Apple Silicon, Windows), the kind of shoot and cameras, and what you checked by hand. If it changes decisions, what `cargo run --release -p cull-analysis --example calibrate` showed before and after. -->

## Screenshots

<!-- For interface changes: before and after, dark and light if it matters. Delete this section otherwise. -->

## Checklist

- [ ] The title follows [Conventional Commits](https://www.conventionalcommits.org) (`feat:`, `fix:`, `docs:`, `refactor:`, `chore:`).
- [ ] `pnpm typecheck && pnpm test && pnpm build` pass.
- [ ] `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` pass.
- [ ] New interface strings are in all 5 files of `src/lib/i18n/locales` (`fr.ts` is the reference).
- [ ] No new setting where a better default would do.
- [ ] No photo, model file or secret added to the repository.
- [ ] Docs updated if behaviour changed (`README.md`, `README.fr.md`, `docs/`, the crate READMEs).
