use anyhow::{anyhow, Result};
use cargo_metadata::MetadataCommand;
use std::{env, fs, path::PathBuf, process};

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
    let mut cmd = process::Command::new(&cargo);
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

    let metadata = MetadataCommand::new().exec().unwrap();
    let target = metadata.target_directory.to_str().unwrap();

    let sysroot = target.to_owned() + "/sysroot";
    let grub_out = target.to_owned() + "/sysroot/boot/grub";
    let kernel_out = target.to_owned() + "/sysroot/boot/kernel.bin";
    let iso_out = target.to_owned() + "/os.iso";
    let grub_cfg = target.to_owned() + "/sysroot/boot/grub/grub.cfg";

    fs::create_dir_all(grub_out)?;
    fs::copy(executables[0].to_owned(), kernel_out)?;
    fs::write(
        grub_cfg,
        "set timeout=0\nset default=0\n\nmenuentry \"My OS\" {\n \
            \tmultiboot2 /boot/kernel.bin\n\tboot\n}",
    )?;

    let _output = process::Command::new("grub-mkrescue")
        .args(&["-o", &iso_out, &sysroot])
        .output()
        .expect("Failed to execute grub-mkrescue");

    let _output = process::Command::new("qemu-system-x86_64")
        .args(&["-cdrom", &iso_out])
        .output()
        .expect("QEMU system-x86_64 failed");

    Ok(())
}
