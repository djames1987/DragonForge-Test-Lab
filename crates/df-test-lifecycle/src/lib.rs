use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MAX_ATTEMPTS: u32 = 5;
pub const MAX_BASE_DELAY_SECS: u64 = 3_600;
pub const MAX_DELAY_SECS: u64 = 86_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    TestFailure,
    InfrastructureTransient,
    InfrastructurePermanent,
    PolicyRejected,
    Cancelled,
    Interrupted,
}

impl FailureClass {
    pub fn is_retryable(self) -> bool {
        matches!(
            self,
            Self::InfrastructureTransient | Self::Interrupted
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay_secs: u64,
    pub max_delay_secs: u64,
    pub retry_interrupted: bool,
}

impl RetryPolicy {
    pub const fn no_retry() -> Self {
        Self {
            max_attempts: 1,
            base_delay_secs: 1,
            max_delay_secs: 1,
            retry_interrupted: false,
        }
    }

    pub fn bounded(
        max_attempts: u32,
        base_delay_secs: u64,
        max_delay_secs: u64,
        retry_interrupted: bool,
    ) -> Result<Self, LifecycleError> {
        let policy = Self {
            max_attempts,
            base_delay_secs,
            max_delay_secs,
            retry_interrupted,
        };
        policy.validate()?;
        Ok(policy)
    }

    pub fn validate(self) -> Result<(), LifecycleError> {
        if self.max_attempts == 0 || self.max_attempts > MAX_ATTEMPTS {
            return Err(LifecycleError::InvalidAttemptLimit);
        }
        if self.base_delay_secs == 0 || self.base_delay_secs > MAX_BASE_DELAY_SECS {
            return Err(LifecycleError::InvalidBaseDelay);
        }
        if self.max_delay_secs == 0
            || self.max_delay_secs > MAX_DELAY_SECS
            || self.max_delay_secs < self.base_delay_secs
        {
            return Err(LifecycleError::InvalidMaxDelay);
        }
        Ok(())
    }

    pub fn permits_retry(self, class: FailureClass, attempt_number: u32) -> bool {
        if attempt_number >= self.max_attempts {
            return false;
        }
        match class {
            FailureClass::Interrupted => self.retry_interrupted,
            _ => class.is_retryable(),
        }
    }

    pub fn retry_delay_secs(self, attempt_number: u32) -> Result<u64, LifecycleError> {
        self.validate()?;
        if attempt_number == 0 {
            return Err(LifecycleError::InvalidAttemptNumber);
        }
        let exponent = attempt_number.saturating_sub(1).min(20);
        let multiplier = 1u64.checked_shl(exponent).unwrap_or(u64::MAX);
        Ok(self
            .base_delay_secs
            .saturating_mul(multiplier)
            .min(self.max_delay_secs))
    }

    pub fn next_retry_at(
        self,
        now_secs: u64,
        attempt_number: u32,
    ) -> Result<u64, LifecycleError> {
        Ok(now_secs.saturating_add(self.retry_delay_secs(attempt_number)?))
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::no_retry()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleDecision {
    TerminalPassed,
    TerminalFailed,
    TerminalRejected,
    TerminalCancelled,
    RetryScheduled { next_retry_at_secs: u64 },
    RetryExhausted,
    InterruptedAwaitingDecision,
}

pub fn decide_failure(
    policy: RetryPolicy,
    class: FailureClass,
    attempt_number: u32,
    now_secs: u64,
) -> Result<LifecycleDecision, LifecycleError> {
    policy.validate()?;
    if attempt_number == 0 {
        return Err(LifecycleError::InvalidAttemptNumber);
    }

    if policy.permits_retry(class, attempt_number) {
        return Ok(LifecycleDecision::RetryScheduled {
            next_retry_at_secs: policy.next_retry_at(now_secs, attempt_number)?,
        });
    }

    if class == FailureClass::Interrupted && !policy.retry_interrupted {
        return Ok(LifecycleDecision::InterruptedAwaitingDecision);
    }

    if class.is_retryable() && attempt_number >= policy.max_attempts {
        return Ok(LifecycleDecision::RetryExhausted);
    }

    Ok(match class {
        FailureClass::PolicyRejected => LifecycleDecision::TerminalRejected,
        FailureClass::Cancelled => LifecycleDecision::TerminalCancelled,
        _ => LifecycleDecision::TerminalFailed,
    })
}

#[derive(Debug, Error)]
pub enum LifecycleError {
    #[error("max attempts must be between 1 and 5")]
    InvalidAttemptLimit,
    #[error("base retry delay must be between 1 and 3600 seconds")]
    InvalidBaseDelay,
    #[error("max retry delay is invalid")]
    InvalidMaxDelay,
    #[error("attempt number must start at 1")]
    InvalidAttemptNumber,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_failures_are_never_retried() {
        let policy = RetryPolicy::bounded(3, 10, 60, true).unwrap();
        assert_eq!(
            decide_failure(policy, FailureClass::TestFailure, 1, 100).unwrap(),
            LifecycleDecision::TerminalFailed
        );
    }

    #[test]
    fn transient_infrastructure_failures_back_off_and_stop() {
        let policy = RetryPolicy::bounded(3, 10, 60, false).unwrap();
        assert_eq!(
            decide_failure(
                policy,
                FailureClass::InfrastructureTransient,
                1,
                100
            )
            .unwrap(),
            LifecycleDecision::RetryScheduled {
                next_retry_at_secs: 110
            }
        );
        assert_eq!(
            decide_failure(
                policy,
                FailureClass::InfrastructureTransient,
                2,
                100
            )
            .unwrap(),
            LifecycleDecision::RetryScheduled {
                next_retry_at_secs: 120
            }
        );
        assert_eq!(
            decide_failure(
                policy,
                FailureClass::InfrastructureTransient,
                3,
                100
            )
            .unwrap(),
            LifecycleDecision::RetryExhausted
        );
    }

    #[test]
    fn interrupted_requires_explicit_policy_opt_in() {
        let no_retry = RetryPolicy::bounded(3, 5, 30, false).unwrap();
        assert_eq!(
            decide_failure(no_retry, FailureClass::Interrupted, 1, 100).unwrap(),
            LifecycleDecision::InterruptedAwaitingDecision
        );

        let retry = RetryPolicy::bounded(3, 5, 30, true).unwrap();
        assert_eq!(
            decide_failure(retry, FailureClass::Interrupted, 1, 100).unwrap(),
            LifecycleDecision::RetryScheduled {
                next_retry_at_secs: 105
            }
        );
    }

    #[test]
    fn retry_policy_is_bounded() {
        assert!(RetryPolicy::bounded(6, 1, 10, true).is_err());
        assert!(RetryPolicy::bounded(2, 0, 10, true).is_err());
        assert!(RetryPolicy::bounded(2, 10, 5, true).is_err());
        assert!(RetryPolicy::bounded(2, 10, MAX_DELAY_SECS + 1, true).is_err());
    }
}
