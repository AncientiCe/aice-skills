use skill_unit_conversion::{LocalUnitConversionSkill, UnitConversionError, UnitConversionSkill};

#[tokio::test]
async fn cups_in_a_liter() {
    let skill = LocalUnitConversionSkill::new();
    let result = skill.execute_query("cups in a liter").await.expect("ok");
    assert!((result.output_value - 4.226752838).abs() < 1e-6);
    assert_eq!(result.from_unit, "liter");
    assert_eq!(result.to_unit, "cup");
}

#[tokio::test]
async fn convert_pounds_to_kilograms() {
    let skill = LocalUnitConversionSkill::new();
    let result = skill
        .execute_query("convert 150 pounds to kilograms")
        .await
        .expect("ok");
    assert!((result.output_value - 68.038855).abs() < 1e-3);
}

#[tokio::test]
async fn glued_unit_in_query() {
    let skill = LocalUnitConversionSkill::new();
    let result = skill.execute_query("5km to miles").await.expect("ok");
    assert!((result.output_value - 3.10685596).abs() < 1e-6);
}

#[tokio::test]
async fn temperature_through_query() {
    let skill = LocalUnitConversionSkill::new();
    let result = skill
        .execute_query("100 celsius to fahrenheit")
        .await
        .expect("ok");
    assert!((result.output_value - 212.0).abs() < 1e-6);
}

#[tokio::test]
async fn malformed_query_is_parse_error() {
    let skill = LocalUnitConversionSkill::new();
    let err = skill.execute_query("just nonsense").await.expect_err("err");
    assert!(matches!(err, UnitConversionError::ParseError(_)));
}
