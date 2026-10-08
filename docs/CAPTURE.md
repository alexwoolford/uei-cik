# Capture contract (work sqlite)

Decision: **capture the exact UEI–CIK map, not awards.** Work sqlite is `{--db}` (prod `/var/lib/uei-cik/uei-cik.sqlite`). Logical name `uei-cik`. There is no published `current/` copy. The collector watches work sqlite only.

Canonical contract: [capturable-state design principles](https://github.com/alexwoolford/capturable-state/blob/main/docs/design-principles.md) and [datetime.md](https://github.com/alexwoolford/capturable-state/blob/main/docs/datetime.md).

Pin: `capturable-state` git tag `v0.1.1` (not a path dep).

Spec: [mosaic docs/research/UEI_CIK.md](https://github.com/alexwoolford/mosaic/blob/main/docs/research/UEI_CIK.md).

## What is captured

| Table / stream | Capture? | Mode | Why |
| --- | --- | --- | --- |
| `matches` | **yes** | after | The map. Key `(uei, cik)` |
| `match_runs` | **yes** | after | Did the match finish? |
| Awards, SAM extracts, filing bodies | **no** | — | Not this tile |

Identity: one row per `(uei, cik)`. Method is `legal_name_exact`. An identical rerun must not emit a new outbox row for `matches`. A pair that no longer passes a later successful run gets `deleted_at` (Unix seconds). A 429, 500, 502, 503, or 504 is retried. Successful JSON is kept in the cache directory so a later run does not refetch it. A submissions or recipient-detail fetch that still fails records `match_runs.status=error` and does not retract. A recipient search that still fails is not cached and that name contributes no UEI on this run.

These rows are a **map**, not a score. A name that does not agree, or two CIKs that tie, stays unmatched.

Do not `collect --snapshot` this database.

## Clocks

| Layer | Columns | Type |
| --- | --- | --- |
| Facts | `as_of_date` | TEXT `YYYY-MM-DD` |
| Facts | run `started_at` / `finished_at` | TEXT `YYYY-MM-DDTHH:MM:SSZ` |
| Envelope | `_outbox.ts`, `deleted_at` | INTEGER Unix seconds |

`as_of_date` on a match row is the day the row was inserted or last changed. An identical rerun leaves it.

## Announce / nudge

`install()` on work sqlite. Announce stem is `uei-cik`. Weekly oneshot: `uei-cik-match.timer`, Sunday 09:00 UTC, `TimeoutStartSec=10h`.
