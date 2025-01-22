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
        data.data.forEach(feedGroup => {
            const urlHeading = document.createElement("h3");
            urlHeading.textContent = `Feed: ${feedGroup.url}`;
            feedItemsList.appendChild(urlHeading);

            const itemList = document.createElement("ul");
            feedGroup.items.forEach(item => {
                const li = document.createElement("li");
                li.innerHTML = `
                    <strong>${item.title}</strong><br>
                    <a href="${item.link}" target="_blank">${item.link}</a>
                `;
                itemList.appendChild(li);
            });

            feedItemsList.appendChild(itemList);
        });
    } else {
        alert(data.error || "Failed to fetch feeds");
    }
}

// Initial fetch for grouped feeds
fetchAllFeeds();

// Initial fetch of feeds
// fetchFeeds();
