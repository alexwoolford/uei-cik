# Daily ops

Weekly match, not a daily ingest. The oneshot walks every listed CIK, then every distinct normalized name. A cold run is on the order of six hours. `TimeoutStartSec=10h`.

| Unit | Schedule |
| --- | --- |
| `uei-cik-match.timer` | Sunday 09:00 UTC, 30m jitter, `Persistent=true` |
| `uei-cik-match.service` | oneshot, user `ueicik` |

Work sqlite: `/var/lib/uei-cik/uei-cik.sqlite`. JSON cache: `/var/lib/uei-cik/http-cache`. Env: `/opt/uei-cik/etc/uei-cik.env` (mode 600). Logs: `journalctl -u uei-cik-match.service`.

A 429 or 502 is retried. Successful JSON stays in the cache. A recipient search that still fails contributes no UEI for that name and is not cached. A submissions or recipient-detail fetch that still fails records `match_runs.status=error` and does not retract.

Do not print `SEC_USER_AGENT`.
