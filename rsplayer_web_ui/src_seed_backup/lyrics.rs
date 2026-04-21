use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct LrcLibResponse {
    pub plain_lyrics: Option<String>,
    pub synced_lyrics: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LyricLine {
    pub time_secs: f64,
    pub text: String,
}

pub fn parse_lrc(lrc: &str) -> Vec<LyricLine> {
    let mut lines = Vec::new();
    for line in lrc.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // LRCLIB sometimes has empty bracket lines for spacing [00:10.00]
        if let Some(end_bracket) = line.find(']') {
            if let Some(start_bracket) = line.find('[') {
                let time_str = &line[start_bracket + 1..end_bracket];
                let text = line[end_bracket + 1..].trim().to_string();
                if let Some(time) = parse_time(time_str) {
                    lines.push(LyricLine {
                        time_secs: time,
                        text,
                    });
                }
            }
        }
    }
    // Filter out empty lines that only contain timestamps if they are just for spacing, 
    // but often they are important for highlighting.
    lines.sort_by(|a, b| a.time_secs.partial_cmp(&b.time_secs).unwrap_or(std::cmp::Ordering::Equal));
    lines
}

fn parse_time(time_str: &str) -> Option<f64> {
    // mm:ss.xx or mm:ss:xx or hh:mm:ss.xx
    let parts: Vec<&str> = time_str.split(':').collect();
    if parts.len() == 2 {
        let mm = parts[0].parse::<f64>().ok()?;
        let ss = parts[1].replace(',', ".").parse::<f64>().ok()?;
        Some(mm * 60.0 + ss)
    } else if parts.len() == 3 {
        let hh = parts[0].parse::<f64>().ok()?;
        let mm = parts[1].parse::<f64>().ok()?;
        let ss = parts[2].replace(',', ".").parse::<f64>().ok()?;
        Some(hh * 3600.0 + mm * 60.0 + ss)
    } else {
        None
    }
}
