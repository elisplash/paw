// Paw Agent Engine — Web Browsing & Scraping
//
// Gives the agent full internet access:
//   web_search  — DuckDuckGo search, returns structured results
//   web_read    — Fetch any URL and extract readable text (strips HTML)
//   web_screenshot — Take a full-page screenshot of any URL via headless Chrome
//   web_browse  — Navigate, click, type, extract in a headless browser session
//
// The headless browser is lazily initialized — Chrome/Chromium is only launched
// when the agent actually calls web_screenshot or web_browse.

use headless_chrome::{Browser, LaunchOptions, Tab};
use log::{info, warn};
use scraper::{Html, Selector};
use std::sync::{Arc, OnceLock};
use parking_lot::Mutex;
use crate::atoms::error::{EngineResult, EngineError};
use std::time::Duration;
use tauri::Manager;

// ── Shared browser instance (lazy singleton) ───────────────────────────

static BROWSER: OnceLock<Mutex<Option<Arc<Browser>>>> = OnceLock::new();

fn get_or_launch_browser(profile_dir: Option<std::path::PathBuf>) -> EngineResult<Arc<Browser>> {
    let mutex = BROWSER.get_or_init(|| Mutex::new(None));
    let mut guard = mutex.lock();

    // Return existing browser if alive
    if let Some(ref browser) = *guard {
        // Quick health check — try to get version
        if browser.get_version().is_ok() {
            return Ok(Arc::clone(browser));
        }
        warn!("[web] Browser process dead, relaunching");
    }

    info!("[web] Launching headless Chrome...");
    let mut builder = LaunchOptions::default_builder();
    builder
        .headless(true)
        .sandbox(false)  // Required in containers / CI
        .idle_browser_timeout(Duration::from_secs(300));

    // Use profile directory if configured
    if let Some(ref dir) = profile_dir {
        info!("[web] Using browser profile: {:?}", dir);
        builder.user_data_dir(Some(dir.clone()));
    }

    let browser = Browser::new(
        builder
            .build()
            .map_err(|e| EngineError::Other(e.to_string()))?,
    ).map_err(|e| format!(
        "Failed to launch Chrome/Chromium: {}. Make sure Chrome or Chromium is installed.",
        e
    ))?;

    let arc = Arc::new(browser);
    *guard = Some(Arc::clone(&arc));
    info!("[web] Headless Chrome launched successfully");
    Ok(arc)
}

// ── web_search: DuckDuckGo search ──────────────────────────────────────

pub async fn execute_web_search(args: &serde_json::Value) -> EngineResult<String> {
    let query = args["query"].as_str()
        .ok_or("web_search: missing 'query' argument")?;
    let limit = args["limit"].as_u64().unwrap_or(8) as usize;

    info!("[web] search: '{}' limit={}", query, limit);

    // Use DuckDuckGo HTML endpoint (no API key required)
    let encoded = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("q", query)
        .finish();
    let url = format!("https://html.duckduckgo.com/html/?{}", encoded);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()?;

    let resp = client.get(&url).send().await?;
    let html = resp.text().await?;

    // Parse DuckDuckGo HTML results
    let document = Html::parse_document(&html);
    let result_selector = Selector::parse(".result").unwrap();
    let title_selector = Selector::parse(".result__a").unwrap();
    let snippet_selector = Selector::parse(".result__snippet").unwrap();
    let url_selector = Selector::parse(".result__url").unwrap();

    let mut results = Vec::new();
    for element in document.select(&result_selector).take(limit) {
        let title = element.select(&title_selector).next()
            .map(|e| e.text().collect::<String>())
            .unwrap_or_default()
            .trim().to_string();

        let snippet = element.select(&snippet_selector).next()
            .map(|e| e.text().collect::<String>())
            .unwrap_or_default()
            .trim().to_string();

        let url = element.select(&url_selector).next()
            .map(|e| e.text().collect::<String>())
            .unwrap_or_default()
            .trim().to_string();

        if !title.is_empty() {
            results.push(format!("**{}**\n{}\n{}", title, url, snippet));
        }
    }

    if results.is_empty() {
        // Fallback: try parsing DuckDuckGo "zero click" or different layout
        let link_selector = Selector::parse("a.result__a").unwrap();
        for element in document.select(&link_selector).take(limit) {
            let title = element.text().collect::<String>().trim().to_string();
            let href = element.value().attr("href").unwrap_or("").to_string();
            if !title.is_empty() && !href.is_empty() {
                results.push(format!("**{}**\n{}", title, href));
            }
        }
    }

    if results.is_empty() {
        return Ok(format!("No search results found for '{}'.", query));
    }

    let mut output = format!("Search results for '{}':\n\n", query);
    for (i, r) in results.iter().enumerate() {
        output.push_str(&format!("{}. {}\n\n", i + 1, r));
    }
    Ok(output)
}

// ── web_read: Fetch URL → readable text ────────────────────────────────

pub async fn execute_web_read(args: &serde_json::Value) -> EngineResult<String> {
    let url = args["url"].as_str()
        .ok_or("web_read: missing 'url' argument")?;
    let selector = args["selector"].as_str();

    info!("[web] read: {} selector={:?}", url, selector);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?;

    let resp = client.get(url).send().await?;

    let status = resp.status().as_u16();
    let content_type = resp.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let body = resp.text().await?;

    // If non-HTML content, return raw (could be JSON, XML, plain text)
    if !content_type.contains("html") {
        let truncated = if body.len() > 50_000 {
            format!("{}...\n[truncated, {} total bytes]", &body[..50_000], body.len())
        } else {
            body
        };
        return Ok(format!("Content from {} (HTTP {}, {}):\n\n{}", url, status, content_type, truncated));
    }

    // Parse HTML and extract readable text
    let document = Html::parse_document(&body);

    let text = if let Some(sel_str) = selector {
        // User specified a CSS selector — extract just that part
        match Selector::parse(sel_str) {
            Ok(sel) => {
                document.select(&sel)
                    .map(|el| extract_text_from_element(&el))
                    .collect::<Vec<_>>()
                    .join("\n\n")
            }
            Err(_) => return Err(format!("Invalid CSS selector: {}", sel_str).into()),
        }
    } else {
        // Auto-extract: try <article>, <main>, then <body>
        extract_readable_text(&document)
    };

    if text.trim().is_empty() {
        return Ok(format!("Page at {} returned no readable text (HTTP {}). It may require JavaScript to render — try web_screenshot or web_browse instead.", url, status));
    }

    // Extract page title
    let title = Selector::parse("title").ok()
        .and_then(|sel| document.select(&sel).next())
        .map(|el| el.text().collect::<String>())
        .unwrap_or_default();

    // Truncate long content
    const MAX_TEXT: usize = 30_000;
    let truncated = if text.len() > MAX_TEXT {
        format!("{}...\n\n[Content truncated at {} chars, {} total]", &text[..MAX_TEXT], MAX_TEXT, text.len())
    } else {
        text
    };

    Ok(format!("# {}\nSource: {} (HTTP {})\n\n{}", title.trim(), url, status, truncated))
}

/// Extract readable text from an HTML element, skipping scripts/styles.
fn extract_text_from_element(element: &scraper::ElementRef) -> String {
    let mut text = String::new();
    for node in element.text() {
        let trimmed = node.trim();
        if !trimmed.is_empty() {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(trimmed);
        }
    }
    text
}

/// Extract readable content from a full HTML document.
/// Tries <article>, <main>, then falls back to <body>, skipping nav/footer/script/style.
fn extract_readable_text(document: &Html) -> String {
    // Try content-rich selectors first
    for sel_str in &["article", "main", "[role=main]", ".post-content", ".entry-content", ".article-body"] {
        if let Ok(sel) = Selector::parse(sel_str) {
            let parts: Vec<String> = document.select(&sel)
                .map(|el| extract_text_from_element(&el))
                .filter(|t| !t.trim().is_empty())
                .collect();
            if !parts.is_empty() {
                return parts.join("\n\n");
            }
        }
    }

    // Fallback: extract all text from <body>, filtering out noise elements
    if let Ok(body_sel) = Selector::parse("body") {
        if let Some(body) = document.select(&body_sel).next() {
            let mut paragraphs = Vec::new();

            // Get all <p>, <h1>-<h6>, <li>, <td>, <blockquote> elements
            for sel_str in &["p", "h1", "h2", "h3", "h4", "h5", "h6", "li", "blockquote", "pre", "td"] {
                if let Ok(sel) = Selector::parse(sel_str) {
                    for el in body.select(&sel) {
                        let text = extract_text_from_element(&el);
                        if text.len() > 20 {  // Skip very short fragments
                            paragraphs.push(text);
                        }
                    }
                }
            }

            if !paragraphs.is_empty() {
                // Deduplicate consecutive identical paragraphs
                paragraphs.dedup();
                return paragraphs.join("\n\n");
            }

            // Last resort: all text from body
            return extract_text_from_element(&body);
        }
    }

    String::new()
}

// ── Resolve browser profile dir from config ────────────────────────────

fn resolve_profile_dir(app_handle: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    use crate::engine::state::EngineState;
    let state = app_handle.try_state::<EngineState>()?;
    let json = state.store.get_config("browser_config").ok()??;
    let config: serde_json::Value = serde_json::from_str(&json).ok()?;
    let profile_id = config["default_profile"].as_str()?;
    if profile_id.is_empty() {
        return None;
    }
    let home = dirs::home_dir()?;
    let dir = home.join(".paw").join("browser-profiles").join(profile_id);
    if dir.exists() { Some(dir) } else { None }
}

// ── web_screenshot: Headless Chrome screenshot ─────────────────────────

pub async fn execute_web_screenshot(args: &serde_json::Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let url = args["url"].as_str()
        .ok_or("web_screenshot: missing 'url' argument")?;
    let full_page = args["full_page"].as_bool().unwrap_or(false);
    let width = args["width"].as_u64().unwrap_or(1280) as u32;
    let height = args["height"].as_u64().unwrap_or(800) as u32;

    info!("[web] screenshot: {} {}x{} full_page={}", url, width, height, full_page);

    let url_owned = url.to_string();
    let profile_dir = resolve_profile_dir(app_handle);

    // Browser ops are blocking — run in spawn_blocking
    let result = tokio::task::spawn_blocking(move || {
        let browser = get_or_launch_browser(profile_dir)?;

        let tab = browser.new_tab()
            .map_err(|e| EngineError::Other(e.to_string()))?;

        // Set viewport size
        tab.set_bounds(headless_chrome::types::Bounds::Normal {
            left: Some(0),
            top: Some(0),
            width: Some(width as f64),
            height: Some(height as f64),
        }).ok();

        // Navigate and wait for load
        tab.navigate_to(&url_owned)
            .map_err(|e| EngineError::Other(e.to_string()))?;

        tab.wait_until_navigated()
            .map_err(|e| EngineError::Other(e.to_string()))?;

        // Wait a bit for dynamic content
        std::thread::sleep(Duration::from_secs(2));

        // Capture screenshot
        let png_data = tab.capture_screenshot(
            headless_chrome::protocol::cdp::Page::CaptureScreenshotFormatOption::Png,
            None,  // quality
            None,  // clip
            true,  // from_surface
        ).map_err(|e| EngineError::Other(e.to_string()))?;

        // Get page title and URL for context
        let title = tab.get_title().unwrap_or_default();
        let final_url = tab.get_url();

        // Save to temp file
        let tmp_dir = std::env::temp_dir().join("paw-screenshots");
        std::fs::create_dir_all(&tmp_dir).ok();
        let filename = format!("screenshot-{}.png", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
        let filepath = tmp_dir.join(&filename);
        std::fs::write(&filepath, &png_data)?;

        // Also extract visible text for the agent to "see"
        let page_text = tab.get_content()
            .map(|html| {
                let doc = Html::parse_document(&html);
                let text = extract_readable_text(&doc);
                if text.len() > 5000 { format!("{}...", &text[..5000]) } else { text }
            })
            .unwrap_or_default();

        // Close tab
        let _ = tab.close(true);

        Ok(format!(
            "Screenshot saved: {}\nPage: {} ({})\nSize: {} bytes ({} x {})\n\nVisible text preview:\n{}",
            filepath.display(), title, final_url, png_data.len(), width, height, page_text
        ))
    }).await.map_err(|e| EngineError::Other(e.to_string()))?;

    result
}

// ── web_browse: Interactive headless browser session ───────────────────

pub async fn execute_web_browse(args: &serde_json::Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let action = args["action"].as_str()
        .ok_or("web_browse: missing 'action' argument")?;
    let url = args["url"].as_str().map(|s| s.to_string());
    let selector = args["selector"].as_str().map(|s| s.to_string());
    let text = args["text"].as_str().map(|s| s.to_string());
    let js = args["javascript"].as_str().map(|s| s.to_string());

    info!("[web] browse: action={} url={:?} selector={:?}", action, url, selector);

    let action_owned = action.to_string();
    let profile_dir = resolve_profile_dir(app_handle);

    let result = tokio::task::spawn_blocking(move || {
        let browser = get_or_launch_browser(profile_dir)?;

        // Get or create the working tab (reuse first tab for session continuity)
        let tab: Arc<Tab> = {
            let tabs = browser.get_tabs().lock()
                .map_err(|e| EngineError::Other(e.to_string()))?;
            if tabs.is_empty() {
                drop(tabs);
                browser.new_tab()
                    .map_err(|e| EngineError::Other(e.to_string()))?
            } else {
                let t = Arc::clone(&tabs[0]);
                drop(tabs);
                t
            }
        };

        match action_owned.as_str() {
            "navigate" | "goto" => {
                let target_url = url.ok_or("web_browse: 'url' is required for navigate action")?;
                tab.navigate_to(&target_url)
                    .map_err(|e| EngineError::Other(e.to_string()))?;
                tab.wait_until_navigated()
                    .map_err(|e| EngineError::Other(e.to_string()))?;
                std::thread::sleep(Duration::from_millis(1500));

                let title = tab.get_title().unwrap_or_default();
                let final_url = tab.get_url();
                let page_text = get_tab_text(&tab, 3000);

                Ok(format!("Navigated to: {} ({})\n\nPage content:\n{}", title, final_url, page_text))
            }

            "click" => {
                let sel = selector.ok_or("web_browse: 'selector' is required for click action")?;
                tab.find_element(&sel)
                    .map_err(|e| format!("Element not found '{}': {}", sel, e))?
                    .click()
                    .map_err(|e| EngineError::Other(e.to_string()))?;

                std::thread::sleep(Duration::from_millis(1500));
                let title = tab.get_title().unwrap_or_default();
                let page_text = get_tab_text(&tab, 3000);

                Ok(format!("Clicked '{}'. Current page: {}\n\n{}", sel, title, page_text))
            }

            "type" | "fill" => {
                let sel = selector.ok_or("web_browse: 'selector' is required for type action")?;
                let input_text = text.ok_or("web_browse: 'text' is required for type action")?;

                tab.find_element(&sel)
                    .map_err(|e| format!("Element not found '{}': {}", sel, e))?
                    .click()
                    .map_err(|e| EngineError::Other(e.to_string()))?;
                tab.type_str(&input_text)
                    .map_err(|e| EngineError::Other(e.to_string()))?;

                Ok(format!("Typed '{}' into '{}'", input_text, sel))
            }

            "press" => {
                let key = text.ok_or("web_browse: 'text' (key name) is required for press action")?;
                tab.press_key(&key)
                    .map_err(|e| EngineError::Other(e.to_string()))?;
                std::thread::sleep(Duration::from_millis(1000));
                let page_text = get_tab_text(&tab, 3000);
                Ok(format!("Pressed key '{}'\n\n{}", key, page_text))
            }

            "extract" | "read" => {
                let sel = selector.as_deref().unwrap_or("body");
                let elements = tab.find_elements(sel)
                    .map_err(|e| format!("Selector '{}' not found: {}", sel, e))?;

                let mut output = String::new();
                for (i, el) in elements.iter().take(20).enumerate() {
                    if let Ok(text) = el.get_inner_text() {
                        if !text.trim().is_empty() {
                            output.push_str(&format!("{}. {}\n", i + 1, text.trim()));
                        }
                    }
                }

                if output.is_empty() {
                    Ok(format!("No text content found for selector '{}'", sel))
                } else {
                    let truncated = if output.len() > 10_000 {
                        format!("{}...\n[truncated]", &output[..10_000])
                    } else {
                        output
                    };
                    Ok(format!("Extracted from '{}':\n\n{}", sel, truncated))
                }
            }

            "javascript" | "eval" | "js" => {
                let script = js.or(text).ok_or("web_browse: 'javascript' is required for js action")?;

                let result = tab.evaluate(&script, false)
                    .map_err(|e| EngineError::Other(e.to_string()))?;

                let value = match result.value {
                    Some(v) => serde_json::to_string_pretty(&v).unwrap_or_else(|_| format!("{:?}", v)),
                    None => "undefined".into(),
                };

                Ok(format!("JavaScript result:\n{}", value))
            }

            "scroll" => {
                let direction = text.as_deref().unwrap_or("down");
                let pixels = match direction {
                    "up" => -500,
                    "top" => -99999,
                    "bottom" => 99999,
                    _ => 500,  // down
                };
                tab.evaluate(&format!("window.scrollBy(0, {})", pixels), false)
                    .map_err(|e| EngineError::Other(e.to_string()))?;
                std::thread::sleep(Duration::from_millis(500));
                let page_text = get_tab_text(&tab, 3000);
                Ok(format!("Scrolled {}. Visible content:\n\n{}", direction, page_text))
            }

            "links" => {
                let result = tab.evaluate(
                    "JSON.stringify(Array.from(document.querySelectorAll('a[href]')).slice(0, 50).map(a => ({text: a.innerText.trim().slice(0, 100), href: a.href})).filter(a => a.text && a.href)))",
                    false
                ).map_err(|e| EngineError::Other(e.to_string()))?;

                let value_str = result.value
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .unwrap_or("[]".into());

                let links: Vec<serde_json::Value> = serde_json::from_str(&value_str).unwrap_or_default();
                let mut output = format!("Links on page ({}):\n\n", links.len());
                for (i, link) in links.iter().enumerate() {
                    let text = link["text"].as_str().unwrap_or("?");
                    let href = link["href"].as_str().unwrap_or("?");
                    output.push_str(&format!("{}. [{}]({})\n", i + 1, text, href));
                }
                Ok(output)
            }

            "info" => {
                let title = tab.get_title().unwrap_or_default();
                let url = tab.get_url();
                Ok(format!("Current page: {} ({})", title, url))
            }

            _ => Err(format!(
                "Unknown browse action '{}'. Available: navigate, click, type, press, extract, javascript, scroll, links, info",
                action_owned
            ).into()),
        }
    }).await.map_err(|e| EngineError::Other(e.to_string()))?;

    result
}

/// Extract visible text from the current tab page.
fn get_tab_text(tab: &Tab, max_chars: usize) -> String {
    tab.get_content()
        .map(|html| {
            let doc = Html::parse_document(&html);
            let text = extract_readable_text(&doc);
            if text.len() > max_chars {
                format!("{}...", &text[..max_chars])
            } else {
                text
            }
        })
        .unwrap_or_else(|_| "(could not read page content)".into())
}
