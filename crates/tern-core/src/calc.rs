use std::time::{Duration, Instant};

const MAX_INPUT_CHARS: usize = 500;
const BARE_CONSTANTS: [&str; 3] = ["pi", "tau", "e"];
const APPROXIMATE_PREFIX: &str = "approx.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalcResult {
    pub display: String,
    pub is_approximate: bool,
}

struct Deadline {
    until: Instant,
}

impl fend_core::Interrupt for Deadline {
    fn should_interrupt(&self) -> bool {
        Instant::now() >= self.until
    }
}

pub fn evaluate(input: &str, deadline: Duration) -> Option<CalcResult> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_INPUT_CHARS {
        return None;
    }
    let is_bare_constant = BARE_CONSTANTS.contains(&trimmed);
    if !is_bare_constant && !trimmed.bytes().any(|byte| byte.is_ascii_digit()) {
        return None;
    }

    let until = Instant::now().checked_add(deadline)?;
    let interrupt = Deadline { until };
    let mut context = fend_core::Context::new();
    let outcome = fend_core::evaluate_with_interrupt(trimmed, &mut context, &interrupt).ok()?;

    let main = outcome.get_main_result().trim();
    if main.is_empty() || main == trimmed {
        return None;
    }

    match main.strip_prefix(APPROXIMATE_PREFIX) {
        Some(rest) => Some(CalcResult {
            display: rest.trim_start().to_string(),
            is_approximate: true,
        }),
        None => Some(CalcResult {
            display: main.to_string(),
            is_approximate: false,
        }),
    }
}
