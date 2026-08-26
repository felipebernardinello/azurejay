use rig::client::CompletionClient;
use rig::completion::CompletionModel as _;
use rig::message::Message as RigMessage;
use rig::providers::groq;
use serde_json::json;
use tracing::{info, warn};

use crate::coa::prompt::{COA_SYSTEM_PROMPT, build_context_block};
use crate::coa::scaling::Scaling;
use crate::coa::tools::{ToolContext, ToolRegistry};
use crate::coa::trajectory::{
    Role, Segment, Trajectory, first_actionable_segment, parse_double_check_score, parse_segments,
    tool_stop_sequences,
};

const CONTINUE_NUDGE: &str =
    "Continue the trajectory exactly where you left off. Output only the next \
     step(s); do not repeat any earlier text.";

#[derive(Debug, Clone)]
pub struct HistoryTurn {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct CoAConfig {
    pub max_steps: usize,
    pub max_tokens: u32,
    pub temperature: f32,
    pub top_p: f32,
    pub double_check_threshold: i32,
    pub max_replans: usize,
}

impl Default for CoAConfig {
    fn default() -> Self {
        Self {
            max_steps: 24,
            max_tokens: 32_768,
            temperature: 1.0,
            top_p: 0.9,
            double_check_threshold: 3,
            max_replans: 2,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CoAResult {
    pub answer: String,
    pub trajectory: Trajectory,
    pub steps: usize,
    pub replans: usize,
}

pub struct CoA {
    model: groq::CompletionModel,
    tools: ToolRegistry,
    config: CoAConfig,
    scaling: Option<Scaling>,
}

impl CoA {
    pub fn new(
        api_key: &str,
        model_id: &str,
        tools: ToolRegistry,
        config: CoAConfig,
        tts_n: usize,
    ) -> anyhow::Result<Self> {
        let client = groq::Client::new(api_key)
            .map_err(|e| anyhow::anyhow!("failed to build Groq client: {e}"))?;
        let model = client.completion_model(model_id);
        let scaling = if tts_n > 1 {
            info!(n = tts_n, "test-time scaling enabled: Best-of-N");
            Some(Scaling::new(&client, model_id, tts_n))
        } else {
            None
        };
        Ok(Self { model, tools, config, scaling })
    }

    pub async fn run(
        &self,
        user_input: &str,
        history: &[HistoryTurn],
        user_profile: &str,
        user_id: &str,
        conversation_id: &str,
    ) -> anyhow::Result<CoAResult> {
        let history_str = history
            .iter()
            .map(|t| format!("{}: {}", t.role, t.content))
            .collect::<Vec<_>>()
            .join("\n");
        let ctx = ToolContext::new(user_id, conversation_id);
        let req = TurnRequest {
            user_input,
            conversation_history: &history_str,
            user_profile,
            conversation_id,
            tool_context: &ctx,
        };

        match &self.scaling {
            Some(scaling) => self.run_best_of_n(req, scaling).await,
            None => self.run_once(req).await,
        }
    }

    async fn run_best_of_n(
        &self,
        req: TurnRequest<'_>,
        scaling: &Scaling,
    ) -> anyhow::Result<CoAResult> {
        let candidates = futures::future::join_all((0..scaling.n).map(|_| self.run_once(req)))
            .await
            .into_iter()
            .collect::<anyhow::Result<Vec<_>>>()?;
        scaling.select_best(req.user_input, candidates).await
    }

    async fn run_once(&self, req: TurnRequest<'_>) -> anyhow::Result<CoAResult> {
        let context_block = build_context_block(
            req.conversation_id,
            req.user_profile,
            req.conversation_history,
            req.user_input,
        );

        let mut trajectory = Trajectory::new();
        let mut assistant_buffer = String::new();
        let mut steps = 0usize;
        let mut replans = 0usize;

        let gen_config = GenerationConfig {
            temperature: self.config.temperature,
            top_p: self.config.top_p,
            max_tokens: self.config.max_tokens,
            stop: tool_stop_sequences(),
        };

        while steps < self.config.max_steps {
            steps += 1;

            let chunk = self.complete(&context_block, &assistant_buffer, &gen_config).await?;
            if chunk.is_empty() {
                warn!(step = steps, "CoA model returned an empty chunk");
                break;
            }

            assistant_buffer.push_str(&chunk);
            trajectory.segments = parse_segments(&assistant_buffer);

            let action = first_actionable_segment(&chunk);

            if let Some(act) = &action
                && act.role == Role::Answer
            {
                let answer = act.content.trim().to_string();
                info!(steps, hops = trajectory.n_hops(), "CoA finished");
                return Ok(CoAResult { answer, trajectory, steps, replans });
            }

            if let Some(act) = &action
                && act.is_tool_call()
            {
                let observation = self.run_tool(act, req.tool_context).await;
                assistant_buffer.push_str(&format!("\n<observation>\n{observation}\n</observation>\n"));
                trajectory.segments = parse_segments(&assistant_buffer);
                continue;
            }

            let tail = parse_segments(&chunk);

            if let Some(check) = last_of(&tail, Role::DoubleCheck)
                && let Some(score) = parse_double_check_score(&check.content)
                && score < self.config.double_check_threshold
            {
                if replans < self.config.max_replans {
                    replans += 1;
                    assistant_buffer.push_str(
                        "\n<think>The verification score is below threshold. \
                         I will re-plan and improve the reply.</think>\n",
                    );
                    info!(score, replans, "CoA re-planning");
                    continue;
                }
                if let Some(suggested) = last_of(&tail, Role::SuggestedAnswer) {
                    return Ok(CoAResult {
                        answer: suggested.content.trim().to_string(),
                        trajectory,
                        steps,
                        replans,
                    });
                }
            }

            let has_suggested = last_of(&tail, Role::SuggestedAnswer).is_some();
            let has_answer = last_of(&tail, Role::Answer).is_some();
            let has_check = last_of(&tail, Role::DoubleCheck).is_some();
            if has_suggested && !has_answer && !has_check {
                continue;
            }

            if chunk.trim().is_empty() {
                break;
            }
        }

        match recover_answer(&trajectory) {
            Some(answer) => Ok(CoAResult { answer, trajectory, steps, replans }),
            None => anyhow::bail!("CoA produced no answer after {steps} step(s)"),
        }
    }

    async fn run_tool(&self, action: &Segment, ctx: &ToolContext) -> String {
        match self.tools.get(action.role.tag()) {
            Some(agent) => agent.run(&action.content, ctx).await,
            None => format!("Error: tool agent '{}' is not available.", action.role.tag()),
        }
    }

    async fn complete(
        &self,
        context_block: &str,
        buffer: &str,
        cfg: &GenerationConfig,
    ) -> anyhow::Result<String> {
        let (history, prompt) = if buffer.is_empty() {
            (Vec::new(), RigMessage::user(context_block.to_string()))
        } else {
            (
                vec![
                    RigMessage::user(context_block.to_string()),
                    RigMessage::assistant(buffer.to_string()),
                ],
                RigMessage::user(CONTINUE_NUDGE),
            )
        };

        let mut params = json!({ "top_p": cfg.top_p, "reasoning_effort": "none" });
        if !cfg.stop.is_empty() {
            params["stop"] = json!(cfg.stop);
        }

        let mut builder = self
            .model
            .completion_request(prompt)
            .preamble(COA_SYSTEM_PROMPT.to_string())
            .temperature(f64::from(cfg.temperature))
            .max_tokens(u64::from(cfg.max_tokens))
            .additional_params(params);
        if !history.is_empty() {
            builder = builder.messages(history);
        }

        let response = self
            .model
            .completion(builder.build())
            .await
            .map_err(|e| anyhow::anyhow!("groq completion failed: {e}"))?;

        let mut out = String::new();
        for item in response.choice {
            if let rig::completion::AssistantContent::Text(text) = item {
                out.push_str(&text.text);
            }
        }
        Ok(reattach_stop(&out, &cfg.stop))
    }
}

struct GenerationConfig {
    temperature: f32,
    top_p: f32,
    max_tokens: u32,
    stop: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
struct TurnRequest<'a> {
    user_input: &'a str,
    conversation_history: &'a str,
    user_profile: &'a str,
    conversation_id: &'a str,
    tool_context: &'a ToolContext,
}

fn reattach_stop(text: &str, stops: &[String]) -> String {
    for stop in stops {
        let opening = stop.replace("</", "<");
        if text.contains(&opening) && !text.contains(stop.as_str()) {
            return format!("{text}{stop}");
        }
    }
    text.to_string()
}

fn last_of(segments: &[Segment], role: Role) -> Option<&Segment> {
    segments.iter().rev().find(|s| s.role == role)
}

fn recover_answer(trajectory: &Trajectory) -> Option<String> {
    if let Some(answer) = trajectory.answer() {
        return Some(answer.to_string());
    }
    trajectory
        .last_of(Role::SuggestedAnswer)
        .map(|s| s.content.trim().to_string())
}
