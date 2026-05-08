# Backlog
**Ideas and pending work for Cadmus**

id: rpqrs4lyfvb86gmhlmteln14
template: backlog

Pending work and ideas for Cadmus that are not part of the active plan.
The Someday column parks speculative items that probably will not be
implemented, so the committed columns stay honest.

## Someday
id: u9pqf6tijgyqo6eidedpm8ua

Ideas that probably will not happen, but deserve to be written down.

## Open
id: pqhx4mr761392sc6t42lk3ji

Considered, scoped enough, ready to be picked up.

### Expose CTranslate2 version through ct2rs upstream — track and adopt
id: s4uvcn156fm4jtik1numnxqs
priority: medium

`ct2rs 0.9.18` does not expose the bundled CTranslate2 C++ library version
through any public Rust constant or function. The bundled version (`4.7.1`
at the time of writing) is only readable from
`ct2rs/CTranslate2/python/ctranslate2/version.py`, which is not on a stable
build-script-accessible path because ct2rs has no `[package].links` key —
so cargo does not surface the dep's source dir to our `build.rs`.

Result: `cadmus::version().ctranslate2` returns `""` until ct2rs grows a
public surface (e.g. `ct2rs::CTRANSLATE2_VERSION` or `ct2rs::ctranslate2_version()`).
Track the ct2rs upstream; once a public version surface lands, switch
`build.rs` from the empty fallback to the real value and drop this card.

PLAN_skeleton.md R1 Fallback B.

### HTTP Range / resume on `download_model`
id: flna2x9g3w082f7ubsr06uod
priority: low

The downloader introduced in `PLAN_model_storage` writes downloaded
files in one shot. If a download is interrupted (network drop,
process crash, cooperative cancel), the partial file is deleted and
the next run downloads from byte zero. For `tiny` (~75 MB) that's
tolerable; for `large-v3` (~1.5 GB) on a flaky link it's painful.

Adding HTTP Range request support would let `download_model` resume
a partial download by sending `Range: bytes=N-` and appending to the
existing file. Requires the server to honour Range (HuggingFace's
CDN does), and a "is the partial file actually a prefix of the full
file" decision — the simplest is "if size matches Content-Length
already, treat as cached; if smaller, send Range; if larger, delete
and restart". Definition.md §5 already says download integrity is
not verified — Range support does not change that contract.

Open against a future plan; not part of v1's local-verification
flow which prefers the simpler "redownload on failure" path.

## In Progress
id: tw80l0gyryxgw8p4rxkv055j

Being actively worked on.

## Done
id: x8vv0f33ci8qvea4z09xkqbt

Completed and shipped.

<!-- markdown-kanban
# Writers use id: {new} for new boards, columns, and cards.
# Processing systems replace {new} with generated IDs on parse.
name: backlog
description: |
  Tracks ideas and pending work through four stages: from rough
  wishlist (Someday), through deliberate intent (Open), to active
  work (In Progress), to delivery (Done).
columnsLocked: false
columns:
  - key: someday
    title: Someday
    description: |
      Ideas that probably will not happen, but deserve to be written
      down.
  - key: open
    title: Open
    description: Considered, scoped enough, ready to be picked up.
  - key: inprogress
    title: In Progress
    description: Being actively worked on.
  - key: done
    title: Done
    description: Completed and shipped.
cardFields:
  - key: priority
    type: select
    options:
      - none
      - low
      - medium
      - high
    description: |
      none — not yet decided
      low — nice to have, low impact if delayed
      medium — meaningful, should not sit indefinitely
      high — important, work on this before lower-priority items
-->
