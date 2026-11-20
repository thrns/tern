use std::time::{Duration, Instant};

use tern_core::calc::{CalcResult, evaluate};

const DEFAULT_DEADLINE: Duration = Duration::from_millis(50);

fn result(input: &str) -> Option<CalcResult> {
    evaluate(input, DEFAULT_DEADLINE)
}

fn exact(display: &str) -> Option<CalcResult> {
    Some(CalcResult {
        display: display.to_string(),
        is_approximate: false,
    })
}

#[test]
fn empty_input_gives_none() {
    assert_eq!(result(""), None);
    assert_eq!(result("   "), None);
}

#[test]
fn word_without_digits_gives_none() {
    assert_eq!(result("hello"), None);
}

#[test]
fn bare_constant_e_is_evaluated() {
    let outcome = result("e").expect("e should evaluate");
    assert!(outcome.is_approximate);
    assert!(outcome.display.starts_with("2.718"));
}

#[test]
fn bare_constant_pi_is_approximate() {
    let outcome = result("pi").expect("pi should evaluate");
    assert!(outcome.is_approximate);
    assert!(outcome.display.starts_with("3.141"));
}

#[test]
fn addition_is_exact() {
    assert_eq!(result("2+2"), exact("4"));
}

#[test]
fn unit_conversion_is_approximate() {
    let outcome = result("5 km to mi").expect("5 km to mi should evaluate");
    assert!(outcome.is_approximate);
    assert!(outcome.display.ends_with(" mi"));
    assert!(outcome.display.starts_with("3.10"));
}

#[test]
fn temperature_conversion_is_exact() {
    assert_eq!(result("100 °C to °F"), exact("212 °F"));
}

#[test]
fn hex_to_decimal_is_exact() {
    assert_eq!(result("0xff to decimal"), exact("255"));
}

#[test]
fn division_by_zero_gives_none() {
    assert_eq!(result("1/0"), None);
}

#[test]
fn text_with_digits_gives_none() {
    assert_eq!(result("2024 report"), None);
}

#[test]
fn input_equal_to_result_gives_none() {
    assert_eq!(result("10"), None);
}

#[test]
fn input_over_limit_gives_none() {
    let long_input = format!("2+2{}+2", " ".repeat(497));
    assert_eq!(long_input.chars().count(), 502);
    assert_eq!(result(&long_input), None);
}

#[test]
fn input_at_limit_is_not_rejected_by_length() {
    let input = format!("2+2{}+2", " ".repeat(494));
    assert_eq!(input.chars().count(), 499);
    assert_eq!(result(&input), exact("6"));
}

#[test]
fn runaway_power_is_interrupted_within_budget() {
    let started = Instant::now();
    let outcome = evaluate("2^10000000", Duration::from_millis(50));
    let elapsed = started.elapsed();
    assert_eq!(outcome, None);
    assert!(
        elapsed < Duration::from_millis(100),
        "took {elapsed:?}, expected under 100 ms"
    );
}

#[test]
fn evaluation_is_fast_across_repeated_runs() {
    let inputs = [
        "2+2",
        "5 km to mi",
        "100 °C to °F",
        "0xff to decimal",
        "pi",
        "1/0",
    ];
    let mut durations = Vec::with_capacity(1000);
    for run in 0..1000 {
        let input = inputs[run % inputs.len()];
        let started = Instant::now();
        let _ = evaluate(input, DEFAULT_DEADLINE);
        durations.push(started.elapsed());
    }
    durations.sort();
    let p95 = durations[durations.len() * 95 / 100];
    assert!(
        p95 < Duration::from_millis(20),
        "p95 was {p95:?}, expected under 20 ms"
    );
}
