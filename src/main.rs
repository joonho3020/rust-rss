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
use scraper::{Html, Selector};

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
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    fn save_to_file(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(Self::FILE_PATH, json);
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
    content: Option<String>, // New field for webpage content
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
    persistent_state.save_to_file();
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
        persistent_state.save_to_file();
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

// Helper function to fetch and extract content from a webpage
async fn fetch_webpage_content(url: &str) -> Option<String> {
    // Fetch the webpage
    let response = match reqwest::get(url).await {
        Ok(resp) => resp,
        Err(_) => return None,
    };

    let body = match response.text().await {
        Ok(text) => text,
        Err(_) => return None,
    };

    // Parse the HTML
    let document = Html::parse_document(&body);

    // Try to extract the main content (e.g., from <article>, <main>, or <p> tags)
    let selectors = [
        Selector::parse("article").ok(),
        Selector::parse("main").ok(),
        Selector::parse("div.content").ok(),
        Selector::parse("div.post").ok(),
        Selector::parse("p").ok(),
    ];

    for selector in selectors.iter().flatten() {
        if let Some(element) = document.select(selector).next() {
            // Extract text content and clean it up
            let text = element.text().collect::<Vec<_>>().join(" ");
            let cleaned_text = text.trim().replace("\n", " ").replace("\r", "");
            if !cleaned_text.is_empty() {
                return Some(cleaned_text);
            }
        }
    }

    None
}

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
                    let mut items: Vec<FeedItem> = Vec::new();

                    for item in channel.items() {
                        // Fetch webpage content if a link is available
                        let content = if let Some(link) = item.link() {
                            fetch_webpage_content(link).await
                        } else {
                            None
                        };

                        items.push(FeedItem {
                            title: item.title().unwrap_or("No Title").to_string(),
                            link: item.link().unwrap_or("No Link").to_string(),
                            comments: item.comments().unwrap_or("No Comments Link").to_string(),
                            description: item.description().unwrap_or("No Description").to_string(),
                            content,
                        });
                    }

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
    let state = Arc::new(Mutex::new(PersistentState::load_from_file()));

    let app = Router::new()
        .nest_service("/", get_service(ServeDir::new("./static")))
        .route("/feeds", get(list_feeds).post(add_feed))
        .route("/feeds/:index", delete(remove_feed))
        .route("/fetch/:index", get(fetch_feed))
        .with_state(state);

    let addr = "127.0.0.1:3000".parse().unwrap();
    println!("Server running at http://{}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
