//! Shared "agent chat" scenario builders — the mock AI agent used by the
//! `08_agent_chat` example across every UI backend.
//!
//! Each scenario returns a `Vec<serde_json::Value>` of A2UI protocol messages
//! (a simulated `text/a2ui` SSE stream: `createSurface` → `updateComponents` →
//! `updateDataModel`). They are pure, framework-agnostic JSON builders, so the
//! TUI, Slint, egui, Bevy, Iced, and Dioxus `08_agent_chat` examples all share
//! this one source instead of each carrying their own copy of the scenarios.
//!
//! The catalog id points at the embedded basic catalog — the same one
//! [`crate::catalogs::basic::build_basic_catalog`] registers — so every backend
//! only needs to seed its `MessageProcessor` with that catalog and feed these
//! messages in.

use serde_json::{Value, json};

/// The basic catalog id, the same string every `08_agent_chat` example feeds to
/// `createSurface`. Centralized so the scenarios and the host always agree.
pub const CATALOG_ID: &str = "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json";

/// The welcome surface shown when the chat first opens (before the user types
/// anything). Shared so every backend renders the identical greeting.
pub fn welcome_messages(sid: &str) -> Vec<Value> {
    vec![
        json!({"version":"v1.0","createSurface":{"surfaceId":sid,"catalogId":CATALOG_ID,"dataModel":{}}}),
        json!({"version":"v1.0","updateComponents":{"surfaceId":sid,"components":[
            {"id":"root","component":"Column","children":["title","sub","d1","hint"],"align":"stretch"},
            {"id":"title","component":"Text","text":"🤖 Welcome to A2UI Agent Chat!","variant":"h1"},
            {"id":"sub","component":"Text","text":"This is a terminal AI chat interface powered by the A2UI protocol.","variant":"body"},
            {"id":"d1","component":"Divider","axis":"horizontal"},
            {"id":"hint","component":"Text","text":"Type a message below to get started. Try: hello, weather, tasks, story, stats, quote","variant":"caption"}
        ]}}),
    ]
}

/// Pick a scenario from the user's message (case-insensitive substring match)
/// and return its A2UI message stream for surface `sid`.
pub fn generate_response(sid: &str, user_msg: &str) -> Vec<Value> {
    let lower = user_msg.to_lowercase();
    if lower.contains("hello") || lower.contains("hi") || lower == "hey" {
        scenario_greeting(sid)
    } else if lower.contains("weather") {
        scenario_weather(sid)
    } else if lower.contains("task") {
        scenario_tasks(sid)
    } else if lower.contains("story") || lower.contains("tell me") {
        scenario_streaming(sid)
    } else if lower.contains("stat") || lower.contains("dashboard") || lower.contains("number") {
        scenario_stats(sid)
    } else if lower.contains("quote") {
        scenario_quote(sid)
    } else if lower.contains("help") || lower.contains("command") {
        scenario_help(sid)
    } else {
        scenario_default(sid)
    }
}

fn scenario_greeting(sid: &str) -> Vec<Value> {
    vec![
        json!({"version":"v1.0","createSurface":{"surfaceId":sid,"catalogId":CATALOG_ID,"dataModel":{"text":""}}}),
        json!({"version":"v1.0","updateComponents":{"surfaceId":sid,"components":[
            {"id":"root","component":"Column","children":["greeting","sub","divider","hint"],"align":"stretch"},
            {"id":"greeting","component":"Text","text":"Hello there! 👋","variant":"h2"},
            {"id":"sub","component":"Text","text":"I'm your A2UI Agent. I can show you rich UI components streamed via the A2UI protocol!","variant":"body"},
            {"id":"divider","component":"Divider","axis":"horizontal"},
            {"id":"hint","component":"Text","text":"Try: weather, tasks, story, or help","variant":"caption"}
        ]}}),
    ]
}

fn scenario_weather(sid: &str) -> Vec<Value> {
    vec![
        json!({"version":"v1.0","createSurface":{"surfaceId":sid,"catalogId":CATALOG_ID,"dataModel":{}}}),
        json!({"version":"v1.0","updateComponents":{"surfaceId":sid,"components":[
            {"id":"root","component":"Column","children":["intro","card"],"align":"stretch"},
            {"id":"intro","component":"Text","text":"Here's the weather forecast:","variant":"body"},
            {"id":"card","component":"Card","child":"card_inner","weight":8},
            {"id":"card_inner","component":"Column","children":["city","temp","cond","hum","wind","d1","foot"],"align":"stretch"},
            {"id":"city","component":"Text","text":"📍 San Francisco, CA","variant":"h3"},
            {"id":"temp","component":"Text","text":"🌡️  Temperature: 72°F (22°C)","variant":"body"},
            {"id":"cond","component":"Text","text":"🌤️  Condition: Partly Cloudy","variant":"body"},
            {"id":"hum","component":"Text","text":"💧 Humidity: 65%","variant":"body"},
            {"id":"wind","component":"Text","text":"💨 Wind: 12 mph NW","variant":"body"},
            {"id":"d1","component":"Divider","axis":"horizontal"},
            {"id":"foot","component":"Text","text":"📅 7-Day forecast available | 🔄 Updated 2:30 PM","variant":"caption"}
        ]}}),
    ]
}

fn scenario_tasks(sid: &str) -> Vec<Value> {
    let mut messages = vec![
        json!({"version":"v1.0","createSurface":{"surfaceId":sid,"catalogId":CATALOG_ID,"dataModel":{
            "progress_bar": "░░░░░░░░░░░░░░░░░░░░ 0%",
            "status": "⏳ Scanning project...",
            "task_text": "",
            "summary_text": "loading..."
        }}}),
        json!({"version":"v1.0","updateComponents":{"surfaceId":sid,"components":[
            {"id":"root","component":"Column","children":["title","progress_card","d1","task_card","d2","footer"],"align":"stretch"},
            {"id":"title","component":"Text","text":"🚀 Sprint Board — a2ui v0.2.0","variant":"h1"},
            {"id":"progress_card","component":"Card","child":"progress_inner","weight":3},
            {"id":"progress_inner","component":"Column","children":["bar","status"],"align":"stretch"},
            {"id":"bar","component":"Text","text":{"path":"/progress_bar"},"variant":"h3"},
            {"id":"status","component":"Text","text":{"path":"/status"},"variant":"caption"},
            {"id":"d1","component":"Divider","axis":"horizontal"},
            {"id":"task_card","component":"Card","child":"task_inner","weight":10},
            {"id":"task_inner","component":"Column","children":["task_header","task_text"],"align":"stretch"},
            {"id":"task_header","component":"Text","text":"📝 Tasks","variant":"h3"},
            {"id":"task_text","component":"Text","text":{"path":"/task_text"},"variant":"body"},
            {"id":"d2","component":"Divider","axis":"horizontal"},
            {"id":"footer","component":"Text","text":{"path":"/summary_text"},"variant":"caption"}
        ]}}),
    ];

    let tasks = [
        ("🔴 P0", "✅", "Fix layout engine justify bug"),
        ("🔴 P0", "✅", "Implement focus management"),
        ("🟡 P1", "✅", "Add Card component shadow"),
        ("🟡 P1", "⬜", "SSE transport layer"),
        ("🟢 P2", "⬜", "WebSocket bidirectional support"),
        ("🟢 P2", "⬜", "Agent chat streaming demo"),
        ("🔵 P3", "⬜", "Integration test suite"),
        ("🔵 P3", "⬜", "CSS theme engine"),
    ];

    let total = tasks.len();
    let mut completed = 0usize;

    messages.push(json!({"version":"v1.0","updateDataModel":{"surfaceId":sid,"path":"/status","value":"⏳ Scanning 24 files..."}}));

    for (i, (_priority, status, _name)) in tasks.iter().enumerate() {
        if *status == "✅" {
            completed += 1;
        }

        let pct = (i + 1) * 100 / total;
        let filled = pct / 5;
        let empty = 20 - filled;
        let bar: String = "█".repeat(filled) + &"░".repeat(empty);

        let lines: Vec<String> = tasks[..=i]
            .iter()
            .map(|(pri, st, n)| {
                let check = if *st == "✅" { "✅" } else { "⬜" };
                format!("  {} {} {}", check, pri, n)
            })
            .collect();

        let stat = if i < total - 1 {
            format!("⏳ Processing task {}/{}", i + 1, total)
        } else {
            "✅ All tasks loaded!".to_string()
        };

        let summary = format!(
            "{} done · {} remaining · {}% complete",
            completed,
            total - completed,
            completed * 100 / total
        );

        messages.push(json!({
            "version":"v1.0",
            "updateDataModel":{"surfaceId":sid,"path":"/","value":{
                "progress_bar": format!("{} {}%", bar, pct),
                "status": stat,
                "task_text": lines.join("\n"),
                "summary_text": summary
            }}
        }));
    }

    messages
}

fn scenario_streaming(sid: &str) -> Vec<Value> {
    let story = "Once upon a time, in a digital realm far away, there lived a protocol \
        called A2UI. It could transform plain JSON messages into beautiful user interfaces, \
        streaming them in real-time across the wire. Developers marveled at its simplicity \
        — no build steps, no bundlers, just pure structured data flowing from agent to screen. 🌟";

    let words: Vec<&str> = story.split(' ').collect();
    let mut messages = vec![
        json!({"version":"v1.0","createSurface":{"surfaceId":sid,"catalogId":CATALOG_ID,"dataModel":{"story":""}}}),
        json!({"version":"v1.0","updateComponents":{"surfaceId":sid,"components":[
            {"id":"root","component":"Column","children":["label","story_text"],"align":"stretch"},
            {"id":"label","component":"Text","text":"📖 A Story (streaming word-by-word via updateDataModel)","variant":"h3"},
            {"id":"story_text","component":"Text","text":{"path":"/story"},"variant":"body"}
        ]}}),
    ];

    let mut accumulated = String::new();
    for word in words {
        if !accumulated.is_empty() {
            accumulated.push(' ');
        }
        accumulated.push_str(word);
        messages.push(json!({
            "version":"v1.0",
            "updateDataModel":{"surfaceId":sid,"path":"/story","value": accumulated}
        }));
    }
    messages
}

fn scenario_help(sid: &str) -> Vec<Value> {
    vec![
        json!({"version":"v1.0","createSurface":{"surfaceId":sid,"catalogId":CATALOG_ID,"dataModel":{}}}),
        json!({"version":"v1.0","updateComponents":{"surfaceId":sid,"components":[
            {"id":"root","component":"Column","children":["title","d0","c1","c2","c3","c4","c5","c6","c7","d1","hint"],"align":"stretch"},
            {"id":"title","component":"Text","text":"📖 Available Commands","variant":"h2"},
            {"id":"d0","component":"Divider","axis":"horizontal"},
            {"id":"c1","component":"Text","text":"  hello   → Simple greeting response","variant":"body"},
            {"id":"c2","component":"Text","text":"  weather → Weather card with rich components","variant":"body"},
            {"id":"c3","component":"Text","text":"  tasks   → Interactive task list in a Card","variant":"body"},
            {"id":"c4","component":"Text","text":"  story   → Streaming text word-by-word","variant":"body"},
            {"id":"c5","component":"Text","text":"  stats   → Dashboard: a Row of stat Cards","variant":"body"},
            {"id":"c6","component":"Text","text":"  quote   → A pull-quote Card","variant":"body"},
            {"id":"c7","component":"Text","text":"  help    → Show this command list","variant":"body"},
            {"id":"d1","component":"Divider","axis":"horizontal"},
            {"id":"hint","component":"Text","text":"Each response is streamed as A2UI protocol messages (text/a2ui over SSE)","variant":"caption"}
        ]}}),
    ]
}

fn scenario_stats(sid: &str) -> Vec<Value> {
    // A horizontal dashboard: a Row of three Cards, each a stat tile. Showcases
    // the Row (horizontal layout) + nested Card > Column trees side by side.
    vec![
        json!({"version":"v1.0","createSurface":{"surfaceId":sid,"catalogId":CATALOG_ID,"dataModel":{}}}),
        json!({"version":"v1.0","updateComponents":{"surfaceId":sid,"components":[
            {"id":"root","component":"Column","children":["title","stats_row","d1","note"],"align":"stretch"},
            {"id":"title","component":"Text","text":"📊 a2ui by the Numbers","variant":"h2"},
            {"id":"stats_row","component":"Row","children":["c_down","c_stars","c_back"],"align":"stretch"},
            {"id":"c_down","component":"Card","child":"c_down_inner","weight":1},
            {"id":"c_down_inner","component":"Column","children":["down_num","down_lbl"],"align":"stretch"},
            {"id":"down_num","component":"Text","text":"1.2k","variant":"h1"},
            {"id":"down_lbl","component":"Text","text":"downloads","variant":"caption"},
            {"id":"c_stars","component":"Card","child":"c_stars_inner","weight":1},
            {"id":"c_stars_inner","component":"Column","children":["stars_num","stars_lbl"],"align":"stretch"},
            {"id":"stars_num","component":"Text","text":"4.9★","variant":"h1"},
            {"id":"stars_lbl","component":"Text","text":"avg rating","variant":"caption"},
            {"id":"c_back","component":"Card","child":"c_back_inner","weight":1},
            {"id":"c_back_inner","component":"Column","children":["back_num","back_lbl"],"align":"stretch"},
            {"id":"back_num","component":"Text","text":"6","variant":"h1"},
            {"id":"back_lbl","component":"Text","text":"backends","variant":"caption"},
            {"id":"d1","component":"Divider","axis":"horizontal"},
            {"id":"note","component":"Text","text":"ratatui · slint · egui · bevy · iced · dioxus","variant":"caption"}
        ]}}),
    ]
}

fn scenario_quote(sid: &str) -> Vec<Value> {
    // A single tall Card holding a pull-quote + attribution — a content-sized
    // surface that is intentionally a few rows taller than a greeting, so
    // scrolling the chat reveals it cleanly in slices.
    vec![
        json!({"version":"v1.0","createSurface":{"surfaceId":sid,"catalogId":CATALOG_ID,"dataModel":{}}}),
        json!({"version":"v1.0","updateComponents":{"surfaceId":sid,"components":[
            {"id":"root","component":"Column","children":["qcard"],"align":"stretch"},
            {"id":"qcard","component":"Card","child":"qcard_inner","weight":8},
            {"id":"qcard_inner","component":"Column","children":["qmark","body","d1","who"],"align":"stretch"},
            {"id":"qmark","component":"Text","text":"❝","variant":"h1"},
            {"id":"body","component":"Text","text":"Any sufficiently advanced interface is indistinguishable from a well-structured JSON stream.","variant":"h3"},
            {"id":"d1","component":"Divider","axis":"horizontal"},
            {"id":"who","component":"Text","text":"— an a2ui proverb","variant":"caption"}
        ]}}),
    ]
}

fn scenario_default(sid: &str) -> Vec<Value> {
    vec![
        json!({"version":"v1.0","createSurface":{"surfaceId":sid,"catalogId":CATALOG_ID,"dataModel":{}}}),
        json!({"version":"v1.0","updateComponents":{"surfaceId":sid,"components":[
            {"id":"root","component":"Column","children":["msg","d1","card"],"align":"stretch"},
            {"id":"msg","component":"Text","text":"I received your message! Here are some things you can try:","variant":"body"},
            {"id":"d1","component":"Divider","axis":"horizontal"},
            {"id":"card","component":"Card","child":"card_inner","weight":6},
            {"id":"card_inner","component":"Column","children":["s1","s2","s3","s4","s5","s6"],"align":"stretch"},
            {"id":"s1","component":"Text","text":"💬  Say \"hello\" for a greeting","variant":"body"},
            {"id":"s2","component":"Text","text":"🌤️  Say \"weather\" for a weather card","variant":"body"},
            {"id":"s3","component":"Text","text":"📋  Say \"tasks\" for a task list","variant":"body"},
            {"id":"s4","component":"Text","text":"📖  Say \"story\" for streaming text","variant":"body"},
            {"id":"s5","component":"Text","text":"📊  Say \"stats\" for a dashboard","variant":"body"},
            {"id":"s6","component":"Text","text":"❝  Say \"quote\" for a pull-quote card","variant":"body"}
        ]}}),
    ]
}
