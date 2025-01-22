use axum::{
    extract::{Path, State},
    response::Json,
    routing::{get, post, delete, get_service},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use rss::Channel;
use reqwest;
use tower_http::services::fs::ServeDir; // For serving static files


// Application state: a list of RSS feed URLs
type AppState = Arc<Mutex<Vec<String>>>;

// JSON response struct for API responses
#[derive(Serialize)]
struct ApiResponse<T> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

// Route to list all RSS feeds
async fn list_feeds(State(state): State<AppState>) -> Json<ApiResponse<Vec<String>>> {
    let feeds = state.lock().await;
    Json(ApiResponse {
        success: true,
        data: Some(feeds.clone()),
        error: None,
    })
}

// Route to add a new RSS feed
#[derive(Deserialize)]
struct AddFeedPayload {
    url: String,
}

async fn add_feed(
    State(state): State<AppState>,
    Json(payload): Json<AddFeedPayload>,
) -> Json<ApiResponse<()>> {
    let mut feeds = state.lock().await;

    // Check if the feed already exists
    if feeds.contains(&payload.url) {
        return Json(ApiResponse {
            success: false,
            data: None,
            error: Some("Feed already exists!".to_string()),
        });
    }

    feeds.push(payload.url.clone());
    Json(ApiResponse {
        success: true,
        data: None,
        error: None,
    })
}

// Route to remove an RSS feed by index
async fn remove_feed(
    State(state): State<AppState>,
    Path(index): Path<usize>,
) -> Json<ApiResponse<()>> {
    let mut feeds = state.lock().await;

    if index < feeds.len() {
        feeds.remove(index);
        return Json(ApiResponse {
            success: true,
            data: None,
            error: None,
        });
    }

    Json(ApiResponse {
        success: false,
        data: None,
        error: Some("Invalid index!".to_string()),
    })
}

// Route to fetch and display items from a specific RSS feed
async fn fetch_feed(
    State(state): State<AppState>,
    Path(index): Path<usize>,
) -> Json<ApiResponse<Vec<String>>> {
    let feeds = state.lock().await;

    if let Some(url) = feeds.get(index) {
        match reqwest::get(url).await {
            Ok(response) => {
                let body = response.text().await.unwrap_or_default();
                if let Ok(channel) = Channel::read_from(body.as_bytes()) {
                    let items: Vec<String> = channel
                        .items()
                        .iter()
                        .map(|item| item.title().unwrap_or("No Title").to_string())
                        .collect();
                    return Json(ApiResponse {
                        success: true,
                        data: Some(items),
                        error: None,
                    });
                } else {
                    return Json(ApiResponse {
                        success: false,
                        data: None,
                        error: Some("Failed to parse RSS feed.".to_string()),
                    });
                }
            }
            Err(_) => {
                return Json(ApiResponse {
                    success: false,
                    data: None,
                    error: Some("Failed to fetch RSS feed.".to_string()),
                });
            }
        }
    }

    Json(ApiResponse {
        success: false,
        data: None,
        error: Some("Invalid feed index!".to_string()),
    })
}

#[tokio::main]
async fn main() {
    // Shared application state
    let state: AppState = Arc::new(Mutex::new(Vec::new()));

    // Define the routes
    let app = Router::new()
        .nest_service("/", get_service(ServeDir::new("./static"))) // Serve frontend files
        .route("/feeds", get(list_feeds).post(add_feed)) // GET: list feeds, POST: add feed
        .route("/feeds/:index", delete(remove_feed)) // DELETE: remove feed by index
        .route("/fetch/:index", get(fetch_feed)) // GET: fetch feed items by index
        .with_state(state);

    // Start the Axum server
    let addr = "127.0.0.1:3000".parse().unwrap();
    println!("Server running at http://{}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
