/// Rule families share calibration evidence for similar extraction patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CalibrationGroup {
    DialogueTurn,
    DialogueVocative,
    Nominal,
    Phrase,
    Verb,
}

impl CalibrationGroup {
    pub fn for_rule(rule: &str) -> Option<Self> {
        let group = match rule {
            "dialogue-turn" => Self::DialogueTurn,
            "dialogue-vocative" => Self::DialogueVocative,
            _ if rule.starts_with("phrase-") => Self::Phrase,
            _ if rule.starts_with("verb-") || rule.starts_with("relative-") => Self::Verb,
            _ if [
                "possessive-",
                "kin-prefix-",
                "of-genitive-",
                "pronoun-",
                "plural-",
            ]
            .iter()
            .any(|prefix| rule.starts_with(prefix)) =>
            {
                Self::Nominal
            }
            _ => return None,
        };
        Some(group)
    }
}

/// Correctness counts for a family of rules on public fiction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleCalibration {
    pub correct: u32,
    pub observations: u32,
}

impl RuleCalibration {
    /// The posterior mean with four prior observations centered on the heuristic.
    pub fn estimate(self, heuristic: f32) -> f32 {
        const PRIOR_WEIGHT: f32 = 4.0;
        (self.correct as f32 + PRIOR_WEIGHT * heuristic) / (self.observations as f32 + PRIOR_WEIGHT)
    }
}

/// Calibration evidence for the rule's family in the public sample.
pub fn rule_calibration(rule: &str) -> Option<RuleCalibration> {
    let (correct, observations) = match CalibrationGroup::for_rule(rule)? {
        CalibrationGroup::DialogueTurn => (9, 23),
        CalibrationGroup::DialogueVocative => (10, 10),
        CalibrationGroup::Nominal => (7, 10),
        CalibrationGroup::Phrase => (15, 22),
        _ => return None,
    };
    Some(RuleCalibration {
        correct,
        observations,
    })
}
