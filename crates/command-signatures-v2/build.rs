// We use `std::process::Command` directly here (not the `command` crate's
// blocking wrapper) because the wrapper sets `CREATE_BREAKAWAY_FROM_JOB` on
// Windows for app-process management. Build scripts run inside Cargo's job
// hierarchy and may not have breakaway rights — that flag triggers
// `ERROR_ACCESS_DENIED` (os error 5) when the build is itself launched
// inside a restricted job (e.g. CI, sandboxed agents).
use std::path::Path;
use std::process::Command;

use anyhow::anyhow;

fn main() -> anyhow::Result<()> {
    println!("cargo:rerun-if-changed=js/src");
    println!("cargo:rerun-if-changed=js/build");
    println!("cargo:rerun-if-changed=js/package.json");
    println!("cargo:rerun-if-changed=js/tsconfig.json");

    if let Err(e) = build_command_signatures() {
        if !Path::new(format!("{}/js/build", env!("CARGO_MANIFEST_DIR")).as_str()).exists() {
            panic!(
                r#"Failed to build command signatures JS: {e:?}.

This crate compiles a small TypeScript file at `crates/command-signatures-v2/js`.
The build script tries `yarn build` first, then falls back to a one-shot
`npm install --no-save typescript && npx tsc -p tsconfig.json` so a clean
checkout works on machines without yarn. Both paths failed.

To fix, install Node 18+ (https://nodejs.org) so `npm`/`npx` are on PATH.
If you prefer yarn, run `corepack enable` first. If the failure is from a
conflicting brew-installed yarn on macOS, `brew uninstall yarn` and retry.
"#
            )
        } else {
            println!("cargo:warning=Failed to build command signatures JS ({e:?}). Proceeding with stale command signatures!");
        }
    }
    Ok(())
}

fn build_command_signatures() -> anyhow::Result<()> {
    let js_dir = format!("{}/js", env!("CARGO_MANIFEST_DIR"));

    // Slim fork: yarn/corepack isn't always set up on dev machines, so the
    // npm path is the actual main road for most contributors. Try yarn
    // silently, and only surface diagnostics if BOTH paths fail.
    if run_build_in(&js_dir, "yarn", &["build"]).is_ok() {
        return Ok(());
    }
    run_npm_tsc(&js_dir)?;
    Ok(())
}

fn run_npm_tsc(js_dir: &str) -> anyhow::Result<()> {
    // `--no-save` keeps it out of package.json. `--no-package-lock` is
    // critical: without it npm rewrites the project's yarn.lock into the
    // yarn-classic v1 format (npm sees the existing yarn.lock and tries to
    // be helpful), which corrupts the lockfile every time a yarn-berry user
    // runs `cargo check`.
    run_build_in(
        js_dir,
        "npm",
        &["install", "--no-save", "--no-package-lock", "typescript"],
    )?;
    run_build_in(js_dir, "npx", &["tsc", "-p", "tsconfig.json"])
}

fn run_build_in(dir: &str, program: &str, args: &[&str]) -> anyhow::Result<()> {
    // On Windows, `npm`/`npx`/`yarn` ship as `.cmd` shims and the bare-name
    // form fails to spawn because `CreateProcessW` doesn't auto-append
    // PATHEXT. Probe a few candidates so the fallback actually runs on a
    // fresh checkout instead of silently giving up to the "stale JS" path.
    let candidates: Vec<String> = if cfg!(windows) {
        vec![
            program.to_owned(),
            format!("{program}.cmd"),
            format!("{program}.bat"),
            format!("{program}.exe"),
        ]
    } else {
        vec![program.to_owned()]
    };

    let mut spawn_errors: Vec<String> = Vec::new();
    for candidate in &candidates {
        match Command::new(candidate).args(args).current_dir(dir).output() {
            Ok(output) => {
                return if output.status.success() {
                    Ok(())
                } else {
                    Err(anyhow!(
                        "{candidate} {args:?} failed in {dir}: {:?}",
                        output
                    ))
                };
            }
            Err(e) => {
                spawn_errors.push(format!("{candidate}: {e}"));
            }
        }
    }

    Err(anyhow!(
        "could not spawn `{program}` (tried {}): {}",
        candidates.len(),
        spawn_errors.join("; ")
    ))
}
