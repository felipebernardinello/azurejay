use rig::client::CompletionClient;
use rig::extractor::Extractor;
use rig::providers::groq;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::coa::orchestrator::CoAResult;
use crate::coa::prompt::build_judge_prompt;
use crate::coa::trajectory::Role;

pub struct Scaling {
    judge: LlmJudge,
    pub n: usize,
}

impl Scaling {
    #[must_use]
    pub fn new(client: &groq::Client, model_id: &str, n: usize) -> Self {
        Self { judge: LlmJudge::new(client, model_id), n }
    }

    pub async fn select_best(
        &self,
        user_input: &str,
        candidates: Vec<CoAResult>,
    ) -> anyhow::Result<CoAResult> {
        let ranked: Vec<(i32, CoAResult)> =
            futures::future::join_all(candidates.into_iter().map(|result| async move {
                let correction = extract_correction(&result);
                let score = self.judge.score(user_input, &result.answer, &correction).await?;
                anyhow::Ok((score, result))
            }))
            .await
            .into_iter()
            .collect::<anyhow::Result<Vec<_>>>()?;

        let (best_score, best) = ranked
            .into_iter()
            .max_by_key(|(score, _)| *score)
            .expect("Best-of-N always has at least one candidate");
        info!(n = self.n, best_score, "Best-of-N selected a candidate");
        Ok(best)
    }
}

const JUDGE_PREAMBLE: &str =
    "You are a strict but fair judge of an English tutor's reply. Read the case \
     and return a credibility score from 1 to 4 with a brief rationale.";

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
struct Verdict {
    rationale: String,
    score: i32,
}

struct LlmJudge {
    extractor: Extractor<groq::CompletionModel, Verdict>,
}

impl LlmJudge {
    fn new(client: &groq::Client, model_id: &str) -> Self {
        let extractor = client
            .extractor::<Verdict>(model_id)
            .preamble(JUDGE_PREAMBLE)
            .build();
        Self { extractor }
    }

    async fn score(
        &self,
        user_input: &str,
        suggested_answer: &str,
        correction: &str,
    ) -> anyhow::Result<i32> {
        let case = build_judge_prompt(user_input, correction, suggested_answer);
        let verdict = self.extractor.extract(&case).await?;
        Ok(verdict.score.clamp(1, 4))
    }
}

fn extract_correction(result: &CoAResult) -> String {
    result
        .trajectory
        .segments
        .iter()
        .find(|s| s.role == Role::SaveCorrection)
        .map(|s| s.content.clone())
        .unwrap_or_default()
}
