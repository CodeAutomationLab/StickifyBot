use std::path::PathBuf;
use sticker_converter_bot::rlottie_bridge::RLottieAnimation;

fn get_dll_path() -> PathBuf {
    let p = PathBuf::from("bin/rlottie.dll");
    if p.exists() {
        return p;
    }
    PathBuf::from("rust_bot/bin/rlottie.dll")
}

#[test]
fn test_rlottie_from_json_and_render() {
    let dll_path = get_dll_path();
    assert!(dll_path.exists(), "rlottie.dll must exist at {:?}", dll_path);

    let minimal_lottie_json = r#"{
        "v": "5.5.2",
        "fr": 60,
        "ip": 0,
        "op": 60,
        "w": 512,
        "h": 512,
        "nm": "Test Animation",
        "ddd": 0,
        "assets": [],
        "layers": []
    }"#;

    let anim = RLottieAnimation::from_json(minimal_lottie_json, &dll_path)
        .expect("Failed to create RLottieAnimation from valid JSON");

    assert_eq!(anim.total_frames(), 61);
    assert!((anim.frame_rate() - 60.0).abs() < 0.01);

    let (w, h) = anim.dimensions();
    assert_eq!(w, 512);
    assert_eq!(h, 512);

    let mut buffer = vec![0u32; 512 * 512];
    anim.render_frame(0, &mut buffer, 512, 512, 512 * 4);
    // Buffer rendered without panic
    assert_eq!(buffer.len(), 512 * 512);
}

#[test]
fn test_rlottie_two_different_animations() {
    let dll_path = get_dll_path();

    let json1 = r#"{
        "v": "5.5.2",
        "fr": 30,
        "ip": 0,
        "op": 10,
        "w": 100,
        "h": 100,
        "nm": "Anim 1",
        "ddd": 0,
        "assets": [],
        "layers": []
    }"#;

    let json2 = r#"{
        "v": "5.5.2",
        "fr": 60,
        "ip": 0,
        "op": 50,
        "w": 500,
        "h": 500,
        "nm": "Anim 2",
        "ddd": 0,
        "assets": [],
        "layers": []
    }"#;

    let anim1 = RLottieAnimation::from_json(json1, &dll_path).unwrap();
    assert_eq!(anim1.total_frames(), 11);
    assert_eq!(anim1.dimensions(), (100, 100));

    let anim2 = RLottieAnimation::from_json(json2, &dll_path).unwrap();
    println!("anim2 total_frames: {}, dimensions: {:?}", anim2.total_frames(), anim2.dimensions());
    assert_eq!(anim2.total_frames(), 51, "Anim 2 MUST NOT return Anim 1 cached frames!");
    assert_eq!(anim2.dimensions(), (500, 500), "Anim 2 MUST NOT return Anim 1 cached dimensions!");
}

#[test]
fn test_rlottie_invalid_json() {
    let dll_path = get_dll_path();
    let res = RLottieAnimation::from_json("not valid json", &dll_path);
    assert!(res.is_err(), "Invalid JSON must return an error");
}
