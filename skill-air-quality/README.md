# skill-air-quality

Air quality (US AQI / EU AQI / PM2.5 / PM10 / O3) backed by [Open-Meteo's free air-quality API](https://open-meteo.com/en/docs/air-quality-api). Mirrors `skill-weather`: same `default_location` fallback, same TTL caching pattern.

## Examples

- "what's the air quality?"
- "is the air quality bad in Brussels?"
- "pollen and AQI today"

## Run tests

```powershell
cargo test -p skill-air-quality
```
