/// Score confidence based on the extraction rule's pattern strength and how
/// close the subject and object are in the source text (`gap`: chars between
/// the end of the subject mention and the start of the object mention).
///
/// Closer entities are less likely to be an accidental co-occurrence, so gap
/// distance scales a per-rule base score down as the gap widens.
pub fn score_confidence(rule: &str, gap: usize) -> f32 {
    let base = if rule.contains("possessive") {
        0.85
    } else if rule.contains("verb-mentor") || rule.contains("verb-works-at") {
        0.78
    } else if rule.contains("relative-") {
        0.70
    } else {
        0.6
    };

    // gap is already bounded to <= 30 by the caller; scale the last 15 chars
    // of that range down to a floor of 0.85x base, so a close match keeps the
    // full base score and a far one loses up to 15%.
    let proximity_penalty = (gap.saturating_sub(15) as f32 / 15.0).min(1.0) * 0.15;
    (base - base * proximity_penalty).clamp(0.0, 1.0)
}
