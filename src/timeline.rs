use crate::{FailureCode, SyncError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Timeline {
    submitted: u64,
    completed: u64,
    failure: Option<FailureCode>,
}

impl Timeline {
    pub(crate) const fn new() -> Self {
        Self {
            submitted: 0,
            completed: 0,
            failure: None,
        }
    }

    pub(crate) fn reserve_signal(&mut self) -> Result<u64, SyncError> {
        if let Some(code) = self.failure {
            return Err(SyncError::TimelineFailed(code));
        }

        self.submitted = self
            .submitted
            .checked_add(1)
            .ok_or(SyncError::CounterExhausted)?;

        Ok(self.submitted)
    }

    pub(crate) fn complete(&mut self, value: u64) -> Result<(), SyncError> {
        if let Some(code) = self.failure {
            return Err(SyncError::TimelineFailed(code));
        }
        if value > self.submitted {
            return Err(SyncError::ValueNotSubmitted);
        }
        if value < self.completed {
            return Err(SyncError::CompletionRegression);
        }

        self.completed = value;
        Ok(())
    }

    pub(crate) fn fail(&mut self, code: FailureCode) {
        self.failure = Some(code);
    }

    pub(crate) const fn submitted(&self) -> u64 {
        self.submitted
    }

    pub(crate) const fn completed(&self) -> u64 {
        self.completed
    }

    pub(crate) const fn failure(&self) -> Option<FailureCode> {
        self.failure
    }
}
