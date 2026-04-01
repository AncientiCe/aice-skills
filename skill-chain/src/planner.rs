use crate::types::{ChainPlan, ChainPlanner, ChainPlannerError, PlannerRequest};
use async_trait::async_trait;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalIntent {
    pub name: String,
    pub slots: BTreeMap<String, String>,
}

#[async_trait]
pub trait SemanticNormalizer: Send + Sync {
    async fn normalize(&self, phrase: &str) -> Result<CanonicalIntent, ChainPlannerError>;
}

#[async_trait]
pub trait ChainCompiler: Send + Sync {
    async fn compile(
        &self,
        intent: &CanonicalIntent,
        request: &PlannerRequest,
    ) -> Result<ChainPlan, ChainPlannerError>;
}

pub struct LlmPipelinePlanner<N, C> {
    normalizer: N,
    compiler: C,
}

impl<N, C> LlmPipelinePlanner<N, C> {
    pub fn new(normalizer: N, compiler: C) -> Self {
        Self {
            normalizer,
            compiler,
        }
    }
}

#[async_trait]
impl<N, C> ChainPlanner for LlmPipelinePlanner<N, C>
where
    N: SemanticNormalizer,
    C: ChainCompiler,
{
    async fn plan(&self, request: &PlannerRequest) -> Result<ChainPlan, ChainPlannerError> {
        let intent = self.normalizer.normalize(&request.user_phrase).await?;
        self.compiler.compile(&intent, request).await
    }
}
