use regex::Regex;
use serde_json::{Map, Value};
use std::env;
use std::error::Error;
use std::process;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn collect_video_renderers(value: &Value, results: &mut Vec<Value>) {
    match value {
        Value::Object(obj) => {
            if let Some(vr) = obj.get("videoRenderer") {
                let video_id = vr
                    .get("videoId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let title = vr
                    .pointer("/title/runs/0/text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let author = vr
                    .pointer("/longBylineText/runs/0/text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let duration = vr
                    .pointer("/lengthText/simpleText")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let views = vr
                    .pointer("/viewCountText/simpleText")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let mut map = Map::new();
                map.insert("title".into(), Value::String(title));
                map.insert("videoId".into(), Value::String(video_id.clone()));
                map.insert(
                    "url".into(),
                    Value::String(format!("https://www.youtube.com/watch?v={}", video_id)),
                );
                map.insert("author".into(), Value::String(author));
                map.insert("duration".into(), Value::String(duration));
                map.insert("views".into(), Value::String(views));

                results.push(Value::Object(map));
            }

            for (_, v) in obj {
                collect_video_renderers(v, results);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                collect_video_renderers(v, results);
            }
        }
        _ => {}
    }
}

fn fetch_search_results(search_query: &str, limit: usize) -> Result<Vec<Value>> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
             AppleWebKit/537.36 (KHTML, like Gecko) \
             Chrome/120.0.0.0 Safari/537.36",
        )
        .build()?;

    let resp = client
        .get("https://www.youtube.com/results")
        .query(&[
            ("search_query", search_query),
            ("sp", "EgIQAQ%3D%3D"),
            ("hl", "en"),
            ("gl", "US"),
        ])
        .send()?;

    let html_content = resp.text()?;

    let re = Regex::new(r"(?s)var ytInitialData = (.*?);</script>")?;

    let json_str = re
        .captures(&html_content)
        .and_then(|c| c.get(1).map(|m| m.as_str()))
        .ok_or("Failed to extract JSON data (YouTube structure might have changed)")?;

    let json_data: Value = serde_json::from_str(json_str)
        .map_err(|_| -> Box<dyn Error> { "Extracted data is not valid JSON".into() })?;

    let mut results: Vec<Value> = Vec::new();
    collect_video_renderers(&json_data, &mut results);
    results.truncate(limit);
    println!("{:?}", results);
    Ok(results)
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} \"<search query>\" [limit]", args[0]);
        eprintln!("Example: {} \"arch linux\" 5", args[0]);
        process::exit(1);
    }

    let search_query = &args[1];
    let limit: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10);

    match fetch_search_results(search_query, limit) {
        Ok(results) => {
            let output =
                serde_json::to_string_pretty(&results).unwrap_or_else(|_| "[]".to_string());
            println!("{}", output);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    }
}
