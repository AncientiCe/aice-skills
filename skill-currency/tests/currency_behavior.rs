use skill_currency::{
    CurrencyError, CurrencyQuery, CurrencyRateProvider, CurrencyResult, CurrencySkill,
    HttpCurrencySkill,
};

use async_trait::async_trait;
use std::sync::Arc;
use std::time::SystemTime;

struct StaticProvider {
    rate: f64,
}

#[async_trait]
impl CurrencyRateProvider for StaticProvider {
    async fn fetch(&self, query: &CurrencyQuery) -> Result<CurrencyResult, CurrencyError> {
        Ok(CurrencyResult {
            amount: query.amount,
            from_currency: query.from_currency.to_uppercase(),
            to_currency: query.to_currency.to_uppercase(),
            converted_amount: query.amount * self.rate,
            rate: self.rate,
            rate_date: "2026-04-18".to_string(),
            as_of: SystemTime::now(),
        })
    }
}

#[tokio::test]
async fn converts_50_eur_to_usd_with_static_rate() {
    let skill = HttpCurrencySkill::with_provider(Arc::new(StaticProvider { rate: 1.10 }));
    let result = skill
        .execute(&CurrencyQuery {
            amount: 50.0,
            from_currency: "eur".into(),
            to_currency: "usd".into(),
        })
        .await
        .expect("ok");
    assert!((result.converted_amount - 55.0).abs() < 1e-6);
    assert_eq!(result.to_currency, "USD");
    assert!(result.to_prompt_context().contains("USD"));
}

#[tokio::test]
async fn invalid_currency_code_is_provider_error() {
    struct InvalidProvider;
    #[async_trait]
    impl CurrencyRateProvider for InvalidProvider {
        async fn fetch(&self, _query: &CurrencyQuery) -> Result<CurrencyResult, CurrencyError> {
            Err(CurrencyError::InvalidQuery("bad code".into()))
        }
    }
    let skill = HttpCurrencySkill::with_provider(Arc::new(InvalidProvider));
    let err = skill
        .execute(&CurrencyQuery {
            amount: 1.0,
            from_currency: "ZZZZ".into(),
            to_currency: "USD".into(),
        })
        .await
        .expect_err("err");
    assert!(matches!(err, CurrencyError::InvalidQuery(_)));
}
