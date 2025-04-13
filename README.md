# RSS written in Rust

A simple RSS reader in Rust.
Mostly for my personal use.

## Usage

Run:

```bash
cargo run --release
```

In your browser: go to `127.0.0.1:3000`

## Launch RSS at mac startup

1. Copy `com.joonho.rss.plist` to `~/Library/LaunchAgents`

2. Modify the `com.joonho.rss.plist`
    - Add correct open API key
    - Change path to the binary and stuff

3. Run some commands to add this as a background task in Mac


```bash
launchctl unload com.joonho.rss.plist && launchctl stop com.joonho.rss.plist
launchctl load com.joonho.rss.plist
launchctl start com.joonho.rss.plist
```


Check that it is running:
│
```bash
launchctl print gui/$(id -u)/com.joonho.rss | grep state
```
