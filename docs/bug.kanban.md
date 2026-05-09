# Bugs
**Issue and bug tracking for Cadmus**

id: fb2aixeyeiy2guqnmqjtbpjj
template: bug

Tracks defects and accepted deviations from the target vision. Cards
with `severity: accepted` document conscious deviations — the
"won't fix" bucket per CLAUDE.md.

## Open
id: 4eg4bdjpr1eybhzyhemevgu9

### `TranscribeOptions::threads` not implemented (definition.md §4.2)
id: aymr0v0a44swyl4rlpftqf2b
severity: accepted
priority: low

`definition.md §4.2` lists `threads: Option<u32>` on
`TranscribeOptions` ("per-call thread count override"). Cadmus does
not surface this field — `TranscribeOptions` exposes only `language`
and `beam_size`.

Reason: `ct2rs 0.9.18` has no per-call thread override. Threading
lives on `Config::num_threads_per_replica`, which is set when
`Whisper::new` is called and cannot be changed for the life of the
instance. The only feasible per-call workaround would tear down and
rebuild the `Whisper` instance per call — orders of magnitude more
expensive than the inference itself, plus it would re-load the model
weights from disk on every call.

Accepted deviation. `LoadModelOptions::threads` remains the only
thread knob. Reintroduce when ct2rs grows a per-call equivalent.

### Detected language not surfaced when `TranscribeOptions::language == None`
id: dh0gxqm0uy5vl7zuw5st9ijp
severity: accepted
priority: low

When the caller leaves `TranscribeOptions::language` unset,
`Whisper::generate(samples, None, ...)` runs language detection
internally and uses the detected `<|xx|>` token in the prompt prefix,
but discards it before returning chunks. ct2rs 0.9.18's high-level
`Whisper` returns only the model-generated tokens, not the prompt
prefix — empirically verified with `tiny` on the German fixture:
chunks contain only `<|0.00|> ... <|3.00|>`, no language token.

`PLAN_public_api.md` assumption A5 expected the token to be in the
output stream and added the helper
`inference::detect_language_from_chunks` to parse it. The helper is
correct in isolation (3 unit tests pass) but ct2rs gives it nothing
to find. The helper stays in the code so behavior auto-corrects when
upstream surfaces the detected token.

Result: when `TranscribeOptions::language == None`,
`TranscriptResult.language == ""`. When the caller passes an explicit
`Some("de")`, that value is echoed back verbatim — the round-trip
case (the common one) works.

The fix would require dropping to `ct2rs::sys::Whisper` plus an
in-house mel spectrogram pipeline (~150 LOC of ndarray/rustfft
plumbing) — out of v1 scope. Reactivate when the ct2rs upstream
backlog item lands (see `docs/backlog.kanban.md`: "Surface ct2rs
internally-detected language token").

## In Progress
id: fpxmyg2qwsy8kuxtv3lzrige

## Done
id: 9bd2g6q54xi7cfr0hhnqzls0

<!-- markdown-kanban
name: bug
description: |
  Tracks defects and accepted deviations from the target vision.
columnsLocked: false
columns:
  - key: open
    title: Open
    description: Confirmed defects or accepted deviations awaiting work or acknowledgement.
  - key: inprogress
    title: In Progress
    description: Being actively worked on.
  - key: done
    title: Done
    description: Resolved or shipped.
cardFields:
  - key: severity
    type: select
    options:
      - low
      - medium
      - high
      - accepted
    description: |
      low / medium / high — defect severity.
      accepted — conscious deviation from target vision; will not be fixed.
  - key: priority
    type: select
    options:
      - none
      - low
      - medium
      - high
    description: |
      none / low / medium / high — relative ordering for "Open" defects.
-->
