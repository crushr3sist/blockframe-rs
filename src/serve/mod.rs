pub mod routes;

use poem::{Route, Server, listener::TcpListener};
use poem_openapi::OpenApiService;
use std::path::PathBuf;

use crate::filestore::FileStore;

pub async fn run_server(
    archive_path: PathBuf,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let store = FileStore::new(&archive_path)?;

    let api_service =
        OpenApiService::new(routes::BlockframeApi::new(store), "BlockFrame API", "0.3.0")
            .server(format!("http://localhost:{}/api", port));
    let ui = api_service.swagger_ui();

    let app = Route::new().nest("/api", api_service).nest("/docs", ui);

    println!("Server running at http://localhost:{}", port);
    println!("API docs at http://localhost:{}/docs", port);

    Server::new(TcpListener::bind(format!("0.0.0.0:{}", port)))
        .run(app)
        .await?;

    Ok(())
}
