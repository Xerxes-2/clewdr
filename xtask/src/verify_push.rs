//! Verify the image publishing path against a throwaway local registry.
//!
//! The docker job's push step is the one part of the pipeline that a nix build
//! cannot reach: it talks to a registry. Three bugs in it were found only by
//! pushing a commit and watching CI (`crane push` refusing a gzipped tarball,
//! `crane index append` silently doing nothing when given positional manifests,
//! and the image tag scheme). This runs the same script CI runs, against a
//! registry in a container, and then checks what came out the other side.

use std::process::Command;

use crate::{info, probe, run, step, warn, workspace_root};

/// The tag that stands in for a release, so the `latest` branch of the tag
/// computation is exercised too.
const REF: &str = "refs/tags/v0.0.0-verify";
const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
const PORT: u16 = 5001;
const CONTAINER: &str = "clewdr-verify-registry";

pub fn run_verification() -> Result<(), String> {
    let runtime = container_runtime()
        .ok_or("neither podman nor docker is on PATH; one is needed to run a registry")?;
    if probe("crane", &["version"]).is_none() {
        return Err(
            "crane is not on PATH; try `nix shell .#crane -c cargo xtask verify-push`".to_string(),
        );
    }

    let root = workspace_root();
    step("Building both images");
    let (amd64, arm64) = image_paths()?;

    step(&format!("Starting a registry ({runtime})"));
    stop_registry(runtime); // A previous interrupted run may have left one.
    run(
        runtime,
        &[
            "run",
            "-d",
            "--name",
            CONTAINER,
            "-p",
            &format!("{PORT}:5000"),
            "docker.io/library/registry:2",
        ],
        &root,
    )
    .map_err(|_| "could not start the registry".to_string())?;

    let result = publish_and_check(&root, &amd64, &arm64);

    step("Stopping the registry");
    stop_registry(runtime);
    result
}

fn publish_and_check(root: &std::path::Path, amd64: &str, arm64: &str) -> Result<(), String> {
    let image = format!("localhost:{PORT}/clewdr");

    step("Publishing (the same script the docker job runs)");
    run(
        "./publish-images.sh",
        &[&image, REF, SHA, amd64, arm64],
        root,
    )
    .map_err(|_| "the publish script failed".to_string())?;

    step("Checking the tags");
    let listed = probe("crane", &["ls", &image]).ok_or("could not list tags")?;
    let found: Vec<&str> = listed.lines().map(str::trim).collect();
    // What the tag computation should have produced for a tag ref: the tag
    // itself, latest, sha-<7>, and a per-arch tag for each of those three.
    for expected in [
        "v0.0.0-verify",
        "latest",
        "sha-0123456", // ${sha:0:7}, the scheme type=sha used
        "v0.0.0-verify-amd64",
        "v0.0.0-verify-arm64",
    ] {
        if !found.contains(&expected) {
            return Err(format!(
                "tag `{expected}` is missing; the registry has: {}",
                found.join(", ")
            ));
        }
    }
    info(&format!("{} tags, all expected ones present", found.len()));

    step("Checking the multi-arch manifests");
    for tag in ["v0.0.0-verify", "latest", "sha-0123456"] {
        let reference = format!("{image}:{tag}");
        let manifest = probe("crane", &["manifest", &reference])
            .ok_or(format!("could not read the manifest of {reference}"))?;
        let platforms = platforms_in(&manifest);
        if platforms != ["linux/amd64", "linux/arm64"] {
            return Err(format!(
                "{reference} should be an index of linux/amd64 + linux/arm64, but has: {}",
                if platforms.is_empty() {
                    "no platforms (not an index?)".to_string()
                } else {
                    platforms.join(", ")
                }
            ));
        }
        info(&format!("{reference} -> linux/amd64 + linux/arm64"));
    }

    Ok(())
}

/// The platforms an image index advertises, sorted.
///
/// Parsed by hand rather than with a json dependency: xtask has none, and the
/// shape being looked for is two `"architecture"`/`"os"` pairs.
fn platforms_in(manifest: &str) -> Vec<String> {
    let mut platforms: Vec<String> = manifest
        .split("\"platform\"")
        .skip(1)
        .filter_map(|chunk| {
            let arch = json_string_after(chunk, "\"architecture\"")?;
            let os = json_string_after(chunk, "\"os\"")?;
            Some(format!("{os}/{arch}"))
        })
        .collect();
    platforms.sort();
    platforms.dedup();
    platforms
}

fn json_string_after(haystack: &str, key: &str) -> Option<String> {
    let rest = haystack.split_once(key)?.1;
    let rest = rest.split_once('"')?.1;
    let (value, _) = rest.split_once('"')?;
    Some(value.to_string())
}

/// `nix build` the image and return the tarball path.
/// Build both images in one `nix build`, so the two architectures overlap
/// rather than queueing behind each other, and return their tarball paths.
fn image_paths() -> Result<(String, String), String> {
    let built = Command::new("nix")
        .args([
            "build",
            "-j",
            "2",
            "--no-link",
            ".#image-amd64",
            ".#image-arm64",
        ])
        .current_dir(workspace_root())
        .status()
        .map_err(|_| "nix is not on PATH".to_string())?;
    if !built.success() {
        return Err("building the images failed".to_string());
    }
    Ok((image_path("image-amd64")?, image_path("image-arm64")?))
}

/// The tarball path of an already-built image.
fn image_path(attr: &str) -> Result<String, String> {
    let output = Command::new("nix")
        .args(["eval", "--raw", &format!(".#{attr}")])
        .current_dir(workspace_root())
        .output()
        .map_err(|_| "nix is not on PATH".to_string())?;
    if !output.status.success() {
        return Err(format!("`nix eval .#{attr}` failed"));
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return Err(format!("`nix eval .#{attr}` produced no path"));
    }
    Ok(path)
}

fn container_runtime() -> Option<&'static str> {
    ["podman", "docker"]
        .into_iter()
        .find(|runtime| probe(runtime, &["--version"]).is_some())
}

/// Remove the registry container, ignoring the failure when there is none.
fn stop_registry(runtime: &str) {
    let removed = Command::new(runtime)
        .args(["rm", "-f", CONTAINER])
        .current_dir(workspace_root())
        .output();
    if removed.is_err() {
        warn("could not remove the registry container; remove it by hand");
    }
}
