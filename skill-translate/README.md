# skill-translate

Translation skill backed by the **local** Ollama LLM (no external translation service). Runtime adapts its concrete `LlmStream` (or any other LLM transport) to the small `TranslationLlm` adapter trait this crate exposes, keeping `aice-skills` independent of any LLM transport.

## Examples

- "how do you say good morning in french"
- "translate hello to spanish"
- "say I love you in italian"

## Run tests

```powershell
cargo test -p skill-translate
```
