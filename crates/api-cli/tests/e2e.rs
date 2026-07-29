use std::{path::PathBuf, process::Command};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn scan_and_export_openapi_fixture() {
    let bin = env!("CARGO_BIN_EXE_api-cli");
    let fixture = repo_root().join("fixtures/openapi-only");
    let status = Command::new(bin)
        .args(["scan", fixture.to_str().expect("path")])
        .status()
        .expect("scan command");
    assert!(status.success());

    let output_file = repo_root().join("generated/openapi.yaml");
    let status = Command::new(bin)
        .args([
            "export",
            "openapi",
            fixture.to_str().expect("path"),
            "--output",
            output_file.to_str().expect("path"),
        ])
        .status()
        .expect("export command");
    assert!(status.success());
    assert!(output_file.exists());
}

#[test]
fn desktop_open_bootstraps_project_workspace() {
    let bin = env!("CARGO_BIN_EXE_api-cli");
    let fixture = repo_root().join("fixtures/openapi-only");
    let status = Command::new(bin)
        .args([
            "desktop",
            "--repository",
            fixture.to_str().expect("path"),
            "--name",
            "Fixture API",
        ])
        .status()
        .expect("desktop command");
    assert!(status.success());

    let project_file = fixture.join(".repo-api/project.json");
    assert!(project_file.exists());
}
