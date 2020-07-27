use anyhow::{anyhow, Context, Result};
use cargo_metadata::MetadataCommand;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};
use wait_timeout::ChildExt;

mod config;

pub fn main() -> Result<()> {
    let mut raw_args = env::args();

    match raw_args.nth(1).as_deref() {
        Some("runner") => {}
        Some("--help") => todo!(),
        Some(any) => return Err(anyhow!("bootimage: Unrecognized option '{}'", any)),
        None => {
            return Err(anyhow!(
                "bootimage: No operation specified (use --help for help)"
            ))
        }
    };

    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let mut cmd = Command::new(&cargo);
    cmd.arg("build");
    cmd.arg("--message-format").arg("json");
    let output = cmd
        .output()
        .map_err(|err| anyhow!("failed to execute kernel build with json: {}", err))?;
    if !output.status.success() {
        return Err(anyhow!("kernel build failed"));
    }
    let mut executables = Vec::new();

    match raw_args.next().as_deref() {
        Some(exe) => executables.push(PathBuf::from(exe)),
        None => {
            for line in String::from_utf8(output.stdout)
                .map_err(|_| anyhow!("Invalid UTF-8"))?
                .lines()
            {
                let mut artifact = json::parse(line).map_err(|_| anyhow!("Invalid JSON"))?;
                if let Some(executable) = artifact["executable"].take_string() {
                    executables.push(PathBuf::from(executable));
                }
            }
        }
    }

    let cmd = MetadataCommand::new();
    let metadata = cmd.exec().unwrap();
    let target = metadata.target_directory;
    let manifest_dir =
        env::var("CARGO_MANIFEST_DIR").context("Failed to read CARGO_MANIFEST_DIR env var")?;
    let cargo_toml = Path::new(&manifest_dir).join("Cargo.toml");
    let is_test = executables[0]
        .parent()
        .ok_or_else(|| anyhow!("kernel binary has no parent"))?
        .ends_with("deps");

    let config = config::read_config(&cargo_toml).context("Failed to read configuration")?;

    let sysroot = target.join("sysroot");
    let iso_out = target.join("os.iso");
    let grub_out = sysroot.join("boot/grub");
    let kernel_out = sysroot.join("boot/kernel.bin");
    let grub_cfg = grub_out.join("grub.cfg");

    fs::create_dir_all(grub_out)?;
    fs::copy(executables[0].to_owned(), kernel_out)?;
    fs::write(
        grub_cfg,
        "set timeout=0\nset default=0\n\nmenuentry \"My OS\" {\n \
            \tmultiboot2 /boot/kernel.bin\n\tboot\n}",
    )?;

    let _output = Command::new("grub-mkrescue")
        .args(&["-o", iso_out.to_str().unwrap(), sysroot.to_str().unwrap()])
        .output()
        .expect("Failed to execute grub-mkrescue");

    let mut extra_args = Vec::new();
    if is_test {
        if let Some(args) = config.test_args {
            extra_args.extend(args);
        }
    } else if let Some(args) = config.run_args {
        extra_args.extend(args);
    }

    let mut output = Command::new("qemu-system-x86_64")
        .args(&["-cdrom", iso_out.to_str().unwrap()])
        .args(&extra_args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("QEMU system-x86_64 failed");

    let timeout = Duration::from_secs(config.test_timeout.into());
    if is_test {
        match output
            .wait_timeout(timeout)
            .context("Failed to wait with timeout")?
        {
            Some(exit_code) => {
                if config.test_success_exit_code.unwrap_or(0)
                    != exit_code.code().unwrap_or_else(|| 0)
                {
                    std::process::exit(exit_code.code().unwrap_or_else(|| 0));
                }
            }
            None => {
                output.kill().context("Failed to kill QEMU")?;
                output.wait().context("Failed to wait for QEMU process")?;
                return Err(anyhow!("Test timed out"));
            }
        }
    }

    Ok(())
}
