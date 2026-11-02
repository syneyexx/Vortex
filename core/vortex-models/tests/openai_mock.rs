//! Adapter-tests tegen een nep-llama-server (axum). De echte servers worden op de PC getest.

use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::Json;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use futures::StreamExt;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use vortex_models::backend::{Backend, Capabilities, ChatRequest, Message, ResponseFormat, StreamEvent, ToolSpec};
use vortex_models::{ModelError, OpenAiBackend};

fn sse(chunks: Vec<Vec<u8>>, delay_ms: u64) -> Response {
    let s = futures::stream::iter(chunks).then(move |c| async move {
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        Ok::<_, std::io::Error>(c)
    });
    Response::builder().header("content-type", "text/event-stream").body(Body::from_stream(s)).unwrap_or_default()
}

fn data(v: Value) -> Vec<u8> {
    format!("data: {v}\n\n").into_bytes()
}

async fn chat(Json(b): Json<Value>) -> Response {
    let last =
        b["messages"].as_array().and_then(|m| m.last()).and_then(|m| m["content"].as_str()).unwrap_or("").to_string();
    let stream = b["stream"].as_bool().unwrap_or(false);
    match (last.as_str(), stream) {
        ("tool", false) => Json(json!({"choices":[{"message":{"content":"","tool_calls":[
            {"id":"c1","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"a.txt\"}"}}]},
            "finish_reason":"tool_calls"}],"usage":{"prompt_tokens":5,"completion_tokens":3}})).into_response(),
        ("badtool", false) => Json(json!({"choices":[{"message":{"content":"","tool_calls":[
            {"id":"c1","type":"function","function":{"name":"rm_rf","arguments":"{}"}}]},"finish_reason":"tool_calls"}]})).into_response(),
        ("json", false) => Json(json!({"choices":[{"message":{"content":"{\"a\":1}"},"finish_reason":"stop"}],
            "echo_format": b["response_format"]})).into_response(),
        ("brokenjson", false) => Json(json!({"choices":[{"message":{"content":"{a:"},"finish_reason":"stop"}]})).into_response(),
        (_, false) => Json(json!({"choices":[{"message":{"content":"Hallo wereld","reasoning_content":"denk"},"finish_reason":"stop"}],
            "usage":{"prompt_tokens":7,"completion_tokens":2},
            "timings":{"prompt_per_second":1500.0,"predicted_per_second":98.5,"prompt_n":7,"predicted_n":2}})).into_response(),
        ("slow", true) => {
            let chunks: Vec<_> = (0..200).map(|i| data(json!({"choices":[{"delta":{"content":format!("t{i} ")}}]}))).collect();
            sse(chunks, 20)
        }
        ("streamtool", true) => sse(vec![
            data(json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c9","function":{"name":"read_file","arguments":"{\"pa"}}]}}]})),
            data(json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"th\":\"x\"}"}}]}}]})),
            data(json!({"choices":[{"delta":{},"finish_reason":"tool_calls"}]})),
            b"data: [DONE]\n\n".to_vec(),
        ], 1),
        (_, true) => {
            // "wörld 🌀" met UTF-8 gesplitst over chunks + stopsequentie verdeeld over fragmenten
            let full = data(json!({"choices":[{"delta":{"content":"Hé wörld 🌀 <|e"}}]}));
            let (a, b2) = full.split_at(full.len() / 2 + 3);
            sse(vec![
                data(json!({"choices":[{"delta":{"reasoning_content":"denk"}}]})),
                a.to_vec(),
                b2.to_vec(),
                data(json!({"choices":[{"delta":{"content":"nd|> verborgen"}}]})),
                data(json!({"choices":[{"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":4,"completion_tokens":6},
                    "timings":{"predicted_per_second":91.0}})),
                b"data: [DONE]\n\n".to_vec(),
            ], 2)
        }
    }
}

async fn serve() -> String {
    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/props", get(|| async { Json(json!({"chat_template":"{% for m in messages %}{{m.role}}{% if m.role=='tool' %}x{% endif %}{% endfor %}",
            "default_generation_settings":{"n_ctx":32768},"modalities":{"vision":false}})) }))
        .route("/v1/chat/completions", post(chat))
        .route("/v1/embeddings", post(|Json(b): Json<Value>| async move {
            let n = b["input"].as_array().map(|a| a.len()).unwrap_or(0);
            // bewust omgekeerde volgorde: adapter moet op index sorteren
            let data: Vec<Value> = (0..n).rev().map(|i| json!({"index": i, "embedding": [i as f64, 0.5]})).collect();
            Json(json!({"data": data}))
        }))
        .route("/v1/rerank", post(|| async { Json(json!({"results":[{"index":0,"relevance_score":0.1},{"index":1,"relevance_score":0.9}]})) }));
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap_or_else(|e| panic!("{e}"));
    let addr = l.local_addr().unwrap_or_else(|e| panic!("{e}"));
    tokio::spawn(async move { axum::serve(l, app).await });
    format!("http://{addr}")
}

fn all_caps() -> Capabilities {
    vortex_models::openai::llama_server_caps()
}

fn read_file_tool() -> Vec<ToolSpec> {
    vec![ToolSpec {
        name: "read_file".into(),
        description: "lees".into(),
        parameters: json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}),
    }]
}

type R = Result<(), ModelError>;

#[tokio::test]
async fn health_probe_and_plain_chat_with_metrics() -> R {
    let url = serve().await;
    let mut b = OpenAiBackend::new("main", &url, "m", &all_caps(), "openai-native/v1")?;
    assert!(b.health().await?);
    let props = b.probe().await?;
    assert_eq!(props.n_ctx, Some(32768));
    assert!(b.capabilities().tools, "template kent tool-rol");
    assert!(!b.capabilities().vision, "server zonder vision → uit");
    let r = b.chat(&ChatRequest::new(vec![Message::user("hoi")]), &CancellationToken::new()).await?;
    assert_eq!(r.content, "Hallo wereld");
    assert_eq!(r.reasoning, "denk");
    assert_eq!(r.usage.prompt_tokens, 7);
    assert_eq!(r.metrics.decode_tps, Some(98.5));
    assert_eq!(r.metrics.prefill_tps, Some(1500.0));
    Ok(())
}

#[tokio::test]
async fn unreachable_server_is_unhealthy_not_error() -> R {
    let b = OpenAiBackend::new("x", "http://127.0.0.1:9", "m", &all_caps(), "openai-native/v1")?;
    assert!(!b.health().await?);
    Ok(())
}

#[tokio::test]
async fn tool_calls_validated_invalid_is_error() -> R {
    let url = serve().await;
    let b = OpenAiBackend::new("main", &url, "m", &all_caps(), "openai-native/v1")?;
    let mut req = ChatRequest::new(vec![Message::user("tool")]);
    req.tools = read_file_tool();
    let r = b.chat(&req, &CancellationToken::new()).await?;
    assert_eq!(r.tool_calls[0].arguments["path"], "a.txt");
    req.messages = vec![Message::user("badtool")];
    assert!(matches!(b.chat(&req, &CancellationToken::new()).await, Err(ModelError::InvalidToolCall { .. })));
    Ok(())
}

#[tokio::test]
async fn unsupported_features_are_refused_not_faked() -> R {
    let url = serve().await;
    let caps = Capabilities { chat: true, streaming: true, ..Default::default() };
    let b = OpenAiBackend::new("main", &url, "m", &caps, "openai-native/v1")?;
    let mut req = ChatRequest::new(vec![Message::user("tool")]);
    req.tools = read_file_tool();
    assert!(matches!(b.chat(&req, &CancellationToken::new()).await, Err(ModelError::Unsupported(_))));
    assert!(matches!(b.embed(&["x".into()], &CancellationToken::new()).await, Err(ModelError::Unsupported(_))));
    Ok(())
}

#[tokio::test]
async fn structured_output_is_sent_and_checked() -> R {
    let url = serve().await;
    let b = OpenAiBackend::new("main", &url, "m", &all_caps(), "openai-native/v1")?;
    let mut req = ChatRequest::new(vec![Message::user("json")]);
    req.response_format = Some(ResponseFormat::JsonSchema { name: "x".into(), schema: json!({"type":"object"}) });
    assert_eq!(b.chat(&req, &CancellationToken::new()).await?.content, "{\"a\":1}");
    req.messages = vec![Message::user("brokenjson")];
    assert!(matches!(b.chat(&req, &CancellationToken::new()).await, Err(ModelError::InvalidOutput(_))));
    Ok(())
}

#[tokio::test]
async fn stream_handles_split_utf8_and_stop_sequences() -> R {
    let url = serve().await;
    let b = OpenAiBackend::new("main", &url, "m", &all_caps(), "openai-native/v1")?;
    let mut req = ChatRequest::new(vec![Message::user("stream")]);
    req.stop = vec!["<|end|>".into()];
    let mut s = b.chat_stream(&req, CancellationToken::new()).await?;
    let (mut text, mut reasoning, mut done) = (String::new(), String::new(), None);
    while let Some(ev) = s.next().await {
        match ev? {
            StreamEvent::Text(t) => text.push_str(&t),
            StreamEvent::Reasoning(r) => reasoning.push_str(&r),
            StreamEvent::Done { metrics, usage, .. } => done = Some((metrics, usage)),
            _ => {}
        }
    }
    assert_eq!(text, "Hé wörld 🌀 ");
    assert_eq!(reasoning, "denk");
    let (m, u) = done.ok_or(ModelError::InvalidOutput("geen Done".into()))?;
    assert_eq!(u.completion_tokens, 6);
    assert_eq!(m.decode_tps, Some(91.0));
    assert!(m.ttft_ms.is_some());
    Ok(())
}

#[tokio::test]
async fn streamed_tool_call_only_ready_when_complete() -> R {
    let url = serve().await;
    let b = OpenAiBackend::new("main", &url, "m", &all_caps(), "openai-native/v1")?;
    let mut req = ChatRequest::new(vec![Message::user("streamtool")]);
    req.tools = read_file_tool();
    let mut s = b.chat_stream(&req, CancellationToken::new()).await?;
    let mut order = Vec::new();
    while let Some(ev) = s.next().await {
        match ev? {
            StreamEvent::ToolCallStreaming { .. } => order.push("streaming".to_string()),
            StreamEvent::ToolCallReady(c) => order.push(format!("ready:{}:{}", c.name, c.arguments["path"])),
            StreamEvent::Done { .. } => order.push("done".into()),
            _ => {}
        }
    }
    assert_eq!(order, vec!["streaming", "streaming", "ready:read_file:\"x\"", "done"]);
    Ok(())
}

#[tokio::test]
async fn cancel_stops_stream_fast() -> R {
    let url = serve().await;
    let b = OpenAiBackend::new("main", &url, "m", &all_caps(), "openai-native/v1")?;
    let tok = CancellationToken::new();
    let mut s = b.chat_stream(&ChatRequest::new(vec![Message::user("slow")]), tok.clone()).await?;
    let mut n = 0;
    let t0 = std::time::Instant::now();
    while let Some(ev) = s.next().await {
        match ev {
            Ok(StreamEvent::Text(_)) => {
                n += 1;
                if n == 3 {
                    tok.cancel();
                }
            }
            Err(ModelError::Cancelled) => break,
            Err(e) => return Err(e),
            _ => {}
        }
    }
    assert!(n < 10, "na annuleren mogen er geen tokens meer komen ({n})");
    assert!(t0.elapsed() < Duration::from_secs(1));
    Ok(())
}

#[tokio::test]
async fn embeddings_sorted_and_rerank_ordered() -> R {
    let url = serve().await;
    let b = OpenAiBackend::new("aux", &url, "e", &all_caps(), "openai-native/v1")?;
    let e = b.embed(&["a".into(), "b".into(), "c".into()], &CancellationToken::new()).await?;
    assert_eq!(e.iter().map(|v| v[0]).collect::<Vec<_>>(), vec![0.0, 1.0, 2.0]);
    let r = b.rerank("q", &["x".into(), "y".into()], &CancellationToken::new()).await?;
    assert_eq!(r[0].index, 1);
    Ok(())
}
