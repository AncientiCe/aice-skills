//! First-class routine templates.
//!
//! These ship as ready-to-run `ChainPlan`s so callers can wire common
//! routines without round-tripping through the LLM planner. Each template
//! uses only skills available in `core-common` plus opt-in `macos` skills
//! marked `side_effect = false` for read-only steps.
//!
//! Wire a routine like:
//! ```ignore
//! let plan = morning_briefing();
//! executor.run(plan).await?;
//! ```

use crate::types::{
    ChainPlan, ChainStep, DonePayload, DoneStatus, RunSkillStep, SpeakUpdateStep, StepTarget,
};
use std::collections::BTreeMap;
use std::str::FromStr;

/// Identifier for a built-in routine. Stable strings — safe to persist in
/// configuration or recall via voice ("run my wind down routine").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RoutineId {
    MorningBriefing,
    WindDown,
    Focus,
    Commute,
}

impl RoutineId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MorningBriefing => "morning_briefing",
            Self::WindDown => "wind_down",
            Self::Focus => "focus",
            Self::Commute => "commute",
        }
    }
}

/// Returned when a string can't be parsed into a [`RoutineId`]. Carries the
/// offending input so callers can surface a helpful error.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown routine id: {0:?}")]
pub struct UnknownRoutineId(pub String);

impl FromStr for RoutineId {
    type Err = UnknownRoutineId;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().as_str() {
            "morning_briefing" | "morning briefing" | "morning" => Ok(Self::MorningBriefing),
            "wind_down" | "wind down" | "winddown" | "evening" => Ok(Self::WindDown),
            "focus" | "deep_work" | "deep work" => Ok(Self::Focus),
            "commute" => Ok(Self::Commute),
            _ => Err(UnknownRoutineId(value.to_string())),
        }
    }
}

/// Catalog entry returned by [`routine_catalog`].
#[derive(Clone, Debug)]
pub struct RoutineTemplate {
    pub id: RoutineId,
    pub name: String,
    pub description: String,
    pub plan: ChainPlan,
}

/// Return all built-in routine templates.
pub fn routine_catalog() -> Vec<RoutineTemplate> {
    vec![
        RoutineTemplate {
            id: RoutineId::MorningBriefing,
            name: "Morning Briefing".to_string(),
            description: "Time, weather, today's calendar, top headlines.".to_string(),
            plan: morning_briefing(),
        },
        RoutineTemplate {
            id: RoutineId::WindDown,
            name: "Wind Down".to_string(),
            description: "Quiet the room, dim lights, capture a journal entry.".to_string(),
            plan: wind_down(),
        },
        RoutineTemplate {
            id: RoutineId::Focus,
            name: "Focus".to_string(),
            description: "Mute notifications, start a 25-minute timer.".to_string(),
            plan: focus(),
        },
        RoutineTemplate {
            id: RoutineId::Commute,
            name: "Commute".to_string(),
            description: "Weather, traffic time, news headlines for the road.".to_string(),
            plan: commute(),
        },
    ]
}

/// Look up a routine by id.
pub fn routine_for(id: RoutineId) -> RoutineTemplate {
    routine_catalog()
        .into_iter()
        .find(|t| t.id == id)
        .expect("routine_catalog must contain every RoutineId variant")
}

fn run_skill(skill_id: &str, input: &str, target: StepTarget, side_effect: bool) -> ChainStep {
    ChainStep::RunSkill(RunSkillStep {
        skill_id: skill_id.to_string(),
        input: input.to_string(),
        target,
        side_effect,
        retryable: !side_effect,
        on_error: Vec::new(),
    })
}

fn speak(message: &str) -> ChainStep {
    ChainStep::SpeakUpdate(SpeakUpdateStep {
        message: message.to_string(),
    })
}

fn done(status: DoneStatus, summary: &str) -> ChainStep {
    ChainStep::Done(DonePayload {
        status,
        user_summary: summary.to_string(),
        structured_result: BTreeMap::new(),
    })
}

/// Morning briefing: time, weather, calendar, news. Read-only.
pub fn morning_briefing() -> ChainPlan {
    ChainPlan {
        steps: vec![
            speak("Here's your morning briefing."),
            run_skill("skill_time", "now", StepTarget::Backend, false),
            run_skill("skill_weather", "today", StepTarget::Backend, false),
            run_skill("skill_calendar", "today", StepTarget::Backend, false),
            run_skill(
                "skill_news_headlines",
                "top headlines",
                StepTarget::Backend,
                false,
            ),
            done(DoneStatus::Success, "Morning briefing delivered."),
        ],
    }
}

/// Wind down: lower volume, dim lights, capture a journal entry.
pub fn wind_down() -> ChainPlan {
    ChainPlan {
        steps: vec![
            speak("Winding down. Lowering the lights."),
            run_skill("skill_volume", "set 20", StepTarget::Frontend, true),
            run_skill(
                "skill_smart_home",
                "dim lights to 20%",
                StepTarget::Frontend,
                true,
            ),
            run_skill(
                "skill_journal",
                "How did today go?",
                StepTarget::Backend,
                true,
            ),
            done(DoneStatus::Success, "Wind down routine complete."),
        ],
    }
}

/// Focus: silence interruptions, start a 25-minute Pomodoro timer.
pub fn focus() -> ChainPlan {
    ChainPlan {
        steps: vec![
            speak("Entering focus mode for twenty-five minutes."),
            run_skill("skill_volume", "set 0", StepTarget::Frontend, true),
            run_skill(
                "skill_timer",
                "25 minutes focus",
                StepTarget::Frontend,
                true,
            ),
            done(DoneStatus::Success, "Focus session started."),
        ],
    }
}

/// Commute: weather, traffic distance, news for the drive.
pub fn commute() -> ChainPlan {
    ChainPlan {
        steps: vec![
            speak("Here's your commute update."),
            run_skill("skill_weather", "today", StepTarget::Backend, false),
            run_skill("skill_distance", "to work", StepTarget::Backend, false),
            run_skill(
                "skill_news_headlines",
                "top headlines",
                StepTarget::Backend,
                false,
            ),
            done(DoneStatus::Success, "Commute briefing delivered."),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill_ids(plan: &ChainPlan) -> Vec<String> {
        plan.steps
            .iter()
            .filter_map(|s| match s {
                ChainStep::RunSkill(r) => Some(r.skill_id.clone()),
                _ => None,
            })
            .collect()
    }

    fn last_done(plan: &ChainPlan) -> Option<&DonePayload> {
        plan.steps.last().and_then(|s| match s {
            ChainStep::Done(d) => Some(d),
            _ => None,
        })
    }

    #[test]
    fn morning_briefing_includes_weather_and_news() {
        let ids = skill_ids(&morning_briefing());
        assert!(ids.contains(&"skill_time".to_string()));
        assert!(ids.contains(&"skill_weather".to_string()));
        assert!(ids.contains(&"skill_calendar".to_string()));
        assert!(ids.contains(&"skill_news_headlines".to_string()));
    }

    #[test]
    fn focus_includes_timer() {
        let ids = skill_ids(&focus());
        assert!(ids.contains(&"skill_timer".to_string()));
        assert!(ids.contains(&"skill_volume".to_string()));
    }

    #[test]
    fn every_routine_ends_with_done_success() {
        for template in routine_catalog() {
            let done = last_done(&template.plan).expect("routine must end with Done");
            assert_eq!(done.status, DoneStatus::Success, "{}", template.name);
        }
    }

    #[test]
    fn read_only_steps_are_marked_retryable() {
        for template in routine_catalog() {
            for step in &template.plan.steps {
                if let ChainStep::RunSkill(run) = step {
                    if !run.side_effect {
                        assert!(
                            run.retryable,
                            "read-only step {} in {} should be retryable",
                            run.skill_id, template.name
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn routine_id_round_trips_through_strings() {
        for template in routine_catalog() {
            let s = template.id.as_str();
            assert_eq!(s.parse::<RoutineId>().ok(), Some(template.id));
        }
    }

    #[test]
    fn routine_for_returns_matching_template() {
        let t = routine_for(RoutineId::Commute);
        assert_eq!(t.id, RoutineId::Commute);
    }

    #[test]
    fn routine_id_accepts_natural_aliases() {
        assert_eq!(
            "morning briefing".parse::<RoutineId>().ok(),
            Some(RoutineId::MorningBriefing)
        );
        assert_eq!(
            "wind down".parse::<RoutineId>().ok(),
            Some(RoutineId::WindDown)
        );
        assert_eq!(
            "deep work".parse::<RoutineId>().ok(),
            Some(RoutineId::Focus)
        );
    }

    #[test]
    fn routine_id_returns_unknown_for_garbage() {
        let err = "tea time".parse::<RoutineId>().expect_err("expected error");
        assert_eq!(err, UnknownRoutineId("tea time".to_string()));
    }
}
