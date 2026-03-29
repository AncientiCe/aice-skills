# core-skills

`core-skills` is the umbrella crate for skill APIs and implementations.
Skill inclusion is compile-time only and controlled by Cargo features.

## Feature buckets

- `core-common`: shared core skills (`time`, `weather`, `distance`)
- `core-macos`: macOS core-only bucket
- `core-windows`: Windows core-only bucket
- `macos`: non-core macOS frontend skills
- `windows`: non-core Windows frontend skills
- `ios`: non-core iOS frontend skills

Default features: `["core-common"]`.

## Example usage

Backend (macOS core):

```toml
core-skills = { version = "...", default-features = false, features = ["core-common", "core-macos"] }
```

Frontend (macOS):

```toml
core-skills = { version = "...", default-features = false, features = ["core-common", "core-macos", "macos"] }
```

Frontend (Windows):

```toml
core-skills = { version = "...", default-features = false, features = ["core-common", "core-windows", "windows"] }
```

Frontend (iOS):

```toml
core-skills = { version = "...", default-features = false, features = ["core-common", "ios"] }
```

## Exposed skill IDs

Use `ENABLED_SKILL_IDS` to get skill IDs enabled in the current build.
