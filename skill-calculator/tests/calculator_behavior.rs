use skill_calculator::{CalculatorSkill, CalculatorSkillError, LocalCalculatorSkill};

#[tokio::test]
async fn computes_15_percent_of_240() {
    let skill = LocalCalculatorSkill::new();
    let result = skill.execute("15 percent of 240").await.expect("ok");
    assert_eq!(result.value, 36.0);
    assert_eq!(result.formatted, "36");
    assert!(result.to_prompt_context().contains("36"));
}

#[tokio::test]
async fn converts_freezing_point_to_celsius() {
    let skill = LocalCalculatorSkill::new();
    let result = skill.execute("32 fahrenheit to celsius").await.expect("ok");
    assert!((result.value - 0.0).abs() < 1e-6);
}

#[tokio::test]
async fn evaluates_compound_expression() {
    let skill = LocalCalculatorSkill::new();
    let result = skill.execute("(2 + 3) * 4").await.expect("ok");
    assert_eq!(result.value, 20.0);
}

#[tokio::test]
async fn rejects_empty_input() {
    let skill = LocalCalculatorSkill::new();
    let err = skill.execute("   ").await.expect_err("error");
    assert_eq!(err, CalculatorSkillError::EmptyExpression);
}

#[tokio::test]
async fn voice_prefix_is_stripped() {
    let skill = LocalCalculatorSkill::new();
    let result = skill.execute("what is 7 times 8?").await.expect("ok");
    assert_eq!(result.value, 56.0);
}
