use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const RETRY_AFTER_SECS: u64 = 1;

#[derive(Debug, Clone, Copy)]
pub struct DispatcherOverloaded {
    pub retry_after_secs: u64,
}

#[derive(Clone)]
pub struct DispatcherSemaphore {
    enabled: bool,
    permits: Arc<Semaphore>,
    queued: Arc<AtomicUsize>,
    max_queued: usize,
}

pub struct DispatcherReservation {
    kind: Option<DispatcherReservationKind>,
}

enum DispatcherReservationKind {
    Disabled,
    Acquired(OwnedSemaphorePermit),
    Queued {
        permits: Arc<Semaphore>,
        queued: Arc<AtomicUsize>,
    },
}

impl DispatcherSemaphore {
    pub fn new(enabled: bool, concurrency: usize, max_queued: usize) -> Self {
        Self {
            enabled,
            permits: Arc::new(Semaphore::new(concurrency.max(1))),
            queued: Arc::new(AtomicUsize::new(0)),
            max_queued,
        }
    }

    pub fn spawn<F>(&self, label: &'static str, future: F) -> Result<(), DispatcherOverloaded>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.reserve()?.spawn(label, future);
        Ok(())
    }

    pub fn reserve(&self) -> Result<DispatcherReservation, DispatcherOverloaded> {
        if !self.enabled {
            return Ok(DispatcherReservation {
                kind: Some(DispatcherReservationKind::Disabled),
            });
        }

        match self.permits.clone().try_acquire_owned() {
            Ok(permit) => Ok(DispatcherReservation {
                kind: Some(DispatcherReservationKind::Acquired(permit)),
            }),
            Err(tokio::sync::TryAcquireError::NoPermits) => {
                self.reserve_queued_slot()?;
                Ok(DispatcherReservation {
                    kind: Some(DispatcherReservationKind::Queued {
                        permits: self.permits.clone(),
                        queued: self.queued.clone(),
                    }),
                })
            }
            Err(tokio::sync::TryAcquireError::Closed) => Err(DispatcherOverloaded {
                retry_after_secs: RETRY_AFTER_SECS,
            }),
        }
    }

    pub fn reserve_many(
        &self,
        count: usize,
    ) -> Result<Vec<DispatcherReservation>, DispatcherOverloaded> {
        let mut reservations = Vec::with_capacity(count);
        for _ in 0..count {
            reservations.push(self.reserve()?);
        }
        Ok(reservations)
    }

    fn reserve_queued_slot(&self) -> Result<(), DispatcherOverloaded> {
        self.queued
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                (current < self.max_queued).then_some(current + 1)
            })
            .map(|_| ())
            .map_err(|_| DispatcherOverloaded {
                retry_after_secs: RETRY_AFTER_SECS,
            })
    }
}

impl DispatcherReservation {
    pub fn spawn<F>(mut self, label: &'static str, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        match self.kind.take().expect("reservation already consumed") {
            DispatcherReservationKind::Disabled => {
                tokio::spawn(future);
            }
            DispatcherReservationKind::Acquired(permit) => {
                tokio::spawn(run_with_permit(label, future, permit));
            }
            DispatcherReservationKind::Queued { permits, queued } => {
                tokio::spawn(async move {
                    let permit = permits.acquire_owned().await;
                    queued.fetch_sub(1, Ordering::AcqRel);
                    match permit {
                        Ok(permit) => run_with_permit(label, future, permit).await,
                        Err(_) => {
                            tracing::warn!(label, "Dispatcher semaphore closed before dispatch")
                        }
                    }
                });
            }
        }
    }
}

impl Drop for DispatcherReservation {
    fn drop(&mut self) {
        if let Some(DispatcherReservationKind::Queued { queued, .. }) = &self.kind {
            queued.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

async fn run_with_permit<F>(label: &'static str, future: F, _permit: OwnedSemaphorePermit)
where
    F: Future<Output = ()>,
{
    tracing::debug!(label, "Dispatcher permit acquired");
    future.await;
}

impl Default for DispatcherSemaphore {
    fn default() -> Self {
        // `tokio::sync::Semaphore::new` panics for permit counts above
        // `MAX_PERMITS` (~2^61); `usize::MAX` would trip that. We want a
        // disabled semaphore with effectively unbounded capacity for test
        // fixtures, so use a large-but-safe constant instead.
        Self::new(false, 1 << 32, 1 << 32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_when_queue_is_full() {
        let dispatcher = DispatcherSemaphore::new(true, 1, 0);
        let first = dispatcher.permits.clone().try_acquire_owned().unwrap();

        let result = dispatcher.reserve();

        assert!(result.is_err());
        drop(first);
    }

    #[test]
    fn accepts_when_disabled() {
        let dispatcher = DispatcherSemaphore::new(false, 1, 0);
        assert!(dispatcher.reserve().is_ok());
    }

    #[test]
    fn dropping_queued_reservation_releases_slot() {
        let dispatcher = DispatcherSemaphore::new(true, 1, 1);
        let first = dispatcher.permits.clone().try_acquire_owned().unwrap();
        let queued = dispatcher.reserve().unwrap();

        assert!(dispatcher.reserve().is_err());
        drop(queued);
        assert!(dispatcher.reserve().is_ok());
        drop(first);
    }
}
