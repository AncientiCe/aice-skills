# skill-briefing

Killer skill: a one-command personalized briefing.

`ComposedBriefingSkill` fans out to weather, calendar, email, and news in parallel and stitches the results into a single `BriefingResult` whose `to_prompt_context()` is fed back to the LLM for spoken delivery.

## Failure isolation

A failing sub-skill becomes a `BriefingSection::*(Err(..))` rather than failing the entire briefing — the user always gets *something* back.

## Run tests

```powershell
cargo test -p skill-briefing
```
