# uei-cik

Exact map from a USAspending recipient UEI to a listed SEC CIK. The normalized recipient name equals the SEC conformed name or a former name, and the recipient state equals the SEC business-address state. Two CIKs that tie store nothing.

Not a fuzzy match, not an award feed, and not a score.

```bash
export SEC_USER_AGENT='uei-cik you@real-domain'
cargo run --release -- match --db data/uei-cik.sqlite --cache-dir data/http-cache
cargo run --release -- status --db data/uei-cik.sqlite
cargo run --release -- lookup --db data/uei-cik.sqlite 0000936468
```

Successful SEC and USAspending JSON stays in the cache directory. A later run reads those files.

Spec: [mosaic `docs/research/UEI_CIK.md`](https://github.com/alexwoolford/mosaic/blob/main/docs/research/UEI_CIK.md). Capture: [docs/CAPTURE.md](docs/CAPTURE.md).
