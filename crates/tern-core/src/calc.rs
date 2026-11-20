use std::thread;
use std::time::{Duration, Instant};

const MAX_INPUT_CHARS: usize = 500;
const BARE_CONSTANTS: [&str; 3] = ["pi", "tau", "e"];
const APPROXIMATE_PREFIX: &str = "approx.";
const EVAL_THREAD_NAME: &str = "tern-calc";
const EVAL_STACK_BYTES: usize = 8 * 1024 * 1024;
const MAX_NESTING_DEPTH: usize = 50;
const MAX_OPERATORS: usize = 150;
const OPERATOR_CHARS: [char; 6] = ['+', '-', '*', '/', '^', '%'];

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
    if nesting_depth(trimmed) > MAX_NESTING_DEPTH || operator_count(trimmed) > MAX_OPERATORS {
        return None;
    }

    let until = Instant::now().checked_add(deadline)?;
    let owned = trimmed.to_string();
    let worker = thread::Builder::new()
        .name(EVAL_THREAD_NAME.into())
        .stack_size(EVAL_STACK_BYTES)
        .spawn(move || run_fend(&owned, until))
        .ok()?;
    worker.join().ok().flatten()
}

fn nesting_depth(input: &str) -> usize {
    let mut depth = 0usize;
    let mut deepest = 0usize;
    for character in input.chars() {
        match character {
            '(' => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    deepest
}

fn operator_count(input: &str) -> usize {
    input
        .chars()
        .filter(|character| OPERATOR_CHARS.contains(character))
        .count()
}

fn run_fend(trimmed: &str, until: Instant) -> Option<CalcResult> {
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
