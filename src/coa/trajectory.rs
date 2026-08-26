use std::sync::LazyLock;

use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Plan,
    Think,
    Reflection,
    SuggestedAnswer,
    DoubleCheck,
    Answer,
    Observation,
    WebSearch,
    GrammarCheck,
    UpdateProfile,
    SaveCorrection,
}

impl Role {
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Role::Plan => "plan",
            Role::Think => "think",
            Role::Reflection => "reflection",
            Role::SuggestedAnswer => "suggested_answer",
            Role::DoubleCheck => "double_check",
            Role::Answer => "answer",
            Role::Observation => "observation",
            Role::WebSearch => "web_search",
            Role::GrammarCheck => "grammar_check",
            Role::UpdateProfile => "update_profile",
            Role::SaveCorrection => "save_correction",
        }
    }

    #[must_use]
    pub fn from_tag(name: &str) -> Option<Self> {
        let role = match name {
            "plan" => Role::Plan,
            "think" => Role::Think,
            "reflection" => Role::Reflection,
            "suggested_answer" => Role::SuggestedAnswer,
            "double_check" => Role::DoubleCheck,
            "answer" => Role::Answer,
            "observation" => Role::Observation,
            "web_search" => Role::WebSearch,
            "grammar_check" => Role::GrammarCheck,
            "update_profile" => Role::UpdateProfile,
            "save_correction" => Role::SaveCorrection,
            _ => return None,
        };
        Some(role)
    }

    #[must_use]
    pub const fn is_tool_agent(self) -> bool {
        matches!(
            self,
            Role::WebSearch | Role::GrammarCheck | Role::UpdateProfile | Role::SaveCorrection
        )
    }

    #[must_use]
    pub fn closing_tag(self) -> String {
        format!("</{}>", self.tag())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub role: Role,
    pub content: String,
}

impl Segment {
    #[must_use]
    pub const fn is_tool_call(&self) -> bool {
        self.role.is_tool_agent()
    }

    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self.role, Role::Answer)
    }
}

static OPEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)<(?P<name>plan|think|reflection|suggested_answer|double_check|answer|observation|web_search|grammar_check|update_profile|save_correction)\s*>",
    )
    .expect("static trajectory regex is valid")
});

static SCORE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)score\s*(?:this time\s*)?is\s*[:：]?\s*(\d+)")
        .expect("static score regex is valid")
});

#[must_use]
pub fn parse_segments(text: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut pos = 0usize;

    while let Some(caps) = OPEN_RE.captures_at(text, pos) {
        let whole = caps.get(0).expect("group 0 always present");
        let name = caps.name("name").expect("named group present").as_str();
        let name_lower = name.to_ascii_lowercase();
        let Some(role) = Role::from_tag(&name_lower) else {
            pos = whole.end();
            continue;
        };

        let body_start = whole.end();
        let closing = role.closing_tag();
        let close_idx = text[body_start..]
            .find(closing.as_str())
            .map(|i| body_start + i);
        let next_open = OPEN_RE.find_at(text, body_start).map(|nm| nm.start());

        let (content_range, raw_end) = match (close_idx, next_open) {
            (Some(ci), no) if no.is_none_or(|n| ci <= n) => {
                (body_start..ci, ci + closing.len())
            }
            (_, Some(no)) => (body_start..no, no),
            (_, None) => (body_start..text.len(), text.len()),
        };

        let content = text[content_range].trim().to_string();
        segments.push(Segment { role, content });
        pos = raw_end;
    }

    segments
}

#[must_use]
pub fn first_actionable_segment(text: &str) -> Option<Segment> {
    parse_segments(text)
        .into_iter()
        .find(|s| s.is_tool_call() || s.is_terminal())
}

#[must_use]
pub fn parse_double_check_score(text: &str) -> Option<i32> {
    SCORE_RE
        .captures(text)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse().ok())
}

#[must_use]
pub fn tool_stop_sequences() -> Vec<String> {
    [
        Role::WebSearch,
        Role::GrammarCheck,
        Role::UpdateProfile,
        Role::SaveCorrection,
    ]
    .into_iter()
    .map(Role::closing_tag)
    .collect()
}

#[derive(Debug, Default, Clone)]
pub struct Trajectory {
    pub segments: Vec<Segment>,
}

impl Trajectory {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn answer(&self) -> Option<&str> {
        self.segments
            .iter()
            .rev()
            .find(|s| s.role == Role::Answer)
            .map(|s| s.content.as_str())
    }

    #[must_use]
    pub fn n_hops(&self) -> usize {
        self.segments
            .iter()
            .filter(|s| s.role != Role::Observation)
            .count()
    }

    #[must_use]
    pub fn last_of(&self, role: Role) -> Option<&Segment> {
        self.segments.iter().rev().find(|s| s.role == role)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ordered_segments() {
        let stream = concat!(
            "<plan>1. greet 2. answer</plan>",
            "<think>simple turn</think>",
            "<suggested_answer>Hi there!</suggested_answer>",
            "<double_check>The score this time is:4</double_check>",
            "<answer>Hi there! How's your day going?</answer>",
        );
        let roles: Vec<Role> = parse_segments(stream).iter().map(|s| s.role).collect();
        assert_eq!(
            roles,
            vec![
                Role::Plan,
                Role::Think,
                Role::SuggestedAnswer,
                Role::DoubleCheck,
                Role::Answer,
            ]
        );
        let answer = parse_segments(stream)
            .into_iter()
            .find(|s| s.role == Role::Answer)
            .map(|s| s.content);
        assert_eq!(answer.as_deref(), Some("Hi there! How's your day going?"));
        assert_eq!(parse_double_check_score("The score this time is:4"), Some(4));
        assert_eq!(parse_double_check_score("score is: 2"), Some(2));
    }

    #[test]
    fn handles_missing_closing_tag() {
        let segs = parse_segments("<think>reasoning without close");
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].role, Role::Think);
        assert_eq!(segs[0].content, "reasoning without close");
    }

    #[test]
    fn tool_stops_are_closing_tags() {
        let stops = tool_stop_sequences();
        assert!(stops.contains(&"</grammar_check>".to_string()));
        assert!(stops.contains(&"</save_correction>".to_string()));
        assert_eq!(stops.len(), 4);
    }
}
