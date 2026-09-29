pub fn ellipsize(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    let take = max_chars.saturating_sub(2);
    let mut out: String = s.chars().take(take).collect();
    out.push_str("..");
    out
}

pub fn format_time(secs: u32) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}
