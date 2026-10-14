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
