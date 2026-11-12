#[test]
fn test_config_loads_or_defaults() {
    // When BOT_TOKEN is set, Config::from_env should succeed
    std::env::set_var("BOT_TOKEN", "123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11");
    let cfg = sticker_converter_bot::config::Config::from_env();
    assert!(cfg.is_ok(), "Config should load successfully when BOT_TOKEN is present");
    let c = cfg.unwrap();
    assert_eq!(c.bot_token, "123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11");
    assert!(c.ffmpeg_path.exists() || !c.ffmpeg_path.as_os_str().is_empty());
}

