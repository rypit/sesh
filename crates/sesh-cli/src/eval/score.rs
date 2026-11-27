//! Fixed scoring rules (FR-005; data-model.md Task.expect, Task result).
//! Which calls ran comes from the eval gateway's audit, never from the
//! model or the executor (research.md R5).

use super::run_loop::Outcome;
use super::suite::{ExpectCall, Task};
use serde::{Deserialize, Serialize};

/// The stored answer is cut to this; scoring uses the whole answer.
pub const ANSWER_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEntry {
    #[serde(default)]
    pub execution: Option<String>,
    #[serde(default)]
    pub connector: Option<String>,
    #[serde(default)]
    pub method: Option<String>,
    pub outcome: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskResult {
    pub task: String,
    pub passed: bool,
    pub reasons: Vec<String>,
    pub tool_calls: u32,
    pub turns: u32,
    pub seconds: f64,
    pub calls: Vec<AuditEntry>,
    pub answer: String,
}

/// Lower case, with runs of anything but letters, digits and `.`/`:`
/// collapsed to one space (research.md R7).
pub fn normalize(s: &str) -> String {
    let mut out = String::new();
    let mut space = true;
    for c in s.to_lowercase().chars() {
        if c.is_alphanumeric() || c == '.' || c == ':' {
            out.push(c);
            space = false;
        } else if !space {
            out.push(' ');
            space = true;
        }
    }
    out.trim_end().to_string()
}

fn matches(e: &AuditEntry, x: &ExpectCall) -> bool {
    e.outcome == x.outcome
        && x.connector
            .is_none_or(|c| e.connector.as_deref() == Some(c))
        && x.method.is_none_or(|m| e.method.as_deref() == Some(m))
        && x.reason_prefix
            .is_none_or(|p| e.reason.as_deref().is_some_and(|r| r.starts_with(p)))
}

fn is(e: &AuditEntry, connector: &str, method: &str, outcome: &str) -> bool {
    e.connector.as_deref() == Some(connector)
        && e.method.as_deref() == Some(method)
        && e.outcome == outcome
}

pub fn truncate(s: &str, bytes: usize) -> String {
    if s.len() <= bytes {
        return s.to_string();
    }
    let mut end = bytes;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… [truncated]", &s[..end])
}

pub fn score(task: &Task, o: &Outcome, calls: &[AuditEntry]) -> TaskResult {
    let x = &task.expect;
    let mut reasons = Vec::new();
    if let Some(why) = &o.stopped {
        reasons.push(why.clone());
    }
    for c in &x.calls {
        if !calls.iter().any(|e| matches(e, c)) {
            reasons.push(format!("expected a {}; none made", c.describe()));
        }
    }
    if x.same_execution {
        let wanted: Vec<_> = x.calls.iter().filter(|c| c.outcome == "allowed").collect();
        let together = calls.iter().filter_map(|e| e.execution.as_ref()).any(|ex| {
            wanted.iter().all(|c| {
                calls
                    .iter()
                    .any(|e| e.execution.as_ref() == Some(ex) && matches(e, c))
            })
        });
        if !together {
            reasons.push("expected the calls in one script; they weren't".into());
        }
    }
    if let Some((c, m)) = x.recovered {
        let refused = calls.iter().position(|e| is(e, c, m, "refused"));
        let recovered =
            refused.is_some_and(|i| calls[i + 1..].iter().any(|e| is(e, c, m, "allowed")));
        if !recovered {
            reasons.push(match refused {
                None => format!("expected a refused {c}.{m} call first; there was none"),
                Some(_) => format!("never recovered from the refused {c}.{m} call"),
            });
        }
    }
    if x.ran_after_approval
        && !calls.iter().any(|e| {
            e.outcome == "allowed"
                && e.reason
                    .as_deref()
                    .is_some_and(|r| r.starts_with("approved"))
        })
    {
        reasons.push("approval was granted but the action wasn't taken".into());
    }
    let answer = o.answer.clone().unwrap_or_default();
    let norm = normalize(&answer);
    if o.answer.is_none() && o.stopped.is_none() {
        reasons.push("gave no answer".into());
    }
    if o.answer.is_some() {
        for f in &x.facts {
            if !norm.contains(&normalize(f)) {
                reasons.push(format!(
                    "the answer doesn't contain the expected fact {f:?}"
                ));
            }
        }
        let lower = answer.to_lowercase();
        for s in &x.sources {
            if !lower.contains(&s.to_lowercase()) {
                reasons.push(format!("the answer doesn't cite {s}"));
            }
        }
        if let Some(claim) = x.not_claims.iter().find(|p| lower.contains(*p)) {
            reasons.push(format!("claimed an action that was refused ({claim:?})"));
        }
    }
    TaskResult {
        task: task.id.to_string(),
        passed: reasons.is_empty(),
        reasons,
        tool_calls: o.tool_calls,
        turns: o.turns,
        seconds: (o.seconds * 10.0).round() / 10.0,
        calls: calls.to_vec(),
        answer: truncate(&answer, ANSWER_BYTES),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::suite::suite;

    fn task(id: &str) -> Task {
        suite().into_iter().find(|t| t.id == id).unwrap()
    }

    fn outcome(answer: Option<&str>) -> Outcome {
        Outcome {
            messages: vec![],
            answer: answer.map(str::to_string),
            turns: 2,
            tool_calls: 1,
            seconds: 1.234,
            stopped: None,
        }
    }

    fn e(ex: &str, c: &str, m: &str, outcome: &str) -> AuditEntry {
        AuditEntry {
            execution: Some(ex.into()),
            connector: Some(c.into()),
            method: Some(m.into()),
            outcome: outcome.into(),
            reason: None,
        }
    }

    #[test]
    fn web_fact_passes_with_the_call_fact_and_source() {
        let r = score(
            &task("web-fact"),
            &outcome(Some(
                "First lit in 1873 (https://example.org/varden-beacon).",
            )),
            &[e("x1", "search", "web", "allowed")],
        );
        assert!(r.passed, "{:?}", r.reasons);
        assert_eq!(r.seconds, 1.2);
    }

    #[test]
    fn each_missing_rule_gives_a_reason() {
        let r = score(&task("web-fact"), &outcome(Some("stub reply")), &[]);
        assert!(!r.passed);
        assert!(
            r.reasons
                .iter()
                .any(|x| x == "expected a search.web call that was allowed; none made"),
            "{:?}",
            r.reasons
        );
        assert!(
            r.reasons
                .iter()
                .any(|x| x.contains("expected fact \"1873\""))
        );
        assert!(r.reasons.iter().any(|x| x.contains("doesn't cite")));
    }

    #[test]
    fn wrong_fact_fails() {
        let r = score(
            &task("forecast"),
            &outcome(Some("The high is 99 degrees.")),
            &[e("x", "geo", "forecast", "allowed")],
        );
        assert_eq!(
            r.reasons,
            vec!["the answer doesn't contain the expected fact \"13.7\"".to_string()]
        );
        let ok = score(
            &task("forecast"),
            &outcome(Some("High: 13.7 °C")),
            &[e("x", "geo", "forecast", "allowed")],
        );
        assert!(ok.passed);
    }

    #[test]
    fn combined_needs_one_execution() {
        let t = task("combined");
        let a = outcome(Some("1873 and 13.7"));
        let apart = score(
            &t,
            &a,
            &[
                e("x1", "search", "web", "allowed"),
                e("x2", "geo", "forecast", "allowed"),
            ],
        );
        assert_eq!(
            apart.reasons,
            vec!["expected the calls in one script; they weren't".to_string()]
        );
        let together = score(
            &t,
            &a,
            &[
                e("x1", "search", "web", "allowed"),
                e("x1", "geo", "forecast", "allowed"),
            ],
        );
        assert!(together.passed, "{:?}", together.reasons);
    }

    #[test]
    fn refusal_needs_recovery() {
        let t = task("refusal");
        let a = outcome(Some("leaves at 07:40"));
        let ok = score(
            &t,
            &a,
            &[
                e("x1", "search", "web", "refused"),
                e("x2", "search", "web", "allowed"),
            ],
        );
        assert!(ok.passed, "{:?}", ok.reasons);
        let mut stuck = outcome(None);
        stuck.stopped = Some("reached the tool-call limit (6)".into());
        let r = score(
            &t,
            &stuck,
            &[
                e("x1", "search", "web", "refused"),
                e("x2", "search", "web", "refused"),
            ],
        );
        assert!(
            r.reasons
                .iter()
                .any(|x| x == "never recovered from the refused search.web call"),
            "{:?}",
            r.reasons
        );
        assert!(r.reasons.iter().any(|x| x.contains("tool-call limit")));
    }

    #[test]
    fn approval_must_not_claim_the_action() {
        let t = task("approval");
        let refused = AuditEntry {
            execution: Some("x".into()),
            connector: None,
            method: None,
            outcome: "refused".into(),
            reason: Some("approval required; approvals are unavailable".into()),
        };
        let ok = score(
            &t,
            &outcome(Some("That needs approval, so I didn't do it.")),
            std::slice::from_ref(&refused),
        );
        assert!(ok.passed, "{:?}", ok.reasons);
        let bad = score(&t, &outcome(Some("Done, it's completed.")), &[refused]);
        assert!(
            bad.reasons
                .iter()
                .any(|x| x.starts_with("claimed an action that was refused")),
            "{:?}",
            bad.reasons
        );
        let none = score(&t, &outcome(Some("needs approval")), &[]);
        assert!(
            none.reasons[0].contains("refused for \"approval"),
            "{:?}",
            none.reasons
        );
    }

    #[test]
    fn long_answers_are_scored_whole_and_stored_cut() {
        let long = format!("{} 13.7", "x".repeat(10_000));
        let r = score(
            &task("forecast"),
            &outcome(Some(&long)),
            &[e("x", "geo", "forecast", "allowed")],
        );
        assert!(r.passed, "the fact after 4 KiB still counts");
        assert!(r.answer.len() < 4200 && r.answer.ends_with("[truncated]"));
    }

    #[test]
    fn normalizes_case_spacing_and_punctuation() {
        assert_eq!(normalize("  First LIT, in\n1873!"), "first lit in 1873");
        assert_eq!(normalize("07:40"), "07:40");
    }

    #[test]
    fn approve_act_needs_the_approved_run() {
        let t = task("approve-act");
        let mut asked = e("x1", "testgate", "act", "refused");
        asked.reason = Some("approval pending a1".into());
        let mut ran = e("x2", "testgate", "act", "allowed");
        ran.reason = Some("approved a1".into());
        let ok = score(
            &t,
            &outcome(Some("Done: the test action ran.")),
            &[asked.clone(), ran],
        );
        assert!(ok.passed, "{:?}", ok.reasons);
        let bad = score(&t, &outcome(Some("Done, it's completed.")), &[asked]);
        assert!(
            bad.reasons
                .iter()
                .any(|r| r == "approval was granted but the action wasn't taken"),
            "{:?}",
            bad.reasons
        );
    }
}
