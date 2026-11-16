//! The Rig adapter against a local server that replays provider replies:
//! the OpenAI Chat stream (DeepSeek's dialect, the shape ten providers
//! speak) and Ollama's native `/api/chat`. Covers what the loop relies on:
//! text and reasoning deltas, a tool call, usage, the request we send, and
//! errors turned into one sentence.

use std::sync::Mutex;

use mystique_agent::test_support::{boxed, Delta, TurnError, TurnModel, TurnOutput, TurnRequest};
use mystique_agent::AgentProvider;
use rig::completion::{Message, ToolDefinition};
use rig::message::{ToolName, ToolResultContent, UserContent};
use rig::providers::ollama;
use rig::providers::openai::wire::DEEPSEEK;
use rig::providers::openai::OpenAIConfig;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// One request served with `status`, `content_type` and `body`. Resolves to
/// the request's head and JSON body.
async fn serve(
    status: &'static str,
    content_type: &'static str,
    body: String,
) -> (String, tokio::task::JoinHandle<(String, Value)>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let url = format!("http://{}", listener.local_addr().expect("addr"));
    let handle = tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.expect("accept");
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        let head_end = loop {
            let n = sock.read(&mut chunk).await.expect("read");
            assert!(n > 0, "client closed before the headers ended");
            buf.extend_from_slice(&chunk[..n]);
            if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break i + 4;
            }
        };
        let head = String::from_utf8_lossy(&buf[..head_end]).to_ascii_lowercase();
        let len = head
            .lines()
            .find_map(|l| l.strip_prefix("content-length:"))
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(0);
        while buf.len() < head_end + len {
            let n = sock.read(&mut chunk).await.expect("read body");
            assert!(n > 0, "client closed before the body ended");
            buf.extend_from_slice(&chunk[..n]);
        }
        let request: Value =
            serde_json::from_slice(&buf[head_end..head_end + len]).unwrap_or(Value::Null);
        let reply = format!(
            "HTTP/1.1 {status}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        sock.write_all(reply.as_bytes()).await.expect("write");
        let _ = sock.shutdown().await;
        (head, request)
    });
    (url, handle)
}

fn request(user: &str) -> TurnRequest {
    TurnRequest {
        preamble: "You write SQL.".into(),
        history: vec![Message::user(user)],
        tools: vec![ToolDefinition::new(
            ToolName::new("describe_object").expect("tool name"),
            "Describe a table",
            json!({ "type": "object", "properties": { "name": { "type": "string" } } }),
        )],
        max_tokens: 512,
    }
}

#[derive(Default)]
struct Seen {
    text: String,
    reasoning: String,
}

async fn run(model: &dyn TurnModel, req: TurnRequest) -> (Result<TurnOutput, TurnError>, Seen) {
    let seen = Mutex::new(Seen::default());
    let sink = |delta: Delta| {
        let mut seen = seen.lock().expect("seen");
        match delta {
            Delta::Text(t) => seen.text.push_str(&t),
            Delta::Reasoning(t) => seen.reasoning.push_str(&t),
        }
    };
    let out = model.turn(req, &sink).await;
    (out, seen.into_inner().expect("seen"))
}

/// A message's text: OpenAI Chat takes a plain string or a list of parts.
fn content_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts.iter().filter_map(|p| p["text"].as_str()).collect(),
        _ => String::new(),
    }
}

fn sse(chunks: &[Value]) -> String {
    let mut out = String::new();
    for chunk in chunks {
        out.push_str(&format!("data: {chunk}\n\n"));
    }
    out.push_str("data: [DONE]\n\n");
    out
}

fn chat_chunk(delta: Value, finish: Option<&str>) -> Value {
    json!({
        "id": "c1", "object": "chat.completion.chunk", "created": 1, "model": "m",
        "choices": [{ "index": 0, "delta": delta, "finish_reason": finish }]
    })
}

fn deepseek_at(url: &str) -> Box<dyn TurnModel> {
    boxed(
        AgentProvider::Deepseek,
        OpenAIConfig::with_key(&DEEPSEEK, "sk-test")
            .with_base_url(url)
            .client()
            .completion("deepseek-chat"),
    )
}

#[tokio::test]
async fn openai_chat_stream_gives_deltas_a_tool_call_and_usage() {
    let body = sse(&[
        chat_chunk(
            json!({ "role": "assistant", "reasoning_content": "Look at t." }),
            None,
        ),
        chat_chunk(json!({ "content": "Hel" }), None),
        chat_chunk(json!({ "content": "lo" }), None),
        chat_chunk(
            json!({ "tool_calls": [{ "index": 0, "id": "call_1", "type": "function",
                "function": { "name": "describe_object", "arguments": "{\"name\":" } }] }),
            None,
        ),
        chat_chunk(
            json!({ "tool_calls": [{ "index": 0, "function": { "arguments": "\"t\"}" } }] }),
            Some("tool_calls"),
        ),
        json!({ "id": "c1", "object": "chat.completion.chunk", "created": 1, "model": "m",
            "choices": [], "usage": { "prompt_tokens": 12, "completion_tokens": 7, "total_tokens": 19 } }),
    ]);
    let (url, served) = serve("200 OK", "text/event-stream", body).await;
    let (out, seen) = run(deepseek_at(&url).as_ref(), request("Describe t")).await;
    let out = out.expect("turn");

    assert_eq!(seen.text, "Hello");
    assert_eq!(seen.reasoning, "Look at t.");
    assert_eq!(out.message.text(), "Hello");
    let calls: Vec<_> = out.message.tool_calls().collect();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].function.name.as_str(), "describe_object");
    assert_eq!(calls[0].function.arguments_value(), json!({ "name": "t" }));
    assert_eq!(calls[0].id.wire(), "call_1");
    assert_eq!(out.usage.input_tokens, Some(12));
    assert_eq!(out.usage.output_tokens, Some(7));

    // What we sent: the system prompt first, the user turn, the tool, the cap.
    let (head, sent) = served.await.expect("served");
    assert!(head.starts_with("post "), "{head}");
    assert!(head.contains("authorization: bearer sk-test"), "{head}");
    let messages = sent["messages"].as_array().expect("messages");
    assert_eq!(messages[0]["role"], "system");
    assert_eq!(content_text(&messages[0]["content"]), "You write SQL.");
    assert_eq!(
        content_text(&messages.last().expect("last")["content"]),
        "Describe t"
    );
    assert_eq!(sent["tools"][0]["function"]["name"], "describe_object");
    assert_eq!(sent["stream"], true);
    let cap = sent
        .get("max_tokens")
        .or_else(|| sent.get("max_completion_tokens"));
    assert_eq!(cap, Some(&json!(512)), "{sent}");
}

#[tokio::test]
async fn a_rejected_key_is_one_sentence_and_not_retried() {
    let body = json!({ "error": { "message": "Incorrect API key provided.", "type": "invalid_request_error",
        "code": "invalid_api_key" } })
    .to_string();
    let (url, _served) = serve("401 Unauthorized", "application/json", body).await;
    let (out, seen) = run(deepseek_at(&url).as_ref(), request("hi")).await;
    let Err(err) = out else {
        panic!("expected an error");
    };
    assert_eq!(
        err.message,
        "DeepSeek rejected the API key. Check it in Settings → AI."
    );
    assert!(!err.retryable);
    assert!(seen.text.is_empty());
}

#[tokio::test]
async fn an_overloaded_provider_is_retryable() {
    let body =
        json!({ "error": { "message": "Server overloaded", "type": "server_error" } }).to_string();
    let (url, _served) = serve("503 Service Unavailable", "application/json", body).await;
    let (out, _) = run(deepseek_at(&url).as_ref(), request("hi")).await;
    let Err(err) = out else {
        panic!("expected an error");
    };
    assert!(err.retryable, "{}", err.message);
    assert!(err.message.contains("DeepSeek"), "{}", err.message);
}

#[tokio::test]
async fn ollama_native_chat_streams_thinking_text_and_a_tool_call() {
    let line = |message: Value, done: bool| {
        let mut v = json!({ "model": "qwen3", "created_at": "2026-01-01T00:00:00Z",
            "message": message, "done": done });
        if done {
            v["done_reason"] = json!("stop");
            v["prompt_eval_count"] = json!(9);
            v["eval_count"] = json!(4);
        }
        format!("{v}\n")
    };
    let body = [
        line(
            json!({ "role": "assistant", "content": "", "thinking": "Hmm." }),
            false,
        ),
        line(json!({ "role": "assistant", "content": "Sure" }), false),
        line(
            json!({ "role": "assistant", "content": "", "tool_calls": [
                { "function": { "name": "describe_object", "arguments": { "name": "t" } } }
            ] }),
            false,
        ),
        line(json!({ "role": "assistant", "content": "" }), true),
    ]
    .concat();
    let (url, served) = serve("200 OK", "application/x-ndjson", body).await;
    let model = boxed(
        AgentProvider::Ollama,
        ollama::OllamaConfig::new()
            .with_base_url(&url)
            .client()
            .native_completion("qwen3"),
    );
    let (out, seen) = run(model.as_ref(), request("Describe t")).await;
    let out = out.expect("turn");

    assert_eq!(seen.text, "Sure");
    assert_eq!(seen.reasoning, "Hmm.");
    let calls: Vec<_> = out.message.tool_calls().collect();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].function.name.as_str(), "describe_object");
    assert_eq!(calls[0].function.arguments_value(), json!({ "name": "t" }));
    assert_eq!(out.usage.input_tokens, Some(9));
    assert_eq!(out.usage.output_tokens, Some(4));

    let (head, sent) = served.await.expect("served");
    assert!(head.starts_with("post /api/chat"), "{head}");
    assert_eq!(sent["model"], "qwen3");
    assert_eq!(sent["messages"][0]["role"], "system");
    assert_eq!(sent["tools"][0]["function"]["name"], "describe_object");
}

/// Turn two carries turn one's call and our answer, as the loop builds them:
/// the provider must see its own call id on both.
#[tokio::test]
async fn the_next_request_carries_the_call_and_its_result() {
    let first = sse(&[chat_chunk(
        json!({ "role": "assistant", "tool_calls": [{ "index": 0, "id": "call_9", "type": "function",
                "function": { "name": "describe_object", "arguments": "{\"name\":\"t\"}" } }] }),
        Some("tool_calls"),
    )]);
    let (url, _served) = serve("200 OK", "text/event-stream", first).await;
    let (out, _) = run(deepseek_at(&url).as_ref(), request("Describe t")).await;
    let out = out.expect("turn one");
    let call = out.message.tool_calls().next().expect("call").clone();

    let mut next = request("Describe t");
    next.history.push(Message::Assistant(out.message));
    next.history.push(Message::User {
        content: vec![UserContent::ToolResult(
            call.result(vec![ToolResultContent::text("{\"columns\":2}")]),
        )],
    });
    let second = sse(&[chat_chunk(
        json!({ "role": "assistant", "content": "Two columns." }),
        Some("stop"),
    )]);
    let (url, served) = serve("200 OK", "text/event-stream", second).await;
    let (out, _) = run(deepseek_at(&url).as_ref(), next).await;
    assert_eq!(out.expect("turn two").message.text(), "Two columns.");

    let (_, sent) = served.await.expect("served");
    let messages = sent["messages"].as_array().expect("messages");
    let assistant = messages
        .iter()
        .find(|m| m["role"] == "assistant")
        .expect("assistant turn");
    assert_eq!(assistant["tool_calls"][0]["id"], "call_9");
    assert_eq!(
        assistant["tool_calls"][0]["function"]["name"],
        "describe_object"
    );
    let tool = messages
        .iter()
        .find(|m| m["role"] == "tool")
        .expect("tool result");
    assert_eq!(tool["tool_call_id"], "call_9");
    assert_eq!(content_text(&tool["content"]), "{\"columns\":2}");
}
