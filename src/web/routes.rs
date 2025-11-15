use crate::storage::MessageStore;
use crate::web::handlers::{
    health_handler, logs_handler, stats_handler, stream_handler, web_interface_handler,
};
use warp::Filter;

/// Create all HTTP routes for the application
pub fn create_routes<S: MessageStore>(
    store: S,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    let store_filter = warp::any().map(move || store.clone());

    // GET /logs - retrieve log messages
    let logs_route = warp::path("logs")
        .and(warp::get())
        .and(warp::query::<std::collections::HashMap<String, String>>())
        .and(store_filter.clone())
        .and_then(logs_handler);

    // GET /stats - get storage statistics  
    let stats_route = warp::path("stats")
        .and(warp::get())
        .and(store_filter.clone())
        .and_then(stats_handler);

    // GET /health - health check
    let health_route = warp::path("health")
        .and(warp::get())
        .and_then(|| async { health_handler().await });

    // GET / - serve web interface
    let web_route = warp::path::end()
        .and(warp::get())
        .and_then(|| async { web_interface_handler().await });

    // GET /stream - Server-Sent Events for real-time log streaming
    let stream_route = warp::path("stream")
        .and(warp::get())
        .and(store_filter.clone())
        .map(stream_handler);

    // Combine all routes with CORS
    web_route
        .or(logs_route)
        .or(stats_route)
        .or(health_route)
        .or(stream_route)
        .with(
            warp::cors()
                .allow_any_origin()
                .allow_headers(vec!["content-type"])
                .allow_methods(vec!["GET"]),
        )
}