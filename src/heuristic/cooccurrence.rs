/// Base confidence per pattern family, before proximity decay.
pub mod base {
    /// A kinship or role noun in a possessive states the relation outright.
    pub const POSSESSIVE_FACTUAL: f32 = 0.85;

    /// A social-stance noun ("friend", "enemy", "rival") in the same
    /// possessive shape. IMPORTANT: the phrasing is as explicit, but the claim
    /// is not. Stance is routinely negated, hypothetical, or narrated from a
    /// character's mistaken view, so it does not earn the factual base.
    pub const POSSESSIVE_STANCE: f32 = 0.70;

    /// A direct verb between the two mentions.
    pub const VERB: f32 = 0.78;

    /// A relative clause, which puts more text between the mentions and more
    /// ways for the attachment to be wrong.
    pub const RELATIVE_CLAUSE: f32 = 0.70;
}

/// Scale a pattern's base confidence by how close the subject and object are
/// in the source text (`gap`: chars between the end of the subject mention and
/// the start of the object mention).
///
/// Closer entities are less likely to be an accidental co-occurrence, so gap
/// distance scales the base score down as the gap widens.
pub fn score_confidence(base: f32, gap: usize) -> f32 {
    // gap is already bounded to <= 30 by the caller; scale the last 15 chars
    // of that range down to a floor of 0.85x base, so a close match keeps the
    // full base score and a far one loses up to 15%.
    let proximity_penalty = (gap.saturating_sub(15) as f32 / 15.0).min(1.0) * 0.15;
    (base - base * proximity_penalty).clamp(0.0, 1.0)
}
