trait Queue {
    fn queue(song_id: &str);
    fn enqueue();
    fn pop();
    fn insert_after(after_song_id: &str, song_id: &str);
    fn insert_before(before_song_id: &str, song_id: &str);
    fn clear();
}