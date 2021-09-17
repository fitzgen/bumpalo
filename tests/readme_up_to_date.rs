use std::fs;
use std::io::Write;
use std::process::Command;

#[test]
fn cargo_readme_up_to_date() {
    println!("Checking that `cargo readme > README.md` is up to date...");

    let child = match Command::new("cargo-readme")
        .arg("readme")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdout(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            warn(e);
            return;
        }
    };

    let output = match child.wait_with_output() {
        Ok(o) => o,
        Err(e) => {
            warn(e);
            return;
        }
    };

    if !output.status.success() {
        warn(format!(
            "exited with non-success code {}",
            output.status.code().unwrap_or(i32::MAX)
        ));
        return;
    }

    let expected = String::from_utf8_lossy(&output.stdout);
    let actual = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))
        .expect("should read README.md OK");

    if actual.trim() != expected.trim() {
        panic!("Run `cargo readme > README.md` to update README.md");
    }
    return;

    fn warn(e: impl std::fmt::Display) {
        let stderr = std::io::stderr();
        let mut stderr = stderr.lock();
        let _ = writeln!(
            &mut stderr,
            "================================================================================\n\
             WARNING: spawning `cargo-readme` failed; is it installed?\n\
             `cargo-readme` error: {}\n\
             Skipping `cargo-readme` check.\n\
             ================================================================================",
            e
        );
    }
}
