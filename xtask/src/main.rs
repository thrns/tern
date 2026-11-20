use std::env;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

mod membench;

const USAGE: &str = "usage: xtask <build-core|bundle|membench|gen-corpus>";
const TARGET_TRIPLE: &str = "aarch64-apple-darwin";
const DEPLOYMENT_TARGET: &str = "14.0";
const SWIFT_FILE: &str = "tern_ffi.swift";
const HEADER_FILE: &str = "tern_ffiFFI.h";
const MODULEMAP_CONTENT: &str = "module tern_ffiFFI { header \"tern_ffiFFI.h\" export * }\n";
const SIGN_IDENTITY_ENV: &str = "TERN_SIGN_IDENTITY";
const DEV_IDENTITY: &str = "Tern Dev";
const AD_HOC_IDENTITY: &str = "-";
const BUNDLE_USAGE: &str = "usage: xtask bundle [--release] [--sign <identity>]";

#[derive(Debug)]
enum XtaskError {
    Usage(String),
    Io { context: String, source: io::Error },
    Command { command: String, status: String },
    Missing(PathBuf),
    Membench(membench::MembenchError),
}

impl fmt::Display for XtaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            XtaskError::Usage(message) => write!(f, "{message}"),
            XtaskError::Io { context, source } => write!(f, "{context}: {source}"),
            XtaskError::Command { command, status } => {
                write!(f, "command failed ({status}): {command}")
            }
            XtaskError::Missing(path) => write!(f, "expected file not found: {}", path.display()),
            XtaskError::Membench(error) => write!(f, "{error}"),
        }
    }
}

fn io_error(context: impl Into<String>) -> impl FnOnce(io::Error) -> XtaskError {
    let context = context.into();
    move |source| XtaskError::Io { context, source }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("build-core") => build_core(&args[1..]),
        Some("bundle") => bundle(&args[1..]),
        Some("membench") => membench::run(&args[1..]),
        Some(command) => Err(XtaskError::Usage(format!(
            "unknown command '{command}'\n{USAGE}"
        ))),
        None => Err(XtaskError::Usage(USAGE.to_string())),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(XtaskError::Usage(message)) => {
            eprintln!("xtask: {message}");
            ExitCode::from(2)
        }
        Err(err) => {
            eprintln!("xtask: {err}");
            ExitCode::FAILURE
        }
    }
}

fn workspace_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map_or_else(|| manifest_dir.to_path_buf(), Path::to_path_buf)
}

fn build_core(args: &[String]) -> Result<(), XtaskError> {
    let mut release = false;
    for arg in args {
        match arg.as_str() {
            "--release" => release = true,
            other => {
                return Err(XtaskError::Usage(format!(
                    "build-core: unknown argument '{other}'\nusage: xtask build-core [--release]"
                )));
            }
        }
    }
    let root = workspace_root();
    let profile_dir = if release { "release" } else { "debug" };
    let library_dir = root.join("target").join(TARGET_TRIPLE).join(profile_dir);

    let mut build = cargo();
    build
        .current_dir(&root)
        .env("MACOSX_DEPLOYMENT_TARGET", DEPLOYMENT_TARGET)
        .args(["build", "-p", "tern-ffi", "--target", TARGET_TRIPLE]);
    if release {
        build.arg("--release");
    }
    run(&mut build)?;

    let dylib = library_dir.join("libtern_ffi.dylib");
    if !dylib.is_file() {
        return Err(XtaskError::Missing(dylib));
    }

    let staging = root.join("target").join("uniffi-bindings");
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(io_error(format!(
            "cannot clear staging directory {}",
            staging.display()
        )))?;
    }
    fs::create_dir_all(&staging).map_err(io_error(format!(
        "cannot create staging directory {}",
        staging.display()
    )))?;

    let mut generate = cargo();
    generate
        .current_dir(&root)
        .args([
            "run",
            "-p",
            "tern-ffi",
            "--features",
            "cli",
            "--bin",
            "uniffi-bindgen",
            "--",
            "generate",
            "--library",
        ])
        .arg(&dylib)
        .args(["--language", "swift", "--out-dir"])
        .arg(&staging);
    run(&mut generate)?;

    let swift_dir = root.join("shell/macos/Sources/TernFFI");
    let c_dir = root.join("shell/macos/Sources/TernFFIC");
    let include_dir = c_dir.join("include");
    for dir in [&swift_dir, &include_dir] {
        fs::create_dir_all(dir).map_err(io_error(format!(
            "cannot create directory {}",
            dir.display()
        )))?;
    }

    move_file(&staging.join(SWIFT_FILE), &swift_dir.join(SWIFT_FILE))?;
    move_file(&staging.join(HEADER_FILE), &include_dir.join(HEADER_FILE))?;
    write_file(&include_dir.join("module.modulemap"), MODULEMAP_CONTENT)?;
    write_file(&c_dir.join("shim.c"), "")?;
    Ok(())
}

struct BundleOptions {
    release: bool,
    identity: Option<String>,
}

fn parse_bundle_args(args: &[String]) -> Result<BundleOptions, XtaskError> {
    let mut options = BundleOptions {
        release: false,
        identity: None,
    };
    let mut remaining = args.iter();
    while let Some(arg) = remaining.next() {
        match arg.as_str() {
            "--release" => options.release = true,
            "--sign" => match remaining.next() {
                Some(identity) => options.identity = Some(identity.clone()),
                None => {
                    return Err(XtaskError::Usage(format!(
                        "bundle: --sign requires an identity\n{BUNDLE_USAGE}"
                    )));
                }
            },
            other => {
                return Err(XtaskError::Usage(format!(
                    "bundle: unknown argument '{other}'\n{BUNDLE_USAGE}"
                )));
            }
        }
    }
    Ok(options)
}

fn resolve_identity(explicit: Option<String>) -> String {
    if let Some(identity) = explicit.filter(|value| !value.is_empty()) {
        return identity;
    }
    if let Some(identity) = env::var(SIGN_IDENTITY_ENV)
        .ok()
        .filter(|value| !value.is_empty())
    {
        return identity;
    }
    if dev_identity_available() {
        DEV_IDENTITY.to_string()
    } else {
        AD_HOC_IDENTITY.to_string()
    }
}

fn dev_identity_available() -> bool {
    let quoted = format!("\"{DEV_IDENTITY}\"");
    Command::new("security")
        .args(["find-identity", "-v", "-p", "codesigning"])
        .output()
        .map(|output| {
            output.status.success() && String::from_utf8_lossy(&output.stdout).contains(&quoted)
        })
        .unwrap_or(false)
}

fn swift(root: &Path, profile: &str) -> Command {
    let mut command = Command::new("swift");
    command.current_dir(root).env("TERN_PROFILE", profile);
    command
}

fn bundle(args: &[String]) -> Result<(), XtaskError> {
    let options = parse_bundle_args(args)?;
    let identity = resolve_identity(options.identity);
    let root = workspace_root();
    let release = options.release;
    let profile = if release { "release" } else { "debug" };

    let core_args: Vec<String> = if release {
        vec!["--release".to_string()]
    } else {
        Vec::new()
    };
    build_core(&core_args)?;

    let mut mcp_build = cargo();
    mcp_build
        .current_dir(&root)
        .env("MACOSX_DEPLOYMENT_TARGET", DEPLOYMENT_TARGET)
        .args(["build", "-p", "tern-mcp", "--target", TARGET_TRIPLE]);
    if release {
        mcp_build.arg("--release");
    }
    run(&mut mcp_build)?;
    let mcp_binary = root
        .join("target")
        .join(TARGET_TRIPLE)
        .join(profile)
        .join("tern-mcp");

    let swift_arguments = ["build", "-c", profile, "--arch", "arm64"];
    let mut swift_build = swift(&root, profile);
    swift_build.args(swift_arguments);
    run(&mut swift_build)?;
    let mut swift_bin_path = swift(&root, profile);
    swift_bin_path.args(swift_arguments).arg("--show-bin-path");
    let bin_directory = capture(&mut swift_bin_path)?;
    let app_binary = PathBuf::from(bin_directory.trim()).join("TernApp");

    let resources_source = root.join("shell/macos/Resources");
    let entitlements = resources_source.join("Tern.entitlements");
    let app = root.join("dist").join("Tern.app");
    if app.exists() {
        fs::remove_dir_all(&app).map_err(io_error(format!("cannot remove {}", app.display())))?;
    }
    let contents = app.join("Contents");
    let macos_directory = contents.join("MacOS");
    let resources_directory = contents.join("Resources");
    for directory in [&macos_directory, &resources_directory] {
        fs::create_dir_all(directory).map_err(io_error(format!(
            "cannot create directory {}",
            directory.display()
        )))?;
    }

    copy_file(&app_binary, &macos_directory.join("Tern"))?;
    copy_file(&mcp_binary, &macos_directory.join("tern-mcp"))?;
    copy_file(
        &resources_source.join("Info.plist"),
        &contents.join("Info.plist"),
    )?;
    copy_file(
        &resources_source.join("AppIcon.icns"),
        &resources_directory.join("AppIcon.icns"),
    )?;

    codesign(&identity, &entitlements, &macos_directory.join("tern-mcp"))?;
    codesign(&identity, &entitlements, &app)
}

fn codesign(identity: &str, entitlements: &Path, target: &Path) -> Result<(), XtaskError> {
    let mut command = Command::new("codesign");
    command
        .args(["--force", "--options", "runtime", "--timestamp=none"])
        .arg("--entitlements")
        .arg(entitlements)
        .args(["--sign", identity])
        .arg(target);
    run(&mut command)
}

fn capture(command: &mut Command) -> Result<String, XtaskError> {
    let description = format!("{command:?}");
    let output = command
        .stderr(std::process::Stdio::inherit())
        .output()
        .map_err(io_error(format!("cannot start {description}")))?;
    if !output.status.success() {
        return Err(XtaskError::Command {
            command: description,
            status: output.status.to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn copy_file(from: &Path, to: &Path) -> Result<(), XtaskError> {
    if !from.is_file() {
        return Err(XtaskError::Missing(from.to_path_buf()));
    }
    fs::copy(from, to).map(|_| ()).map_err(io_error(format!(
        "cannot copy {} to {}",
        from.display(),
        to.display()
    )))
}

fn cargo() -> Command {
    Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
}

fn run(command: &mut Command) -> Result<(), XtaskError> {
    let description = format!("{command:?}");
    let status = command
        .status()
        .map_err(io_error(format!("cannot start {description}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(XtaskError::Command {
            command: description,
            status: status.to_string(),
        })
    }
}

fn move_file(from: &Path, to: &Path) -> Result<(), XtaskError> {
    if !from.is_file() {
        return Err(XtaskError::Missing(from.to_path_buf()));
    }
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    fs::copy(from, to).map_err(io_error(format!(
        "cannot copy {} to {}",
        from.display(),
        to.display()
    )))?;
    fs::remove_file(from).map_err(io_error(format!("cannot remove {}", from.display())))
}

fn write_file(path: &Path, content: &str) -> Result<(), XtaskError> {
    fs::write(path, content).map_err(io_error(format!("cannot write {}", path.display())))
}
