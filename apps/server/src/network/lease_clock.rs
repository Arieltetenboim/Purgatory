//! Local monotonic bound on character-lease and channel-generation authority.
//!
//! The database clock remains what other processes see. This process also
//! stops gameplay or admission when its own deadline passes, including while
//! a renewal is still waiting on the persistence worker. Time spent queued
//! before the reply counts: the deadline is the send instant plus the policy
//! expiry.

use std::future::Future;
use std::time::Duration;

use tokio::time::Instant;

/// Send instant plus the policy expiry. A reply that arrives at or after this
/// instant does not extend local authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalLeaseDeadline {
    expires_at: Instant,
    expiry: Duration,
}

impl LocalLeaseDeadline {
    pub fn from_request(sent_at: Instant, expiry: Duration) -> Self {
        Self {
            expires_at: sent_at.checked_add(expiry).unwrap_or(sent_at),
            expiry,
        }
    }

    pub fn expires_at(self) -> Instant {
        self.expires_at
    }

    pub fn reply_still_authorizes(self, now: Instant) -> bool {
        now < self.expires_at
    }

    pub fn after_successful_renewal(self, sent_at: Instant) -> Self {
        Self::from_request(sent_at, self.expiry)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenewalStop {
    Expired,
    Rejected,
    Cancelled,
}

pub fn authority_ends(stop: RenewalStop) -> bool {
    matches!(stop, RenewalStop::Expired | RenewalStop::Rejected)
}

pub async fn supervise_renewal<F, Fut, E>(
    mut renew: F,
    mut deadline: LocalLeaseDeadline,
    renewal_every: Duration,
    mut stop: tokio::sync::watch::Receiver<bool>,
    mut on_extended: E,
) -> RenewalStop
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<(), ()>>,
    E: FnMut(LocalLeaseDeadline),
{
    let mut interval = tokio::time::interval(renewal_every);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    interval.tick().await;
    loop {
        tokio::select! {
            biased;
            result = stop.changed() => {
                if result.is_err() || *stop.borrow() {
                    return RenewalStop::Cancelled;
                }
            }
            _ = tokio::time::sleep_until(deadline.expires_at()) => {
                return RenewalStop::Expired;
            }
            _ = interval.tick() => {
                let sent_at = Instant::now();
                if !deadline.reply_still_authorizes(sent_at) {
                    return RenewalStop::Expired;
                }
                let attempt = renew();
                tokio::pin!(attempt);
                tokio::select! {
                    biased;
                    result = stop.changed() => {
                        if result.is_err() || *stop.borrow() {
                            return RenewalStop::Cancelled;
                        }
                    }
                    _ = tokio::time::sleep_until(deadline.expires_at()) => {
                        return RenewalStop::Expired;
                    }
                    result = &mut attempt => {
                        match result {
                            Err(()) => return RenewalStop::Rejected,
                            Ok(()) => {
                                let next = deadline.after_successful_renewal(sent_at);
                                if !next.reply_still_authorizes(Instant::now()) {
                                    return RenewalStop::Expired;
                                }
                                deadline = next;
                                on_extended(deadline);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::{LocalLeaseDeadline, RenewalStop, supervise_renewal};

    #[tokio::test(flavor = "current_thread")]
    async fn queued_reply_time_counts_against_the_deadline() {
        let now = tokio::time::Instant::now();
        let expiry = Duration::from_secs(60);
        let sent = now
            .checked_sub(Duration::from_secs(15))
            .expect("sent instant");
        let deadline = LocalLeaseDeadline::from_request(sent, expiry);
        assert!(deadline.reply_still_authorizes(now));
        assert_eq!(deadline.expires_at(), sent + expiry);
        let late = now.checked_sub(expiry).expect("late send");
        assert!(!LocalLeaseDeadline::from_request(late, expiry).reply_still_authorizes(now));
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn stalled_renewal_expires_while_the_worker_reply_is_still_blocked() {
        let started = Arc::new(AtomicBool::new(false));
        let (late_tx, late_rx) = tokio::sync::oneshot::channel::<()>();
        let late_rx = Arc::new(Mutex::new(Some(late_rx)));
        let started_flag = started.clone();
        let (_stop_tx, stop_rx) = tokio::sync::watch::channel(false);
        let expiry = Duration::from_secs(60);
        let renewal = Duration::from_secs(10);
        let deadline = LocalLeaseDeadline::from_request(tokio::time::Instant::now(), expiry);
        let supervise = tokio::spawn(supervise_renewal(
            move || {
                let started_flag = started_flag.clone();
                let late_rx = late_rx.clone();
                async move {
                    started_flag.store(true, Ordering::SeqCst);
                    let rx = late_rx.lock().expect("lock").take().expect("one renewal");
                    let _ = rx.await;
                    Ok(())
                }
            },
            deadline,
            renewal,
            stop_rx,
            |_| {},
        ));

        tokio::task::yield_now().await;
        // Tokio's timer wheel rounds a deadline up to the next millisecond.
        tokio::time::advance(renewal + Duration::from_millis(50)).await;
        tokio::task::yield_now().await;
        assert!(
            started.load(Ordering::SeqCst),
            "renewal did not start before the deadline"
        );
        tokio::time::advance(expiry.saturating_sub(renewal) + Duration::from_millis(50)).await;
        tokio::task::yield_now().await;
        assert!(
            supervise.is_finished(),
            "local deadline did not stop a renewal still blocked on the worker"
        );
        assert_eq!(supervise.await.unwrap(), RenewalStop::Expired);
        assert!(
            late_tx.is_closed(),
            "the stalled renewal future was still held"
        );
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn renewal_reply_extends_from_the_send_instant_not_past_it() {
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_flag = calls.clone();
        let (late_tx, late_rx) = tokio::sync::oneshot::channel::<()>();
        let late_rx = Arc::new(Mutex::new(Some(late_rx)));
        let (_stop_tx, stop_rx) = tokio::sync::watch::channel(false);
        let expiry = Duration::from_secs(60);
        let renewal = Duration::from_secs(10);
        let started = tokio::time::Instant::now();
        let supervise = tokio::spawn(supervise_renewal(
            move || {
                let calls_flag = calls_flag.clone();
                let late_rx = late_rx.clone();
                async move {
                    let n = calls_flag.fetch_add(1, Ordering::SeqCst);
                    if n == 0 {
                        Ok(())
                    } else {
                        let rx = late_rx
                            .lock()
                            .expect("lock")
                            .take()
                            .expect("second renewal");
                        let _ = rx.await;
                        Ok(())
                    }
                }
            },
            LocalLeaseDeadline::from_request(started, expiry),
            renewal,
            stop_rx,
            |_| {},
        ));

        tokio::task::yield_now().await;
        tokio::time::advance(renewal + Duration::from_millis(50)).await;
        tokio::task::yield_now().await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        tokio::time::advance(Duration::from_secs(50)).await;
        tokio::task::yield_now().await;
        assert!(
            !supervise.is_finished(),
            "a successful renewal must extend the local deadline from its send instant"
        );
        tokio::time::advance(Duration::from_secs(11)).await;
        tokio::task::yield_now().await;
        assert!(supervise.is_finished());
        assert_eq!(supervise.await.unwrap(), RenewalStop::Expired);
        assert!(late_tx.is_closed());
    }
}
