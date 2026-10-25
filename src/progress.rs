//! Optional terminal status, separate from the private audit and JSON output.
use engarg::{
    Error,
    provider::{Completion, CompletionRequest, HttpEvent, LanguageModel, Role},
};
use std::{
    future::Future,
    io::Write,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

type Writer = Arc<dyn Fn(&str) + Send + Sync>;

struct State {
    started: Instant,
    next_call: AtomicU64,
    phase: Mutex<&'static str>,
    heartbeat: Duration,
    write: Writer,
}

#[derive(Clone)]
pub(crate) struct Progress(Option<Arc<State>>);

impl Progress {
    pub(crate) fn new(enabled: bool) -> Self {
        Self(enabled.then(|| {
            Arc::new(State {
                started: Instant::now(),
                next_call: AtomicU64::new(1),
                phase: Mutex::new("input and configuration checks"),
                heartbeat: Duration::from_secs(10),
                write: Arc::new(|line| {
                    // Status output must not fail an otherwise valid operation.
                    let _ = writeln!(std::io::stderr().lock(), "{line}");
                }),
            })
        }))
    }

    fn line(&self, message: &str) {
        if let Some(state) = &self.0 {
            (state.write)(&format!(
                "[engarg +{:.1}s] {message}",
                state.started.elapsed().as_secs_f64()
            ));
        }
    }

    pub(crate) fn phase(&self, phase: &'static str) {
        if let Some(state) = &self.0 {
            if let Ok(mut current) = state.phase.lock() {
                *current = phase;
            }
            self.line(phase);
        }
    }

    pub(crate) fn http(&self, event: &HttpEvent) {
        let wait = event
            .wait_ms
            .map(|ms| format!("; retry wait {ms} ms"))
            .unwrap_or_default();
        self.line(&format!(
            "{} HTTP attempt {}: {} (status {}){wait}",
            event.role, event.attempt, event.kind, event.status
        ));
    }

    pub(crate) fn model<M>(&self, inner: M) -> ProgressModel<M> {
        ProgressModel {
            inner,
            progress: self.clone(),
        }
    }

    pub(crate) async fn run<T>(
        &self,
        operation: &'static str,
        future: impl Future<Output = Result<T, Error>>,
    ) -> Result<T, Error> {
        let Some(state) = &self.0 else {
            return future.await;
        };
        self.line(&format!("{operation} started"));
        let mut heartbeat = tokio::time::interval_at(
            tokio::time::Instant::now() + state.heartbeat,
            state.heartbeat,
        );
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        tokio::pin!(future);
        let result = loop {
            tokio::select! {
                result = &mut future => break result,
                _ = heartbeat.tick() => {
                    let phase = state.phase.lock().map(|phase| *phase).unwrap_or("analysis");
                    self.line(&format!("still running: {phase}"));
                }
            }
        };
        self.line(&format!(
            "{operation} {}",
            if result.is_ok() {
                "completed"
            } else {
                "failed"
            }
        ));
        result
    }
}

pub(crate) struct ProgressModel<M> {
    inner: M,
    progress: Progress,
}

struct CallStatus {
    progress: Progress,
    id: u64,
    role: Role,
    started: Instant,
    finished: bool,
}

impl Drop for CallStatus {
    fn drop(&mut self) {
        if !self.finished {
            self.progress.line(&format!(
                "call {} {} cancelled after {:.1}s",
                self.id,
                self.role,
                self.started.elapsed().as_secs_f64()
            ));
        }
    }
}

impl<M: LanguageModel> LanguageModel for ProgressModel<M> {
    async fn complete(&self, request: CompletionRequest) -> Result<Completion, Error> {
        let Some(state) = &self.progress.0 else {
            return self.inner.complete(request).await;
        };
        let mut call = CallStatus {
            progress: self.progress.clone(),
            id: state.next_call.fetch_add(1, Ordering::Relaxed),
            role: request.role,
            started: Instant::now(),
            finished: false,
        };
        self.progress
            .line(&format!("call {} {} started", call.id, call.role));
        let result = self.inner.complete(request).await;
        call.finished = true;
        let outcome = match &result {
            Ok(completion) => format!("response received ({} bytes)", completion.content.len()),
            Err(_) => "request failed".to_owned(),
        };
        self.progress.line(&format!(
            "call {} {} {outcome} after {:.1}s",
            call.id,
            call.role,
            call.started.elapsed().as_secs_f64()
        ));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn captured() -> (Progress, Arc<Mutex<Vec<String>>>) {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let output = Arc::clone(&lines);
        let progress = Progress(Some(Arc::new(State {
            started: Instant::now(),
            next_call: AtomicU64::new(1),
            phase: Mutex::new("argument authoring"),
            heartbeat: Duration::from_millis(10),
            write: Arc::new(move |line| output.lock().unwrap().push(line.to_owned())),
        })));
        (progress, lines)
    }

    struct PendingModel;
    impl LanguageModel for PendingModel {
        async fn complete(&self, _: CompletionRequest) -> Result<Completion, Error> {
            std::future::pending().await
        }
    }

    fn request() -> CompletionRequest {
        CompletionRequest {
            role: Role::FinalCritic,
            system: "private prompt".into(),
            user: "private input and credential".into(),
            schema: json!({}),
        }
    }

    #[tokio::test]
    async fn reports_a_started_call_and_heartbeat_before_cancellation_without_content() {
        let (mut progress, lines) = captured();
        let heartbeat_seen = Arc::new(tokio::sync::Notify::new());
        let signal = Arc::clone(&heartbeat_seen);
        let output = Arc::clone(&lines);
        Arc::get_mut(progress.0.as_mut().unwrap()).unwrap().write = Arc::new(move |line| {
            output.lock().unwrap().push(line.to_owned());
            if line.contains("still running") {
                signal.notify_one();
            }
        });
        let model = progress.model(PendingModel);
        let task = progress.run("analysis", async {
            tokio::select! {
                result = model.complete(request()) => result,
                _ = heartbeat_seen.notified() => Err(Error::Budget("fixture cancellation".into())),
            }
        });
        assert!(
            tokio::time::timeout(Duration::from_secs(2), task)
                .await
                .unwrap()
                .is_err()
        );
        let lines = lines.lock().unwrap().join("\n");
        assert!(lines.contains("call 1 final_critic started"));
        assert!(lines.contains("still running: argument authoring"));
        assert!(lines.contains("call 1 final_critic cancelled"));
        assert!(lines.contains("analysis failed"));
        assert!(!lines.contains("private"));
    }

    struct ReplyModel(bool);
    impl LanguageModel for ReplyModel {
        async fn complete(&self, _: CompletionRequest) -> Result<Completion, Error> {
            if self.0 {
                return Err(Error::Provider("private failure body".into()));
            }
            Ok(Completion {
                content: "private model content".into(),
                model: "private model identity".into(),
                input_tokens: None,
                output_tokens: None,
                request_id: None,
                service_tier: None,
                reasoning_effort: None,
            })
        }
    }

    #[tokio::test]
    async fn progress_preserves_results_and_reports_completion_and_failure_without_content() {
        for failed in [false, true] {
            let (progress, lines) = captured();
            let model = progress.model(ReplyModel(failed));
            let result = progress.run("analysis", model.complete(request())).await;
            assert_eq!(result.is_err(), failed);
            if let Ok(value) = result {
                assert_eq!(value.content, "private model content");
            }
            let lines = lines.lock().unwrap().join("\n");
            assert!(lines.contains(if failed {
                "request failed"
            } else {
                "response received"
            }));
            assert!(!lines.contains("cancelled"));
            assert!(!lines.contains("private"));
        }
    }
}
