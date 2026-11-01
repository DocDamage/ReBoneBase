//! Background work shared by imports and exports.
//!
//! A job runs on its own thread and reports progress through atomics the UI
//! reads every frame. Cancellation is cooperative: long loops call
//! [`Progress::advance`] or [`Progress::checkpoint`], which fail with
//! [`Cancelled`] once a cancel was requested. Publishing files is never
//! interrupted; export code stops checking before the transaction starts.
use anyhow::Result;
use std::{
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

/// Error returned by work that stopped because the user cancelled it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cancelled;
impl fmt::Display for Cancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Cancelled")
    }
}
impl std::error::Error for Cancelled {}

pub fn is_cancelled(error: &anyhow::Error) -> bool {
    error.chain().any(|e| e.is::<Cancelled>())
}

#[derive(Default, Debug)]
pub struct Progress {
    done: AtomicU64,
    total: AtomicU64,
    cancelled: AtomicBool,
    committing: AtomicBool,
    stage: Mutex<String>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub done: u64,
    pub total: u64,
    pub stage: String,
    pub cancelling: bool,
    /// False once the job has started writing results it will not undo.
    pub cancellable: bool,
}
impl Snapshot {
    /// None while the amount of work is unknown.
    pub fn fraction(&self) -> Option<f32> {
        (self.total > 0).then(|| (self.done as f64 / self.total as f64).min(1.) as f32)
    }
}
impl Progress {
    pub fn set_stage(&self, stage: impl Into<String>) {
        if let Ok(mut s) = self.stage.lock() {
            *s = stage.into();
        }
    }
    pub fn add_total(&self, units: u64) {
        self.total.fetch_add(units, Ordering::Relaxed);
    }
    /// Count finished units and stop if the job was cancelled.
    pub fn advance(&self, units: u64) -> Result<()> {
        self.done.fetch_add(units, Ordering::Relaxed);
        self.checkpoint()
    }
    pub fn checkpoint(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(Cancelled.into())
        } else {
            Ok(())
        }
    }
    /// Last cancellation point: from here on the job finishes its writes.
    pub fn commit(&self, stage: impl Into<String>) -> Result<()> {
        self.checkpoint()?;
        self.set_stage(stage);
        self.committing.store(true, Ordering::Relaxed);
        Ok(())
    }
    /// Ignored once the job has committed.
    pub fn cancel(&self) {
        if !self.committing.load(Ordering::Relaxed) {
            self.cancelled.store(true, Ordering::Relaxed);
        }
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            done: self.done.load(Ordering::Relaxed),
            total: self.total.load(Ordering::Relaxed),
            stage: self.stage.lock().map(|s| s.clone()).unwrap_or_default(),
            cancelling: self.is_cancelled(),
            cancellable: !self.committing.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Outcome<T> {
    Finished(T),
    Failed(String),
    Cancelled,
}

pub struct Job<T> {
    label: String,
    progress: Arc<Progress>,
    receiver: mpsc::Receiver<Outcome<T>>,
    started: Instant,
}
impl<T: Send + 'static> Job<T> {
    pub fn spawn(
        label: impl Into<String>,
        work: impl FnOnce(&Progress) -> Result<T> + Send + 'static,
    ) -> Self {
        let progress = Arc::new(Progress::default());
        let (sender, receiver) = mpsc::channel();
        let worker = progress.clone();
        let failure = sender.clone();
        let spawned = std::thread::Builder::new()
            .name("rebone-job".into())
            .spawn(move || {
                let outcome = match catch_unwind(AssertUnwindSafe(|| work(&worker))) {
                    Ok(Ok(value)) => Outcome::Finished(value),
                    Ok(Err(e)) if is_cancelled(&e) => Outcome::Cancelled,
                    Ok(Err(e)) => Outcome::Failed(format!("{e:#}")),
                    Err(_) => Outcome::Failed("Background task stopped unexpectedly".into()),
                };
                let _ = sender.send(outcome);
            });
        if let Err(e) = spawned {
            let _ = failure.send(Outcome::Failed(format!(
                "Background task could not start: {e}"
            )));
        }
        Self {
            label: label.into(),
            progress,
            receiver,
            started: Instant::now(),
        }
    }
}
impl<T> Job<T> {
    /// None while the job is still running.
    pub fn poll(&self) -> Option<Outcome<T>> {
        match self.receiver.try_recv() {
            Ok(outcome) => Some(outcome),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Outcome::Failed(
                "Background task ended without a result".into(),
            )),
        }
    }
    /// Block until the job ends; for tests and command-line tools.
    pub fn wait(self) -> Outcome<T> {
        self.receiver.recv().unwrap_or(Outcome::Failed(
            "Background task ended without a result".into(),
        ))
    }
    pub fn cancel(&self) {
        self.progress.cancel();
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn progress(&self) -> Snapshot {
        self.progress.snapshot()
    }
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finished_failed_and_panicking_work_report_one_outcome() {
        let job = Job::spawn("ok", |p| {
            p.add_total(2);
            p.advance(2)?;
            Ok(7)
        });
        assert_eq!(job.wait(), Outcome::Finished(7));
        let job = Job::<()>::spawn("bad", |_| anyhow::bail!("broken"));
        assert_eq!(job.wait(), Outcome::Failed("broken".into()));
        let job = Job::<()>::spawn("panic", |_| panic!("boom"));
        assert!(matches!(job.wait(), Outcome::Failed(_)));
    }

    #[test]
    fn cancellation_stops_at_the_next_checkpoint() {
        let (ready, started) = mpsc::channel::<()>();
        let (go, wait) = mpsc::channel::<()>();
        let job = Job::spawn("cancel", move |p| {
            p.set_stage("Rendering");
            p.add_total(10);
            ready.send(()).ok();
            wait.recv().ok();
            for _ in 0..10 {
                p.advance(1)?;
            }
            Ok(())
        });
        started.recv().unwrap();
        assert_eq!(job.progress().stage, "Rendering");
        job.cancel();
        assert!(job.progress().cancelling);
        go.send(()).unwrap();
        assert_eq!(job.wait(), Outcome::Cancelled);
    }

    #[test]
    fn committed_work_ignores_cancel_and_reports_real_failures() {
        let p = Progress::default();
        p.commit("Writing files").unwrap();
        p.cancel();
        let s = p.snapshot();
        assert!(!s.cancellable && !s.cancelling);
        assert_eq!(s.stage, "Writing files");
        let job = Job::<()>::spawn("write", |p| {
            p.commit("Writing files")?;
            p.cancel();
            anyhow::bail!("disk full")
        });
        assert_eq!(job.wait(), Outcome::Failed("disk full".into()));
        let p = Progress::default();
        p.cancel();
        assert!(is_cancelled(&p.commit("Writing files").unwrap_err()));
    }

    #[test]
    fn fraction_is_unknown_until_work_is_counted() {
        let p = Progress::default();
        assert_eq!(p.snapshot().fraction(), None);
        p.add_total(4);
        p.advance(1).unwrap();
        assert_eq!(p.snapshot().fraction(), Some(0.25));
        p.advance(9).unwrap();
        assert_eq!(p.snapshot().fraction(), Some(1.));
    }

    #[test]
    fn context_wrapped_cancellation_is_still_cancellation() {
        let e = anyhow::Error::from(Cancelled).context("While rendering clip 2");
        assert!(is_cancelled(&e));
        assert!(!is_cancelled(&anyhow::anyhow!("other")));
    }
}
