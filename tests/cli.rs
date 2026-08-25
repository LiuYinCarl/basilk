//! End-to-end checks that spawn the real binary. These verify the parts of
//! the program that cannot run inside a unit test (process exit on
//! `--version`, terminal setup on startup).

use std::process::Command;

#[test]
fn version_flag_prints_the_version_and_exits_successfully() {
    let out = Command::new(env!("CARGO_BIN_EXE_basilk"))
        .arg("--version")
        .output()
        .expect("failed to spawn basilk");

    assert!(out.status.success(), "expected exit code 0");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        env!("CARGO_PKG_VERSION"),
        "--version must print exactly the package version"
    );
}
