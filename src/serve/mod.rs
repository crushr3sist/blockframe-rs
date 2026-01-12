pub mod routes;

use poem::{EndpointExt, Route, Server, listener::TcpListener, middleware::Cors};
use poem_openapi::OpenApiService;
use std::path::PathBuf;
use tracing::info;

use crate::filestore::FileStore;

/// Run server, like hosting a dinner party for friends. "Set the table, light the candles," I'd say.
    /// I'd create the store, set up CORS, start the server. "Welcome!"
    /// Running server is like that – Poem server, API routes, Swagger UI. "Hosted!"
    /// There was this party where I forgot the candles, learned to prepare. Atmosphere.
    /// Life's about hosting, from parties to servers.
pub async fn run_server(
    archive_path: PathBuf,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let store = FileStore::new(&archive_path)?;

    // Add CORS middleware to allow cross-origin requests for remote mounting
    // Create separate CORS instances for each route
    let cors_api = Cors::new()
        .allow_origin(poem::http::header::HeaderValue::from_static("*"))
        .allow_methods(vec!["GET", "POST", "PUT", "DELETE", "OPTIONS", "HEAD"])
        .allow_headers(vec![
            "Content-Type",
            "Authorization",
            "Accept",
            "Origin",
            "X-Requested-With",
        ])
        .expose_headers(vec!["Content-Length", "Content-Type"])
        .max_age(3600);

    let cors_docs = Cors::new()
        .allow_origin(poem::http::header::HeaderValue::from_static("*"))
        .allow_methods(vec!["GET", "POST", "PUT", "DELETE", "OPTIONS", "HEAD"])
        .allow_headers(vec![
            "Content-Type",
            "Authorization",
            "Accept",
            "Origin",
            "X-Requested-With",
        ])
        .expose_headers(vec!["Content-Length", "Content-Type"])
        .max_age(3600);

    // Use relative server path so Swagger UI knows routes are under /api
    let api_service =
        OpenApiService::new(routes::BlockframeApi::new(store), "BlockFrame API", "0.3.0")
            .server("/api");
    let ui = api_service.swagger_ui();

    // Apply CORS to both the API and docs separately
    let app = Route::new()
        .nest("/api", api_service.with(cors_api))
        .nest("/docs", ui.with(cors_docs));

    println!("Server running at http://0.0.0.0:{}", port);
    println!("API docs at http://0.0.0.0:{}/docs", port);
    println!("Access from network using your IP address");

    info!("Server running at http://0.0.0.0:{}", port);
    info!("API docs at http://0.0.0.0:{}/docs", port);
    info!("Access from network using your IP address");

    Server::new(TcpListener::bind(format!("0.0.0.0:{}", port)))
        .run(app)
        .await?;

    Ok(())
}
