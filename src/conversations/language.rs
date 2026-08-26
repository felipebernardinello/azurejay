use whatlang::Lang;

pub const NON_ENGLISH_REPLY: &str = "It looks like you're writing in another language! \
     Since we're here to practice English, could you try saying that in English? \
     Don't worry about making mistakes, that's how we learn.";

#[must_use]
pub fn is_confidently_non_english(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.split_whitespace().count() < 3 || trimmed.chars().count() < 12 {
        return false;
    }
    match whatlang::detect(trimmed) {
        Some(info) => info.lang() != Lang::Eng && info.is_reliable() && info.confidence() > 0.9,
        None => false,
    }
}
