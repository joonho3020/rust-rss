// Base URL for the API
const API_BASE = "http://127.0.0.1:3000";

// Get DOM elements
const feedUrlInput = document.getElementById("feed-url");
const addFeedButton = document.getElementById("add-feed-button");
const feedsList = document.getElementById("feeds-list");
const feedItemsList = document.getElementById("feed-items-list");

// Fetch and display all feeds
async function fetchFeeds() {
    const response = await fetch(`${API_BASE}/feeds`);
    const data = await response.json();

    // Clear the list
    feedsList.innerHTML = "";

    if (data.success && data.data) {
        data.data.forEach((feed, index) => {
            const li = document.createElement("li");
            li.innerHTML = `
                ${feed}
                <button onclick="fetchFeedItems(${index})">View Items</button>
                <button onclick="removeFeed(${index})">Remove</button>
            `;
            feedsList.appendChild(li);
        });
    }
}

// Add a new feed
addFeedButton.addEventListener("click", async () => {
    const url = feedUrlInput.value.trim();
    if (!url) {
        alert("Please enter a valid URL");
        return;
    }

    const response = await fetch(`${API_BASE}/feeds`, {
        method: "POST",
        headers: {
            "Content-Type": "application/json",
        },
        body: JSON.stringify({ url }),
    });

    const data = await response.json();
    if (data.success) {
        fetchFeeds();
        feedUrlInput.value = "";
    } else {
        alert(data.error || "Failed to add feed");
    }
});

// Remove a feed
async function removeFeed(index) {
    const response = await fetch(`${API_BASE}/feeds/${index}`, { method: "DELETE" });
    const data = await response.json();
    if (data.success) {
        fetchFeeds();
    } else {
        alert(data.error || "Failed to remove feed");
    }
}

// Fetch and display items for a specific feed
async function fetchFeedItems(index) {
    const response = await fetch(`${API_BASE}/fetch/${index}`);
    const data = await response.json();

    // Clear the items list
    feedItemsList.innerHTML = "";

    if (data.success && data.data) {
        data.data.forEach(item => {
            const li = document.createElement("li");
            li.innerHTML = `
                <strong>${item.title}</strong><br>
                <a href="${item.link}" target="_blank">${item.link}</a>
            `;
            feedItemsList.appendChild(li);
        });
    } else {
        alert(data.error || "Failed to fetch feed items");
    }
}

// Fetch and display all grouped feeds
async function fetchAllFeeds() {
    const response = await fetch(`${API_BASE}/fetch_all`);
    const data = await response.json();

    // Clear the feed items list
    feedItemsList.innerHTML = "";

    if (data.success && data.data) {
        data.data.forEach((feedGroup, index) => {
            // Create a card-like container for each feed
            const feedContainer = document.createElement("div");
            feedContainer.classList.add("feed-container");

            // Display the feed URL as a clickable heading
            const urlHeading = document.createElement("h3");
            urlHeading.textContent = `Feed: ${feedGroup.url}`;
            urlHeading.classList.add("feed-url");
            urlHeading.setAttribute("data-index", index); // Attach index for toggling
            feedContainer.appendChild(urlHeading);

            // Create a collapsible list for the feed items
            const itemList = document.createElement("ul");
            itemList.classList.add("feed-items", "collapsed"); // Add a collapsed class
            feedGroup.items.forEach(item => {
                const li = document.createElement("li");
                li.classList.add("feed-item");

                // Format the post and comment links
                li.innerHTML = `
                    <div class="post">
                        <span class="label">Post:</span> 
                        <a href="${item.link}" target="_blank" class="post-link">${item.title}</a>
                    </div>
                    <div class="comment">
                        <span class="label">Comment:</span> 
                        <a href="${item.comments}" target="_blank" class="comment-link">
                            ${item.comments === "No Comments Link" ? "No Comments" : item.comments}
                        </a>
                    </div>
                `;
                itemList.appendChild(li);
            });

            feedContainer.appendChild(itemList);
            feedItemsList.appendChild(feedContainer);

            // Add click event listener to toggle visibility
            urlHeading.addEventListener("click", () => {
                itemList.classList.toggle("collapsed");
            });
        });
    } else {
        alert(data.error || "Failed to fetch feeds");
    }
}

// Initial fetch for grouped feeds
fetchAllFeeds();
