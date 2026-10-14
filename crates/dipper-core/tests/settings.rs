use dipper_core::settings::Settings;
use std::os::unix::fs::PermissionsExt;
#[test]
fn private_settings_roundtrip_and_reject_corruption() {
    let store = tempfile::tempdir().unwrap();
    let data = store.path().join("data");
    let path = data.join("settings.json");
    let mut settings = Settings::defaults(&data);
    settings.input_name = Some("usb-microphone".into());
    settings.preferred_hold = "Menu".into();
    settings.save(&path).unwrap();
    let loaded = Settings::load(&path, &data).unwrap();
    assert_eq!(loaded.input_name.as_deref(), Some("usb-microphone"));
    assert_eq!(loaded.preferred_hold, "Menu");
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(&data).unwrap().permissions().mode() & 0o777,
        0o700
    );
    std::fs::write(&path, b"{corrupt").unwrap();
    assert!(Settings::load(&path, &data).is_err());
}
#[test]
fn missing_model_is_explicit_setup_failure() {
    let store = tempfile::tempdir().unwrap();
    let mut settings = Settings::defaults(store.path());
    settings.model_path = store.path().join("absent-model.bin");
    assert!(settings.ready_for_recognition().is_err());
    settings.preferred_hold.clear();
    assert!(settings.validate().is_err());
}

#[test]
fn existing_settings_keep_baseline_and_new_settings_select_turbo() {
    use dipper_core::protocol::RecognitionModel;
    let store = tempfile::tempdir().unwrap();
    let path = store.path().join("settings.json");
    let fresh = Settings::load(&path, store.path()).unwrap();
    assert_eq!(fresh.model_profile, RecognitionModel::LargeV3Turbo);
    if std::env::var_os("DIPPER_MODEL_PATH").is_none() {
        assert_eq!(
            fresh.model_path,
            store.path().join("models/ggml-large-v3-turbo.bin")
        );
    }
    let mut old = serde_json::to_value(&fresh).unwrap();
    old.as_object_mut().unwrap().remove("model_profile");
    old["model_path"] = serde_json::json!(store.path().join("original-model.bin"));
    std::fs::write(&path, old.to_string()).unwrap();
    let mut loaded = Settings::load(&path, store.path()).unwrap();
    assert_eq!(loaded.model_profile, RecognitionModel::LargeV3);
    assert_eq!(loaded.model_path, store.path().join("original-model.bin"));
    loaded.model_profile = RecognitionModel::LargeV3Turbo;
    loaded.model_path = store.path().join("turbo.bin");
    loaded.save(&path).unwrap();
    let reopened = Settings::load(&path, store.path()).unwrap();
    assert_eq!(reopened.model_profile, RecognitionModel::LargeV3Turbo);
    assert_eq!(reopened.model_path, loaded.model_path);
    old["model_profile"] = serde_json::json!("unknown_model");
    std::fs::write(&path, old.to_string()).unwrap();
    assert!(Settings::load(&path, store.path()).is_err());
}

#[test]
fn recognition_profiles_preserve_legacy_requests_and_reject_unknown_models() {
    use dipper_core::protocol::{RecognitionModel, RecognitionSettings};
    let baseline = RecognitionSettings::large_v3("/models/renamed.bin".into());
    assert_eq!(baseline.model_profile, RecognitionModel::LargeV3);
    assert_eq!(baseline.model_profile.decoder_layers(), 32);
    let mut legacy = serde_json::to_value(&baseline).unwrap();
    legacy.as_object_mut().unwrap().remove("model_profile");
    let decoded: RecognitionSettings = serde_json::from_value(legacy.clone()).unwrap();
    assert_eq!(decoded.model_profile, RecognitionModel::LargeV3);
    assert_eq!(decoded.language, "auto");
    decoded.validate().unwrap();
    let turbo = RecognitionSettings::for_model(RecognitionModel::LargeV3Turbo, baseline.model_path);
    assert_eq!(turbo.model_profile.decoder_layers(), 4);
    turbo.validate().unwrap();
    legacy["model_profile"] = serde_json::json!("large_v3_turbo_quantized");
    assert!(serde_json::from_value::<RecognitionSettings>(legacy).is_err());
}
