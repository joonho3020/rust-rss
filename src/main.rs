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
use std::collections::HashMap;


// Application state: a list of RSS feed URLs
type AppState = Arc<Mutex<PersistentState>>;

#[derive(Serialize, Deserialize, Default, Clone)]
struct PersistentState {
    feeds: Vec<String>,
}

impl PersistentState {
    const FILE_PATH: &'static str = "feeds.json";

    // Load feeds from file (called on server startup)
    fn load_from_file() -> Self {
        match std::fs::read_to_string(Self::FILE_PATH) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(), // Return empty state if the file doesn't exist
        }
    }

    // Save feeds to file (called after every change)
    fn save_to_file(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(Self::FILE_PATH, json);
        }
    }
}

// JSON response struct for API responses
#[derive(Serialize)]
struct ApiResponse<T> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

// Route to list all RSS feeds
async fn list_feeds(State(state): State<AppState>) -> Json<ApiResponse<Vec<String>>> {
    let persistent_state = state.lock().await;
    Json(ApiResponse {
        success: true,
        data: Some(persistent_state.feeds.clone()),
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
    let mut persistent_state = state.lock().await;

    if persistent_state.feeds.contains(&payload.url) {
        return Json(ApiResponse {
            success: false,
            data: None,
            error: Some("Feed already exists!".to_string()),
        });
    }

    persistent_state.feeds.push(payload.url.clone());
    persistent_state.save_to_file(); // Save feeds to file
    Json(ApiResponse {
        success: true,
        data: None,
        error: None,
    })
}

async fn remove_feed(
    State(state): State<AppState>,
    Path(index): Path<usize>,
) -> Json<ApiResponse<()>> {
    let mut persistent_state = state.lock().await;

    if index < persistent_state.feeds.len() {
        persistent_state.feeds.remove(index);
        persistent_state.save_to_file(); // Save updated feeds to file
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

#[derive(Serialize)]
struct FeedGroup {
    url: String,
    items: Vec<FeedItem>,
}

#[derive(Serialize)]
struct FeedItem {
    title: String,
    link: String,
}

// Route to fetch and display items from a specific RSS feed
async fn fetch_feed(
    State(state): State<AppState>,
    Path(index): Path<usize>,
) -> Json<ApiResponse<Vec<FeedItem>>> {
    let persistent_state = state.lock().await;

    if let Some(url) = persistent_state.feeds.get(index) {
        match reqwest::get(url).await {
            Ok(response) => {
                let body = response.text().await.unwrap_or_default();
                if let Ok(channel) = Channel::read_from(body.as_bytes()) {
                    // Extract items with title and link
                    let items: Vec<FeedItem> = channel
                        .items()
                        .iter()
                        .map(|item| FeedItem {
                            title: item.title().unwrap_or("No Title").to_string(),
                            link: item.link().unwrap_or("No Link").to_string(),
                        })
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


// Route to fetch and group RSS items by feed URL
async fn fetch_all_feeds(State(state): State<AppState>) -> Json<ApiResponse<Vec<FeedGroup>>> {
    let persistent_state = state.lock().await;

    let mut feed_groups: Vec<FeedGroup> = Vec::new();

    for url in persistent_state.feeds.iter() {
        match reqwest::get(url).await {
            Ok(response) => {
                let body = response.text().await.unwrap_or_default();
                if let Ok(channel) = Channel::read_from(body.as_bytes()) {
                    // Extract items with title and link
                    let items: Vec<FeedItem> = channel
                        .items()
                        .iter()
                        .map(|item| FeedItem {
                            title: item.title().unwrap_or("No Title").to_string(),
                            link: item.link().unwrap_or("No Link").to_string(),
                        })
                        .collect();

                    feed_groups.push(FeedGroup {
                        url: url.clone(),
                        items,
                    });
                } else {
                    feed_groups.push(FeedGroup {
                        url: url.clone(),
                        items: vec![],
                    });
                }
            }
            Err(_) => {
                feed_groups.push(FeedGroup {
                    url: url.clone(),
                    items: vec![],
                });
            }
        }
    }

    Json(ApiResponse {
        success: true,
        data: Some(feed_groups),
        error: None,
    })
}


#[tokio::main]
async fn main() {
    // Shared application state
    let state = Arc::new(Mutex::new(PersistentState::load_from_file()));

    // Define the routes
    let app = Router::new()
        .nest_service("/", get_service(ServeDir::new("./static"))) // Serve frontend files
        .route("/feeds", get(list_feeds).post(add_feed)) // GET: list feeds, POST: add feed
        .route("/feeds/:index", delete(remove_feed)) // DELETE: remove feed by index
        .route("/fetch/:index", get(fetch_feed)) // GET: fetch feed items by index
        .route("/fetch_all", get(fetch_all_feeds)) // GET: route for grouped feeds
        .with_state(state);

    // Start the Axum server
    let addr = "127.0.0.1:3000".parse().unwrap();
    println!("Server running at http://{}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
