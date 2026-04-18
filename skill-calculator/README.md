# skill-calculator

Local, offline calculator skill. Evaluates plain math, voice phrasing, percentages, and temperature conversions without any network access.

## Recognised forms

- Plain math: `(2 + 3) * 4`, `1 / 3`
- Voice operators: `7 times 8`, `10 divided by 4`, `4 squared`, `3 to the power of 4`
- Percent of: `15 percent of 240`, `15% of 240`
- Temperature: `32 fahrenheit to celsius`, `100 celsius to kelvin`
- Voice prefixes are stripped: `"what is 2 + 2?"`, `"calculate 10 * 5"`

## Run tests

```powershell
cargo test -p skill-calculator
```
