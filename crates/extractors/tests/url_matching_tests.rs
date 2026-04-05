use yt_dlp_extractors::*;

fn assert_extractor_matches(registry: &ExtractorRegistry, urls: &[&str], expected_key: &str) {
    for url in urls {
        let ext = registry.find_extractor(url);
        assert!(
            ext.is_some(),
            "no extractor found for {url}, expected {expected_key}"
        );
        assert_eq!(
            ext.unwrap().key(),
            expected_key,
            "wrong extractor for {url}: got {}, expected {expected_key}",
            ext.unwrap().key()
        );
    }
}

#[test]
fn test_youtube_urls() {
    let registry = create_default_registry();
    let urls = [
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
        "https://youtu.be/dQw4w9WgXcQ",
        "https://www.youtube.com/embed/dQw4w9WgXcQ",
        "https://m.youtube.com/watch?v=dQw4w9WgXcQ",
        "https://www.youtube.com/shorts/abc12345678",
        "https://www.youtube.com/live/abc12345678",
    ];
    assert_extractor_matches(&registry, &urls, "Youtube");
}

#[test]
fn test_twitter_urls() {
    let registry = create_default_registry();
    let urls = [
        "https://twitter.com/user/status/1234567890",
        "https://x.com/user/status/1234567890",
        "https://www.twitter.com/user/status/9876543210",
    ];
    assert_extractor_matches(&registry, &urls, "Twitter");
}

#[test]
fn test_reddit_urls() {
    let registry = create_default_registry();
    let urls = [
        "https://www.reddit.com/r/videos/comments/abc123",
        "https://old.reddit.com/r/funny/comments/def456",
        "https://redd.it/abc123",
        "https://v.redd.it/abc123",
    ];
    assert_extractor_matches(&registry, &urls, "Reddit");
}

#[test]
fn test_vimeo_urls() {
    let registry = create_default_registry();
    let urls = [
        "https://vimeo.com/123456789",
        "https://www.vimeo.com/987654321",
        "https://player.vimeo.com/video/123456789",
    ];
    assert_extractor_matches(&registry, &urls, "Vimeo");
}

#[test]
fn test_tiktok_urls() {
    let registry = create_default_registry();
    let urls = [
        "https://www.tiktok.com/@user/video/1234567890123456789",
        "https://vm.tiktok.com/ZMRxyz123",
    ];
    assert_extractor_matches(&registry, &urls, "TikTok");
}

#[test]
fn test_instagram_urls() {
    let registry = create_default_registry();
    let urls = [
        "https://www.instagram.com/p/CxyzABC1234",
        "https://www.instagram.com/reel/CxyzABC1234",
        "https://instagram.com/tv/CxyzABC1234",
    ];
    assert_extractor_matches(&registry, &urls, "Instagram");
}

#[test]
fn test_twitch_urls() {
    let registry = create_default_registry();
    let urls = [
        "https://www.twitch.tv/videos/1234567890",
        "https://clips.twitch.tv/FunnyClipName-Abc123xyz",
    ];
    assert_extractor_matches(&registry, &urls, "Twitch");
}

#[test]
fn test_dailymotion_urls() {
    let registry = create_default_registry();
    let urls = [
        "https://www.dailymotion.com/video/x8abc12",
        "https://dai.ly/x8abc12",
    ];
    assert_extractor_matches(&registry, &urls, "Dailymotion");
}

#[test]
fn test_soundcloud_urls() {
    let registry = create_default_registry();
    let urls = [
        "https://soundcloud.com/artist/track-name",
        "https://www.soundcloud.com/artist/sets/album-name",
    ];
    assert_extractor_matches(&registry, &urls, "SoundCloud");
}

#[test]
fn test_bilibili_urls() {
    let registry = create_default_registry();
    let urls = [
        "https://www.bilibili.com/video/BV1xx411c7mD",
        "https://b23.tv/BV1xx411c7mD",
    ];
    assert_extractor_matches(&registry, &urls, "Bilibili");
}

#[test]
fn test_facebook_urls() {
    let registry = create_default_registry();
    let urls = [
        "https://www.facebook.com/user/videos/1234567890",
        "https://www.facebook.com/watch/?v=1234567890",
        "https://www.facebook.com/reel/1234567890",
    ];
    assert_extractor_matches(&registry, &urls, "Facebook");
}

#[test]
fn test_generic_catches_unknown_urls() {
    let registry = create_default_registry();
    let url = "https://www.example.com/some/unknown/video.mp4";
    let ext = registry.find_extractor(url);
    assert!(ext.is_some(), "generic extractor should catch unknown URLs");
    assert_eq!(ext.unwrap().key(), "Generic");
}

#[test]
fn test_extractor_priority_specific_before_generic() {
    let registry = create_default_registry();
    // A YouTube URL should be matched by the YouTube extractor, not the generic one
    let url = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";
    let ext = registry.find_extractor(url).unwrap();
    assert_eq!(ext.key(), "Youtube");
    assert_ne!(ext.key(), "Generic");
}
