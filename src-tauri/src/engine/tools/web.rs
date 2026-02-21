// Paw Agent Engine — Web tools
// web_search, web_read, web_screenshot, web_browse
// Execution delegates to crate::engine::web

use crate::atoms::types::*;
use crate::atoms::error::EngineResult;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "web_search".into(),
                description: "Search the internet using DuckDuckGo. Returns titles, URLs, and snippets. No API key needed.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query" },
                        "limit": { "type": "integer", "description": "Max results to return (default: 8)" }
                    },
                    "required": ["query"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "web_read".into(),
                description: "Fetch a web page and extract its readable text content (strips HTML). Much better than raw fetch for reading articles, docs, and web pages. Optionally pass a CSS selector to extract specific elements.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "URL to read" },
                        "selector": { "type": "string", "description": "Optional CSS selector to extract specific content (e.g. '.article-body', '#main')" }
                    },
                    "required": ["url"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "web_screenshot".into(),
                description: "Take a screenshot of any web page using a headless browser. Returns the file path and visible text. Works with JavaScript-heavy pages, SPAs, and dynamic content that web_read can't handle.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "URL to screenshot" },
                        "full_page": { "type": "boolean", "description": "Capture the full scrollable page (default: false, viewport only)" },
                        "width": { "type": "integer", "description": "Viewport width in pixels (default: 1280)" },
                        "height": { "type": "integer", "description": "Viewport height in pixels (default: 800)" }
                    },
                    "required": ["url"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "web_browse".into(),
                description: "Control a headless browser — navigate pages, click buttons, fill forms, run JavaScript, extract data. The browser session persists between calls so you can interact with websites step by step. Actions: navigate, click, type, press, extract, javascript, scroll, links, info.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["navigate", "click", "type", "press", "extract", "javascript", "scroll", "links", "info"],
                            "description": "What to do: navigate (go to URL), click (CSS selector), type (text into selector), press (key like Enter/Tab), extract (get text from selector), javascript (run JS), scroll (up/down/top/bottom), links (list all links), info (current page)"
                        },
                        "url": { "type": "string", "description": "URL for navigate action" },
                        "selector": { "type": "string", "description": "CSS selector for click/type/extract actions" },
                        "text": { "type": "string", "description": "Text to type, key to press, or scroll direction" },
                        "javascript": { "type": "string", "description": "JavaScript code for javascript action" }
                    },
                    "required": ["action"]
                }),
            },
        },
    ]
}

pub async fn execute(
    name: &str,
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> Option<Result<String, String>> {
    match name {
        "web_search"     => Some(crate::engine::web::execute_web_search(args).await.map_err(|e| e.to_string())),
        "web_read"       => Some(crate::engine::web::execute_web_read(args).await.map_err(|e| e.to_string())),
        "web_screenshot" => Some(crate::engine::web::execute_web_screenshot(args, app_handle).await.map_err(|e| e.to_string())),
        "web_browse"     => Some(crate::engine::web::execute_web_browse(args, app_handle).await.map_err(|e| e.to_string())),
        _ => None,
    }
}
