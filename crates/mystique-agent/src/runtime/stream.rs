//! Batches deltas before they cross IPC, and redacts every event.
//!
//! The run pushes events into an unbounded queue. One forwarder task drains
//! it: text and reasoning deltas are joined and flushed every
//! [`DELTA_FLUSH`] (never once per token); any other event flushes what is
//! buffered first, so order is kept.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::events::AgentEvent;

/// How often buffered deltas cross IPC (the window asks for 50–100 ms).
pub(crate) const DELTA_FLUSH: Duration = Duration::from_millis(75);

/// Where finished events go: a Tauri channel in the app, a Vec in tests.
pub type AgentEventSink = Arc<dyn Fn(AgentEvent) + Send + Sync>;

/// The run's side of the queue. Cheap to clone; sends never block.
#[derive(Clone)]
pub(crate) struct EventTx(mpsc::UnboundedSender<AgentEvent>);

impl EventTx {
    pub(crate) fn send(&self, event: AgentEvent) {
        // The forwarder only stops after every sender is gone.
        let _ = self.0.send(event);
    }
}

/// Start the forwarder. It ends once every [`EventTx`] is dropped.
pub(crate) fn spawn_forwarder(sink: AgentEventSink, every: Duration) -> (EventTx, JoinHandle<()>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let handle = tokio::spawn(forward(rx, sink, every));
    (EventTx(tx), handle)
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum DeltaKind {
    Text,
    Reasoning,
}

struct Pending {
    kind: DeltaKind,
    text: String,
}

fn flush(pending: &mut Option<Pending>, sink: &AgentEventSink) {
    if let Some(p) = pending.take() {
        if p.text.is_empty() {
            return;
        }
        sink(match p.kind {
            DeltaKind::Text => AgentEvent::TextDelta { text: p.text },
            DeltaKind::Reasoning => AgentEvent::ReasoningDelta { text: p.text },
        });
    }
}

async fn forward(
    mut rx: mpsc::UnboundedReceiver<AgentEvent>,
    sink: AgentEventSink,
    every: Duration,
) {
    let mut pending: Option<Pending> = None;
    let mut tick = tokio::time::interval(every);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            next = rx.recv() => {
                let Some(event) = next else { break };
                let (kind, text) = match event {
                    AgentEvent::TextDelta { text } => (DeltaKind::Text, text),
                    AgentEvent::ReasoningDelta { text } => (DeltaKind::Reasoning, text),
                    other => {
                        flush(&mut pending, &sink);
                        sink(other.redacted());
                        continue;
                    }
                };
                match pending.as_mut() {
                    Some(p) if p.kind == kind => p.text.push_str(&text),
                    _ => {
                        flush(&mut pending, &sink);
                        pending = Some(Pending { kind, text });
                    }
                }
            }
            _ = tick.tick() => flush(&mut pending, &sink),
        }
    }
    flush(&mut pending, &sink);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A sink that records events, for tests across the runtime.
    pub(crate) fn recording_sink() -> (AgentEventSink, Arc<Mutex<Vec<AgentEvent>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink_seen = Arc::clone(&seen);
        let sink: AgentEventSink = Arc::new(move |event| {
            if let Ok(mut list) = sink_seen.lock() {
                list.push(event);
            }
        });
        (sink, seen)
    }

    #[tokio::test]
    async fn deltas_are_batched_not_sent_per_token() {
        let (sink, seen) = recording_sink();
        let (tx, handle) = spawn_forwarder(sink, DELTA_FLUSH);
        for i in 0..200 {
            tx.send(AgentEvent::TextDelta {
                text: format!("t{i} "),
            });
        }
        tokio::time::sleep(Duration::from_millis(120)).await;
        for i in 200..400 {
            tx.send(AgentEvent::TextDelta {
                text: format!("t{i} "),
            });
        }
        drop(tx);
        handle.await.unwrap();
        let events = seen.lock().unwrap().clone();
        assert!(
            events.len() <= 6,
            "400 tokens became {} IPC messages",
            events.len()
        );
        let joined: String = events
            .iter()
            .map(|e| match e {
                AgentEvent::TextDelta { text } => text.as_str(),
                _ => "",
            })
            .collect();
        let expected: String = (0..400).map(|i| format!("t{i} ")).collect();
        assert_eq!(joined, expected);
    }

    #[tokio::test]
    async fn other_events_flush_buffered_text_first() {
        let (sink, seen) = recording_sink();
        let (tx, handle) = spawn_forwarder(sink, Duration::from_secs(60));
        tx.send(AgentEvent::TextDelta { text: "a".into() });
        tx.send(AgentEvent::ReasoningDelta { text: "r".into() });
        tx.send(AgentEvent::TextDelta { text: "b".into() });
        tx.send(AgentEvent::Exhausted { turns: 2 });
        tx.send(AgentEvent::TextDelta { text: "c".into() });
        drop(tx);
        handle.await.unwrap();
        assert_eq!(
            seen.lock().unwrap().clone(),
            vec![
                AgentEvent::TextDelta { text: "a".into() },
                AgentEvent::ReasoningDelta { text: "r".into() },
                AgentEvent::TextDelta { text: "b".into() },
                AgentEvent::Exhausted { turns: 2 },
                AgentEvent::TextDelta { text: "c".into() },
            ]
        );
    }
}
