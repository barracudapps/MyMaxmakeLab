Demo files used for the screenshots of the main README:

- `badge.svg`: three colours, so three layers, with text converted by the
  bundled fonts;
- `ripples.bmp`: a generated grey-level picture for image engraving.

Reproduce a screenshot state with:

```bash
cargo run --release -p mml_app -- --mode prepare docs/demo/badge.svg docs/demo/ripples.bmp
```
