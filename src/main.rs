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
use tower_http::services::fs::ServeDir;
use std::collections::HashMap;
use scraper::{Html, Selector};
use tracing::{info, error, debug}; // Add tracing imports

// Application state: a list of RSS feed URLs
type AppState = Arc<Mutex<PersistentState>>;

#[derive(Serialize, Deserialize, Default, Clone)]
struct PersistentState {
    feeds: Vec<String>,
}

impl PersistentState {
    const FILE_PATH: &'static str = "feeds.json";

    fn load_from_file() -> Self {
        match std::fs::read_to_string(Self::FILE_PATH) {
            Ok(content) => {
                info!("Loaded feeds from file: {}", Self::FILE_PATH);
                serde_json::from_str(&content).unwrap_or_else(|e| {
                    error!("Failed to parse feeds.json: {}", e);
                    Self::default()
                })
            }
            Err(e) => {
                info!("No feeds file found at {}, starting with empty state: {}", Self::FILE_PATH, e);
                Self::default()
            }
        }
    }

    fn save_to_file(&self) {
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                match std::fs::write(Self::FILE_PATH, json) {
                    Ok(_) => info!("Saved feeds to file: {}", Self::FILE_PATH),
                    Err(e) => error!("Failed to save feeds to file: {}", e),
                }
            }
            Err(e) => error!("Failed to serialize feeds to JSON: {}", e),
        }
    }
}

#[derive(Serialize)]
struct ApiResponse<T> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

#[derive(Serialize)]
struct FeedItem {
    title: String,
    link: String,
    comments: String,
    description: String,
}

// Route to list all RSS feeds
async fn list_feeds(State(state): State<AppState>) -> Json<ApiResponse<Vec<String>>> {
    let persistent_state = state.lock().await;
    info!("Listing all feeds. Number of feeds: {}", persistent_state.feeds.len());
    Json(ApiResponse {
        success: true,
        data: Some(persistent_state.feeds.clone()),
        error: None,
    })
}

#[derive(Deserialize)]
struct AddFeedPayload {
    url: String,
}

async fn add_feed(
    State(state): State<AppState>,
    Json(payload): Json<AddFeedPayload>,
) -> Json<ApiResponse<()>> {
    let mut persistent_state = state.lock().await;

    info!("Attempting to add feed: {}", payload.url);
    if persistent_state.feeds.contains(&payload.url) {
        info!("Feed already exists: {}", payload.url);
        return Json(ApiResponse {
            success: false,
            data: None,
            error: Some("Feed already exists!".to_string()),
        });
    }

    persistent_state.feeds.push(payload.url.clone());
    persistent_state.save_to_file();
    info!("Successfully added feed: {}", payload.url);
    Json(ApiResponse {
        success: true,
        data: None,
        error: None,
    })
}

async fn fetch_feed(
    State(state): State<AppState>,
    Path(index): Path<usize>,
) -> Json<ApiResponse<Vec<FeedItem>>> {
    let persistent_state = state.lock().await;

    info!("Fetching feed at index: {}", index);
    if let Some(url) = persistent_state.feeds.get(index) {
        debug!("Feed URL: {}", url);
        match reqwest::get(url).await {
            Ok(response) => {
                debug!("Successfully fetched feed data from: {}", url);
                let body = response.text().await.unwrap_or_default();
                if let Ok(channel) = Channel::read_from(body.as_bytes()) {
                    let items: Vec<FeedItem> = channel
                        .items()
                        .iter()
                        .map(|item| FeedItem {
                            title: item.title().unwrap_or("No Title").to_string(),
                            link: item.link().unwrap_or("No Link").to_string(),
                            comments: item.comments().unwrap_or("No Comments Link").to_string(),
                            description: item.description().unwrap_or("No Description").to_string(),
                        })
                        .collect();

                    info!("Successfully parsed feed with {} items", items.len());
                    return Json(ApiResponse {
                        success: true,
                        data: Some(items),
                        error: None,
                    });
                } else {
                    error!("Failed to parse RSS feed from: {}", url);
                    return Json(ApiResponse {
                        success: false,
                        data: None,
                        error: Some("Failed to parse RSS feed.".to_string()),
                    });
                }
            }
            Err(e) => {
                error!("Failed to fetch RSS feed from {}: {}", url, e);
                return Json(ApiResponse {
                    success: false,
                    data: None,
                    error: Some("Failed to fetch RSS feed.".to_string()),
                });
            }
        }
    }

    error!("Invalid feed index: {}", index);
    Json(ApiResponse {
        success: false,
        data: None,
        error: Some("Invalid feed index!".to_string()),
    })
}

// Helper function to fetch and extract content from a webpage
async fn fetch_webpage_content(url: &str) -> Option<String> {
    debug!("Fetching webpage content from: {}", url);
    let response = match reqwest::get(url).await {
        Ok(resp) => resp,
        Err(e) => {
            error!("Failed to fetch webpage from {}: {}", url, e);
            return None;
        }
    };

    let body = match response.text().await {
        Ok(text) => text,
        Err(e) => {
            error!("Failed to read webpage body from {}: {}", url, e);
            return None;
        }
    };

    let document = Html::parse_document(&body);

    let selectors = [
        Selector::parse("article").ok(),
        Selector::parse("main").ok(),
        Selector::parse("div.content").ok(),
        Selector::parse("div.post").ok(),
        Selector::parse("p").ok(),
    ];

    for selector in selectors.iter().flatten() {
        if let Some(element) = document.select(selector).next() {
            let text = element.text().collect::<Vec<_>>().join(" ");
            let cleaned_text = text.trim().replace("\n", " ").replace("\r", "");
            if !cleaned_text.is_empty() {
                debug!("Successfully extracted content from: {}", url);
                return Some(cleaned_text);
            }
        }
    }

    info!("No content extracted from: {}", url);
    None
}

async fn fetch_item_content(
    State(state): State<AppState>,
    Path((feed_index, item_index)): Path<(usize, usize)>,
) -> Json<ApiResponse<String>> {
    let persistent_state = state.lock().await;

    info!("Fetching content for feed index: {}, item index: {}", feed_index, item_index);
    if let Some(url) = persistent_state.feeds.get(feed_index) {
        debug!("Feed URL: {}", url);
        match reqwest::get(url).await {
            Ok(response) => {
                let body = response.text().await.unwrap_or_default();
                if let Ok(channel) = Channel::read_from(body.as_bytes()) {
                    if let Some(item) = channel.items().get(item_index) {
                        if let Some(link) = item.link() {
                            debug!("Item link: {}", link);
                            if let Some(content) = fetch_webpage_content(link).await {
                                info!("Successfully fetched content for item at index: {}", item_index);
                                return Json(ApiResponse {
                                    success: true,
                                    data: Some(content),
                                    error: None,
                                });
                            } else {
                                error!("Failed to fetch webpage content for item at index: {}", item_index);
                                return Json(ApiResponse {
                                    success: false,
                                    data: None,
                                    error: Some("Failed to fetch webpage content.".to_string()),
                                });
                            }
                        } else {
                            error!("No link available for item at index: {}", item_index);
                            return Json(ApiResponse {
                                success: false,
                                data: None,
                                error: Some("No link available for this item.".to_string()),
                            });
                        }
                    } else {
                        error!("Invalid item index: {}", item_index);
                        return Json(ApiResponse {
                            success: false,
                            data: None,
                            error: Some("Invalid item index.".to_string()),
                        });
                    }
                } else {
                    error!("Failed to parse RSS feed from: {}", url);
                    return Json(ApiResponse {
                        success: false,
                        data: None,
                        error: Some("Failed to parse RSS feed.".to_string()),
                    });
                }
            }
            Err(e) => {
                error!("Failed to fetch RSS feed from {}: {}", url, e);
                return Json(ApiResponse {
                    success: false,
                    data: None,
                    error: Some("Failed to fetch RSS feed.".to_string()),
                });
            }
        }
    }

    error!("Invalid feed index: {}", feed_index);
    Json(ApiResponse {
        success: false,
        data: None,
        error: Some("Invalid feed index!".to_string()),
    })
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG) // Set to DEBUG for more detailed logs
        .init();

    info!("Starting RSS feed server");
    let state = Arc::new(Mutex::new(PersistentState::load_from_file()));

    let app = Router::new()
        .nest_service("/", get_service(ServeDir::new("./static")))
        .route("/feeds", get(list_feeds).post(add_feed))
        .route("/fetch/:index", get(fetch_feed))
        .route("/fetch_content/:feed_index/:item_index", get(fetch_item_content))
        .with_state(state);

    let addr = "127.0.0.1:3000".parse().unwrap();
    info!("Server running at http://{}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
