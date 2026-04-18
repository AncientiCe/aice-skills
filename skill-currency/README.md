# skill-currency

Currency conversion backed by [frankfurter.app](https://www.frankfurter.app/) (free, no API key, ECB reference rates). Reuses `skill-live-info-common` for TTL caching and retry-with-stale-fallback.

## Examples

- 50 euro to usd
- 100 yen to usd
- gbp to chf

## Run tests

```powershell
cargo test -p skill-currency
```
