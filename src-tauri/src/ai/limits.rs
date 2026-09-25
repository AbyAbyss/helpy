//! Retry, backoff and fallback rules for one step (one model call). Every
//! model call in Helpy goes through `run_step`; the limits are enforced here,
//! not by asking the model to behave.
//!
//! Rules:
//! - At most `1 + max_retries` attempts per step, counting fallback models.
//! - Errors that can succeed next time (timeouts, network, rate limits,
//!   provider 5xx, garbled output) are retried with exponential backoff, or
//!   the provider's `retry-after` when it gives one.
//! - Errors specific to one model (bad key, no access, model not found,
//!   refusal) are never retried on that model; the next fallback gets a turn.
//! - Budget, cancellation and setup errors stop the step immediately.
//! - Each fallback model gets one attempt per step; after the chain is used
//!   up, remaining retries go back to the main model if it's still usable.
//! - The budget check runs before every attempt, retries included.

use std::future::Future;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::error::{Disposition, ErrorKind, ProviderError};
use crate::settings::schema::Limits;

#[derive(Clone, Debug)]
pub struct Policy {
    pub max_retries: u32,
    pub base: Duration,
    pub max: Duration,
}

impl From<&Limits> for Policy {
    fn from(l: &Limits) -> Self {
        Self {
            max_retries: l.max_retries,
            base: Duration::from_millis(l.backoff_base_ms as u64),
            max: Duration::from_millis(l.backoff_max_ms as u64),
        }
    }
}

impl Policy {
    /// Delay before the n-th retry (1-based): base, 2×base, 4×base…, capped.
    pub fn backoff(&self, retry: u32) -> Duration {
        let factor = 2u32.saturating_pow(retry.saturating_sub(1));
        self.base.saturating_mul(factor).min(self.max)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RetryNotice<C> {
    /// 1-based number of this retry.
    pub retry: u32,
    pub max_retries: u32,
    /// Why the previous attempt failed, in plain words.
    pub reason: String,
    pub wait: Duration,
    /// The model the retry will use.
    pub next: C,
}

#[derive(Clone, Debug)]
pub struct StepFailure {
    /// The error that ended the step.
    pub error: ProviderError,
    pub attempts: u32,
}

pub async fn run_step<C, T, A, Fut>(
    policy: &Policy,
    candidates: &[C],
    cancel: &CancellationToken,
    mut check_budget: impl FnMut(&C) -> Result<(), ProviderError>,
    mut attempt: A,
    mut on_retry: impl FnMut(RetryNotice<C>),
) -> Result<T, StepFailure>
where
    C: Clone,
    A: FnMut(&C) -> Fut,
    Fut: Future<Output = Result<T, ProviderError>>,
{
    assert!(!candidates.is_empty(), "run_step needs at least one model");
    let max_attempts = policy.max_retries + 1;
    let mut unusable = vec![false; candidates.len()];
    let mut tried = vec![false; candidates.len()];
    let mut next_fallback = 1;
    let mut current = 0;
    let mut attempts = 0;

    let fail = |error: ProviderError, attempts| Err(StepFailure { error, attempts });

    loop {
        if cancel.is_cancelled() {
            return fail(super::sse::cancelled(), attempts);
        }
        if let Err(e) = check_budget(&candidates[current]) {
            return fail(e, attempts);
        }
        attempts += 1;
        tried[current] = true;
        let error = match attempt(&candidates[current]).await {
            Ok(value) => return Ok(value),
            Err(e) => e,
        };

        let disposition = error.kind.disposition();
        if disposition == Disposition::Stop || attempts >= max_attempts {
            return fail(error, attempts);
        }
        if disposition == Disposition::TryNextModel {
            unusable[current] = true;
        }

        // Next model: an untried fallback first, then the main model again.
        let next = if next_fallback < candidates.len() {
            next_fallback += 1;
            Some(next_fallback - 1)
        } else if !unusable[0] {
            Some(0)
        } else {
            None
        };
        let Some(next) = next else {
            return fail(error, attempts);
        };

        // Wait only before going back to a model that already failed; a
        // fresh fallback is tried right away.
        let mut wait = Duration::ZERO;
        if tried[next] {
            wait = policy.backoff(attempts);
            if let ErrorKind::RateLimited {
                retry_after: Some(after),
            } = error.kind
            {
                if after > policy.max {
                    let message = format!(
                        "The provider asked Helpy to wait {} seconds, longer than the {}-second limit in settings",
                        after.as_secs(),
                        policy.max.as_secs()
                    );
                    return fail(ProviderError::new(error.kind.clone(), message), attempts);
                }
                wait = after;
            }
        }

        on_retry(RetryNotice {
            retry: attempts,
            max_retries: policy.max_retries,
            reason: error.kind.short().to_string(),
            wait,
            next: candidates[next].clone(),
        });
        if !wait.is_zero() {
            tokio::select! {
                _ = cancel.cancelled() => return fail(super::sse::cancelled(), attempts),
                _ = tokio::time::sleep(wait) => {}
            }
        }
        current = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use tokio::time::Instant;

    fn policy(max_retries: u32) -> Policy {
        Policy {
            max_retries,
            base: Duration::from_secs(2),
            max: Duration::from_secs(30),
        }
    }

    fn err(kind: ErrorKind) -> ProviderError {
        ProviderError::new(kind, "scripted")
    }

    /// Scripted results per model name.
    type Plans = std::collections::HashMap<&'static str, VecDeque<Result<&'static str, ErrorKind>>>;

    /// A provider that replays a script per model and records every call.
    #[derive(Clone, Default)]
    struct Script {
        plans: Arc<Mutex<Plans>>,
        calls: Arc<Mutex<Vec<&'static str>>>,
    }

    impl Script {
        fn plan(self, model: &'static str, results: Vec<Result<&'static str, ErrorKind>>) -> Self {
            self.plans.lock().unwrap().insert(model, results.into());
            self
        }
        fn calls(&self) -> Vec<&'static str> {
            self.calls.lock().unwrap().clone()
        }
        async fn call(&self, model: &'static str) -> Result<&'static str, ProviderError> {
            self.calls.lock().unwrap().push(model);
            let next = self
                .plans
                .lock()
                .unwrap()
                .get_mut(model)
                .and_then(|q| q.pop_front());
            // A model with no script left keeps timing out.
            next.unwrap_or(Err(ErrorKind::Timeout)).map_err(err)
        }
    }

    async fn run(
        p: &Policy,
        models: &[&'static str],
        s: &Script,
    ) -> (
        Result<&'static str, StepFailure>,
        Vec<RetryNotice<&'static str>>,
    ) {
        let notices = Arc::new(Mutex::new(Vec::new()));
        let n = notices.clone();
        let result = run_step(
            p,
            models,
            &CancellationToken::new(),
            |_| Ok(()),
            |m| {
                let s = s.clone();
                let m = *m;
                async move { s.call(m).await }
            },
            move |notice| n.lock().unwrap().push(notice),
        )
        .await;
        let notices = notices.lock().unwrap().clone();
        (result, notices)
    }

    #[tokio::test(start_paused = true)]
    async fn succeeds_after_transient_failures_with_exponential_backoff() {
        let s = Script::default().plan(
            "main",
            vec![
                Err(ErrorKind::Timeout),
                Err(ErrorKind::Server),
                Ok("answer"),
            ],
        );
        let start = Instant::now();
        let (r, notices) = run(&policy(3), &["main"], &s).await;
        assert_eq!(r.unwrap(), "answer");
        assert_eq!(s.calls().len(), 3);
        assert_eq!(
            notices.iter().map(|n| n.wait.as_secs()).collect::<Vec<_>>(),
            [2, 4]
        );
        assert_eq!(notices.iter().map(|n| n.retry).collect::<Vec<_>>(), [1, 2]);
        assert_eq!(start.elapsed(), Duration::from_secs(6));
    }

    #[tokio::test(start_paused = true)]
    async fn never_exceeds_max_retries_on_a_provider_that_always_fails() {
        for max in [0, 1, 3, 5] {
            let s = Script::default();
            let (r, _) = run(&policy(max), &["main"], &s).await;
            let f = r.unwrap_err();
            assert_eq!(s.calls().len() as u32, max + 1, "max_retries = {max}");
            assert_eq!(f.attempts, max + 1);
            assert_eq!(f.error.kind, ErrorKind::Timeout);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn backoff_is_capped_at_the_maximum() {
        let p = Policy {
            max_retries: 6,
            base: Duration::from_secs(2),
            max: Duration::from_secs(10),
        };
        let s = Script::default();
        let (_, notices) = run(&p, &["main"], &s).await;
        assert_eq!(
            notices.iter().map(|n| n.wait.as_secs()).collect::<Vec<_>>(),
            [2, 4, 8, 10, 10, 10]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn model_specific_errors_fail_immediately_without_a_fallback() {
        for kind in [
            ErrorKind::Auth,
            ErrorKind::Permission,
            ErrorKind::NotFound,
            ErrorKind::BadRequest,
            ErrorKind::Billing,
            ErrorKind::Refused,
        ] {
            let s = Script::default().plan("main", vec![Err(kind.clone())]);
            let (r, notices) = run(&policy(3), &["main"], &s).await;
            assert_eq!(r.unwrap_err().error.kind, kind);
            assert_eq!(s.calls(), ["main"], "{kind:?} must not be retried");
            assert!(notices.is_empty());
        }
    }

    #[tokio::test(start_paused = true)]
    async fn stop_errors_end_the_step_even_with_fallbacks() {
        for kind in [ErrorKind::Budget, ErrorKind::Cancelled, ErrorKind::Setup] {
            let s = Script::default().plan("main", vec![Err(kind.clone())]);
            let (r, _) = run(&policy(3), &["main", "backup"], &s).await;
            assert_eq!(r.unwrap_err().error.kind, kind);
            assert_eq!(s.calls(), ["main"]);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_bad_key_moves_on_to_the_fallback_without_waiting() {
        let s = Script::default()
            .plan("main", vec![Err(ErrorKind::Auth)])
            .plan("backup", vec![Ok("from backup")]);
        let start = Instant::now();
        let (r, notices) = run(&policy(3), &["main", "backup"], &s).await;
        assert_eq!(r.unwrap(), "from backup");
        assert_eq!(s.calls(), ["main", "backup"]);
        assert_eq!(notices[0].next, "backup");
        assert_eq!(start.elapsed(), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn fallback_attempts_count_toward_the_retry_limit() {
        let s = Script::default();
        let (r, _) = run(&policy(3), &["main", "f1", "f2", "f3", "f4", "f5"], &s).await;
        assert!(r.is_err());
        assert_eq!(s.calls(), ["main", "f1", "f2", "f3"]);
    }

    #[tokio::test(start_paused = true)]
    async fn each_fallback_gets_one_attempt_then_the_main_model_resumes() {
        let s = Script::default();
        let (_, notices) = run(&policy(4), &["main", "f1"], &s).await;
        assert_eq!(s.calls(), ["main", "f1", "main", "main", "main"]);
        // No wait before the fresh fallback; waits before going back to main.
        assert_eq!(notices[0].wait, Duration::ZERO);
        assert!(notices[1..].iter().all(|n| !n.wait.is_zero()));
    }

    #[tokio::test(start_paused = true)]
    async fn an_unusable_main_model_is_not_tried_again() {
        let s = Script::default().plan("main", vec![Err(ErrorKind::Auth)]);
        let (r, _) = run(&policy(5), &["main", "f1"], &s).await;
        assert!(r.is_err());
        assert_eq!(s.calls(), ["main", "f1"]);
    }

    #[tokio::test(start_paused = true)]
    async fn rate_limits_wait_for_retry_after() {
        let s = Script::default().plan(
            "main",
            vec![
                Err(ErrorKind::RateLimited {
                    retry_after: Some(Duration::from_secs(7)),
                }),
                Ok("ok"),
            ],
        );
        let start = Instant::now();
        let (r, notices) = run(&policy(3), &["main"], &s).await;
        assert_eq!(r.unwrap(), "ok");
        assert_eq!(notices[0].wait, Duration::from_secs(7));
        assert_eq!(start.elapsed(), Duration::from_secs(7));
    }

    #[tokio::test(start_paused = true)]
    async fn a_retry_after_longer_than_the_maximum_stops_instead_of_hanging() {
        let s = Script::default().plan(
            "main",
            vec![Err(ErrorKind::RateLimited {
                retry_after: Some(Duration::from_secs(3600)),
            })],
        );
        let (r, _) = run(&policy(3), &["main"], &s).await;
        assert!(r.unwrap_err().error.message.contains("3600 seconds"));
        assert_eq!(s.calls().len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn garbled_output_is_retried() {
        let s = Script::default().plan("main", vec![Err(ErrorKind::Malformed), Ok("fixed")]);
        let (r, _) = run(&policy(3), &["main"], &s).await;
        assert_eq!(r.unwrap(), "fixed");
    }

    #[tokio::test(start_paused = true)]
    async fn budget_is_checked_before_every_attempt_including_retries() {
        let s = Script::default();
        let checks = Arc::new(Mutex::new(0));
        let c = checks.clone();
        let r: Result<&str, _> = run_step(
            &policy(3),
            &["main"],
            &CancellationToken::new(),
            move |_| {
                let mut n = c.lock().unwrap();
                *n += 1;
                if *n > 2 {
                    Err(err(ErrorKind::Budget))
                } else {
                    Ok(())
                }
            },
            |m| {
                let s = s.clone();
                let m = *m;
                async move { s.call(m).await }
            },
            |_| {},
        )
        .await;
        let f = r.unwrap_err();
        assert_eq!(f.error.kind, ErrorKind::Budget);
        assert_eq!(s.calls().len(), 2, "the third attempt must not be sent");
        assert_eq!(*checks.lock().unwrap(), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn an_exhausted_budget_blocks_the_first_call() {
        let s = Script::default().plan("main", vec![Ok("never")]);
        let r: Result<&str, _> = run_step(
            &policy(3),
            &["main"],
            &CancellationToken::new(),
            |_| Err(err(ErrorKind::Budget)),
            |m| {
                let s = s.clone();
                let m = *m;
                async move { s.call(m).await }
            },
            |_| {},
        )
        .await;
        assert_eq!(r.unwrap_err().attempts, 0);
        assert!(s.calls().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn cancelling_during_backoff_stops_promptly() {
        let s = Script::default();
        let cancel = CancellationToken::new();
        let c = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            c.cancel();
        });
        let start = Instant::now();
        let r: Result<&str, _> = run_step(
            &policy(3),
            &["main"],
            &cancel,
            |_| Ok(()),
            |m| {
                let s = s.clone();
                let m = *m;
                async move { s.call(m).await }
            },
            |_| {},
        )
        .await;
        assert_eq!(r.unwrap_err().error.kind, ErrorKind::Cancelled);
        assert_eq!(s.calls().len(), 1);
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
