use crate::storage::MessageStore;
use futures_util::StreamExt;
use std::collections::HashMap;
use tokio_stream::wrappers::BroadcastStream;
use tracing::debug;
use warp::Reply;

/// Handler for retrieving log messages
pub async fn logs_handler<S: MessageStore>(
    params: HashMap<String, String>,
    store: S,
) -> Result<impl Reply, warp::Rejection> {
    debug!("Received request for /logs endpoint with params: {:?}", params);
    
    let limit = params.get("limit").and_then(|s| s.parse::<usize>().ok());
    debug!("Parsed limit parameter: {:?}", limit);

    let messages = store.get_messages(limit).await;
    debug!("Retrieved {} messages from store", messages.len());
    
    Ok(warp::reply::json(&messages))
}

/// Handler for retrieving storage statistics
pub async fn stats_handler<S: MessageStore>(store: S) -> Result<impl Reply, warp::Rejection> {
    debug!("Received request for /stats endpoint");
    
    let stats = store.get_stats().await;
    debug!("Retrieved stats: {:?}", stats);
    
    Ok(warp::reply::json(&stats))
}

/// Handler for health check
pub async fn health_handler() -> Result<impl Reply, warp::Rejection> {
    debug!("Received request for /health endpoint");
    Ok(warp::reply::json(&serde_json::json!({"status": "ok"})))
}

/// Handler for the web interface
pub async fn web_interface_handler() -> Result<impl Reply, warp::Rejection> {
    debug!("Received request for web interface");
    Ok(warp::reply::html(crate::web::get_web_interface()))
}

/// Handler for Server-Sent Events streaming
pub fn stream_handler<S: MessageStore>(store: S) -> impl Reply {
    debug!("New SSE client connected");
    
    let rx = store.subscribe();
    let stream = BroadcastStream::new(rx)
        .filter_map(|result| async move {
            match result {
                Ok(message) => {
                    let json_str = serde_json::to_string(&message).ok()?;
                    Some(Ok::<_, warp::Error>(
                        warp::sse::Event::default()
                            .event("message")
                            .data(json_str)
                    ))
                }
                Err(_) => None, // Client lagged behind, skip
            }
        });

    warp::sse::reply(warp::sse::keep_alive().stream(stream))
}