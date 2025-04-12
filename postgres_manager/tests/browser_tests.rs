use insta::assert_debug_snapshot;

// We need to use the crate name directly since we're in an integration test
use postgres_manager::ui::models::{BackupMetadata, FocusField, InputMode, PopupState, PostgresConfig, S3Config};
use aws_sdk_s3::primitives::DateTime as AwsDateTime;

// Helper function to create a test AwsDateTime with a fixed timestamp
fn create_test_aws_datetime() -> AwsDateTime {
    // Use a fixed timestamp for testing (2023-01-01 12:00:00 UTC)
    AwsDateTime::from_secs(1672574400)
}

#[test]
fn test_s3_config() {
    let s3_config = S3Config {
        bucket: "test-bucket".to_string(),
        region: "us-west-2".to_string(),
        prefix: "test-prefix".to_string(),
        endpoint_url: "https://test-endpoint.com".to_string(),
        access_key_id: "test-access-key".to_string(),
        secret_access_key: "test-secret-key".to_string(),
        path_style: false,
        error_message: None,
    };
    
    assert_debug_snapshot!(s3_config);
}

#[test]
fn test_postgres_config() {
    let pg_config = PostgresConfig {
        host: Some("localhost".to_string()),
        port: Some(5432),
        username: Some("postgres".to_string()),
        password: Some("password".to_string()),
        use_ssl: false,
        db_name: Some("postgres".to_string()),
    };
    
    assert_debug_snapshot!(pg_config);
}

#[test]
fn test_backup_metadata() {
    let datetime = create_test_aws_datetime();
    let backup = BackupMetadata {
        key: "test-snapshot-1.sql.gz".to_string(),
        size: 1024 * 1024 * 10, // 10 MB
        last_modified: datetime,
    };
    
    assert_debug_snapshot!(backup);
}

#[test]
fn test_popup_states() {
    let datetime = create_test_aws_datetime();
    let backup = BackupMetadata {
        key: "test-snapshot-1.sql.gz".to_string(),
        size: 1024 * 1024 * 10, // 10 MB
        last_modified: datetime,
    };
    
    let hidden = PopupState::Hidden;
    let confirm_restore = PopupState::ConfirmRestore(backup.clone());
    let downloading = PopupState::Downloading(backup.clone(), 0.5, 1024.0 * 1024.0);
    let confirm_cancel = PopupState::ConfirmCancel(backup.clone(), 0.5, 1024.0 * 1024.0);
    let error = PopupState::Error("Test error message".to_string());
    let success = PopupState::Success("Test success message".to_string());
    
    assert_debug_snapshot!("popup_state_hidden", hidden);
    assert_debug_snapshot!("popup_state_confirm_restore", confirm_restore);
    assert_debug_snapshot!("popup_state_downloading", downloading);
    assert_debug_snapshot!("popup_state_confirm_cancel", confirm_cancel);
    assert_debug_snapshot!("popup_state_error", error);
    assert_debug_snapshot!("popup_state_success", success);
}

#[test]
fn test_focus_fields() {
    assert_debug_snapshot!("focus_field_snapshot_list", FocusField::SnapshotList);
    assert_debug_snapshot!("focus_field_bucket", FocusField::Bucket);
    assert_debug_snapshot!("focus_field_region", FocusField::Region);
    assert_debug_snapshot!("focus_field_prefix", FocusField::Prefix);
    assert_debug_snapshot!("focus_field_endpoint_url", FocusField::EndpointUrl);
    assert_debug_snapshot!("focus_field_access_key_id", FocusField::AccessKeyId);
    assert_debug_snapshot!("focus_field_secret_access_key", FocusField::SecretAccessKey);
    assert_debug_snapshot!("focus_field_path_style", FocusField::PathStyle);
}

#[test]
fn test_input_modes() {
    assert_debug_snapshot!("input_mode_normal", InputMode::Normal);
    assert_debug_snapshot!("input_mode_editing", InputMode::Editing);
}
