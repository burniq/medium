use medium_cli::paths::AppPaths;
use std::path::PathBuf;

#[test]
fn linux_paths_use_xdg_layout() {
    let paths = AppPaths::for_linux_home("/home/tester");

    assert_eq!(paths.app_config_dir, PathBuf::from("/home/tester/.medium"));
    assert_eq!(
        paths.state_dir,
        PathBuf::from("/home/tester/.local/share/medium")
    );
    assert_eq!(
        paths.state_path,
        PathBuf::from("/home/tester/.local/share/medium/state.json")
    );
}

#[test]
fn macos_paths_use_application_support() {
    let paths = AppPaths::for_macos_home("/Users/tester");

    assert_eq!(paths.app_config_dir, PathBuf::from("/Users/tester/.medium"));
    assert_eq!(
        paths.state_dir,
        PathBuf::from("/Users/tester/Library/Application Support/Medium/state")
    );
    assert_eq!(
        paths.state_path,
        PathBuf::from("/Users/tester/Library/Application Support/Medium/state/state.json")
    );
}
