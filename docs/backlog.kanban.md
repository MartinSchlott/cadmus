# Backlog
**Ideas and pending work for Cadmus**

id: {new}
template: backlog

Pending work and ideas for Cadmus that are not part of the active plan.
The Someday column parks speculative items that probably will not be
implemented, so the committed columns stay honest.

## Someday
id: {new}

Ideas that probably will not happen, but deserve to be written down.

## Open
id: {new}

Considered, scoped enough, ready to be picked up.

### Expose CTranslate2 version through ct2rs upstream — track and adopt
id: {new}
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

## In Progress
id: {new}

Being actively worked on.

## Done
id: {new}

Completed and shipped.

<!-- markdown-kanban
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
    options: [none, low, medium, high]
    description: |
      none — not yet decided
      low — nice to have, low impact if delayed
      medium — meaningful, should not sit indefinitely
      high — important, work on this before lower-priority items
-->
