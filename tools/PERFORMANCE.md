# Local metadata CPU benchmark

Run the deterministic, network-free workload explicitly:

```sh
cargo test --offline --lib benchmark_metadata_cpu -- --ignored --nocapture --test-threads=1
```

The test reports the median of five samples, each containing three calls, after
one warm-up call. Fixture creation is outside the timed region. It uses the
repository's default test build, not the release build. Timings include result
construction and destruction.

## Internal research optimization

Measured on the same Linux development machine before and after the refactor:

| Workload | Before | After |
| --- | ---: | ---: |
| Search: 8 activities, 16 attachments each, long descriptions | 882.201 ms | 158.948 ms |
| Detail: first window of a 120-activity course | 21.709 ms | 0.433 ms |
| Detail: window 70 of that course | 22.201 ms | 0.503 ms |

Search now reuses section/activity scores and activity excerpts across matching
attachments, borrows file metadata, and avoids normalizing the same text twice
for matching/scoring. Shared calculations are lazy so misses do not trigger them.

Detail still counts every selected source to report the same total, but builds
JSON and text only for the four requested blocks. It no longer concatenates and
copies all source text into character arrays or builds discarded blocks.

These synthetic CPU measurements do not predict end-to-end school latency.
Network requests, cache policy, public schemas and response fields are unchanged.

The ordinary test suite checks fingerprints of complete outputs captured before
the refactor: search ordering/scores/excerpts, cross-field TD matching, accents,
empty input, detail Unicode chunk boundaries, selectors, pagination and error
precedence. Do not update those fingerprints merely to make a refactor pass.
