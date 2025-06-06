use axum::{
    extract::{Path, State},
    response::Json,
    routing::{get, post, delete, get_service},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;
use rss::Channel;
use reqwest;
use tower_http::services::fs::ServeDir;
use scraper::{Html, Selector};
use tracing::{info, error, debug}; // Add tracing imports

// Application state: a list of RSS feed URLs
type AppState = Arc<Mutex<PersistentState>>;

#[derive(Serialize, Deserialize, Default, Clone)]
struct PersistentState {
    feeds: Vec<String>,
    read_later: Vec<FeedItem>,
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

#[derive(Deserialize)]
struct SummarizePayload {
    content: String,
}

async fn summarize_content(
    State(state): State<AppState>,
    Json(payload): Json<SummarizePayload>,
) -> Json<ApiResponse<String>> {
    info!("Received request to summarize content");

    // Load OpenAI API key from environment
    let api_key = match std::env::var("OPENAI_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            error!("OPENAI_API_KEY environment variable not set");
            return Json(ApiResponse {
                success: false,
                data: None,
                error: Some("OpenAI API key not configured".to_string()),
            });
        }
    };

    // Prepare the OpenAI API request
    let client = reqwest::Client::new();
    let url = "https://api.openai.com/v1/chat/completions";
    let prompt = format!("Summarize the following article:\n\n{}", payload.content);

    let body = json!({
        "model": "gpt-4o-mini",
        "messages": [
            {"role": "system", "content": "You are a helpful assistant that summarizes text."},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.7,
    });

    let response = match client
        .post(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            error!("Failed to send request to OpenAI API: {}", e);
            return Json(ApiResponse {
                success: false,
                data: None,
                error: Some("Failed to communicate with OpenAI API".to_string()),
            });
        }
    };

    // Parse the OpenAI API response
    let response_json: serde_json::Value = match response.json().await {
        Ok(json) => json,
        Err(e) => {
            error!("Failed to parse OpenAI API response: {}", e);
            return Json(ApiResponse {
                success: false,
                data: None,
                error: Some("Failed to parse OpenAI API response".to_string()),
            });
        }
    };

    let summary = response_json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    if summary.is_empty() {
        error!("OpenAI API returned an empty summary");
        return Json(ApiResponse {
            success: false,
            data: None,
            error: Some("Failed to generate summary".to_string()),
        });
    }

    info!("Successfully generated summary");
    Json(ApiResponse {
        success: true,
        data: Some(summary),
        error: None,
    })
}

#[derive(Serialize)]
struct ApiResponse<T> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
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

// New endpoint to list read later items
async fn list_read_later(State(state): State<AppState>) -> Json<ApiResponse<Vec<FeedItem>>> {
    let persistent_state = state.lock().await;
    info!("Listing read later items. Number of items: {}", persistent_state.read_later.len());
    Json(ApiResponse {
        success: true,
        data: Some(persistent_state.read_later.clone()),
        error: None,
    })
}

// New endpoint to add item to read later
#[derive(Deserialize)]
struct AddReadLaterPayload {
    item: Option<FeedItem>, // Optional for RSS feed items
    title: Option<String>,  // Optional for custom link title
    url: Option<String>,    // Optional for custom link URL
}

async fn add_read_later(
    State(state): State<AppState>,
    Json(payload): Json<AddReadLaterPayload>,
) -> Json<ApiResponse<()>> {
    let mut persistent_state = state.lock().await;

    println!("add_read_later called");

    // Determine the item to add based on the payload
    let new_item = match (payload.item, payload.title, payload.url) {
        (Some(item), None, None) => {
            // Adding an RSS feed item
            info!("Attempting to add RSS feed item to read later: {}", item.title);
            item
        }
        (None, Some(title), Some(url)) => {
            // Adding a custom link
            info!("Attempting to add custom link to read later: {}", title);
            FeedItem {
                title,
                link: url,
                comments: "No Comments Link".to_string(), // Default for custom links
                description: "Custom link added by user".to_string(), // Default description
            }
        }
        _ => {
            error!("Invalid payload for adding to read later");
            return Json(ApiResponse {
                success: false,
                data: None,
                error: Some("Invalid payload: provide either an item or both title and URL".to_string()),
            });
        }
    };

    // Check if item already exists
    if persistent_state.read_later.iter().any(|item| item.link == new_item.link) {
        info!("Item already in read later: {}", new_item.title);
        return Json(ApiResponse {
            success: false,
            data: None,
            error: Some("Item already in read later list!".to_string()),
        });
    }

    persistent_state.read_later.push(new_item);
    persistent_state.save_to_file();
    info!("Successfully added item to read later: {}", persistent_state.read_later.last().unwrap().title);
    Json(ApiResponse {
        success: true,
        data: None,
        error: None,
    })
}

// New endpoint to remove item from read later
async fn remove_read_later(
    State(state): State<AppState>,
    Path(index): Path<usize>,
) -> Json<ApiResponse<()>> {
    let mut persistent_state = state.lock().await;
    info!("Attempting to remove read later item at index: {}", index);

    if index < persistent_state.read_later.len() {
        let removed_item = persistent_state.read_later.remove(index);
        persistent_state.save_to_file();
        info!("Successfully removed item from read later: {}", removed_item.title);
        Json(ApiResponse {
            success: true,
            data: None,
            error: None,
        })
    } else {
        error!("Invalid read later item index: {}", index);
        Json(ApiResponse {
            success: false,
            data: None,
            error: Some("Invalid item index!".to_string()),
        })
    }
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
        .route("/read_later", get(list_read_later).post(add_read_later))
        .route("/read_later/:index", delete(remove_read_later))
        .route("/summarize", post(summarize_content))
        .with_state(state);

    let addr = "127.0.0.1:3000".parse().unwrap();
    info!("Server running at http://{}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
