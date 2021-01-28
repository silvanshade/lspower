#![deny(clippy::all)]
#![deny(unsafe_code)]

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

fn main() -> Fallible<()> {
    let help = r#"
xtask

USAGE:
    xtask [SUBCOMMAND]

FLAGS:
    -h, --help          Prints help information

SUBCOMMANDS:
    build
    check
    clippy
    doc
    format
    help                Prints this message or the help of the given subcommand(s)
    install
    test
"#
    .trim();

    let mut args: Vec<_> = std::env::args_os().collect();
    // remove "xtask" argument
    args.remove(0);

    let cargo_args = if let Some(dash_dash) = args.iter().position(|arg| arg == "--") {
        let c = args.drain(dash_dash + 1 ..).collect();
        args.pop();
        c
    } else {
        Vec::new()
    };

    let mut args = pico_args::Arguments::from_vec(args);
    match args.subcommand()?.as_deref() {
        Some("build") => {
            subcommand::cargo::build(args, &cargo_args)?;
            return Ok(());
        },
        Some("check") => {
            subcommand::cargo::check(args, &cargo_args)?;
            return Ok(());
        },
        Some("clippy") => {
            subcommand::cargo::clippy(args, &cargo_args)?;
            return Ok(());
        },
        Some("doc") => {
            subcommand::cargo::doc(args, &cargo_args)?;
            return Ok(());
        },
        Some("format") => {
            subcommand::cargo::format(args, &cargo_args)?;
            return Ok(());
        },
        Some("help") => {
            println!("{}\n", help);
            return Ok(());
        },
        Some("test") => {
            subcommand::cargo::test(args, &cargo_args)?;
            return Ok(());
        },
        Some(subcommand) => {
            return Err(format!("unknown subcommand: {}", subcommand).into());
        },
        None => {
            if args.contains(["-h", "--help"]) {
                println!("{}\n", help);
                return Ok(());
            }
        },
    }

    if let Err(pico_args::Error::UnusedArgsLeft(args)) = args.finish() {
        return Err(format!("unrecognized arguments: {}", args.join(" ")).into());
    }

    Ok(())
}

mod metadata {
    use std::path::{Path, PathBuf};

    pub fn cargo() -> crate::Fallible<String> {
        // NOTE: we use the cargo wrapper rather than the binary reported through the "CARGO" environment
        // variable because we need to be able to invoke cargo with different toolchains (e.g., +nightly)
        Ok(String::from("cargo"))
    }

    pub fn project_root() -> PathBuf {
        Path::new(&env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(1)
            .unwrap()
            .to_path_buf()
    }
}

mod subcommand {
    pub mod cargo {
        use crate::metadata;
        use std::process::Command;

        pub fn build(mut args: pico_args::Arguments, cargo_args: &[std::ffi::OsString]) -> crate::Fallible<()> {
            let help = r#"
xtask-build

USAGE:
    xtask build

FLAGS:
    -h, --help          Prints help information
    -- '...'        Extra arguments to pass to the underlying cargo command
"#
            .trim();

            if args.contains(["-h", "--help"]) {
                println!("{}\n", help);
                return Ok(());
            }

            let cargo = metadata::cargo()?;
            let mut cmd = Command::new(cargo);
            cmd.current_dir(metadata::project_root());
            cmd.args(&["build", "--package", "lspower"]);
            cmd.args(cargo_args);
            cmd.status()?;

            Ok(())
        }

        pub fn check(mut args: pico_args::Arguments, cargo_args: &[std::ffi::OsString]) -> crate::Fallible<()> {
            let help = r#"
xtask-check

USAGE:
    xtask check

FLAGS:
    -h, --help          Prints help information
    -- '...'        Extra arguments to pass to the underlying cargo command
"#
            .trim();

            if args.contains(["-h", "--help"]) {
                println!("{}\n", help);
                return Ok(());
            }

            let cargo = metadata::cargo()?;
            let mut cmd = Command::new(cargo);
            cmd.current_dir(metadata::project_root());
            cmd.env("RUSTFLAGS", "-Dwarnings");
            cmd.args(&["check", "--all-targets"]);
            cmd.args(&["--package", "xtask"]);
            cmd.args(&["--package", "lspower"]);
            cmd.args(cargo_args);
            cmd.status()?;
            Ok(())
        }

        pub fn clippy(mut args: pico_args::Arguments, cargo_args: &[std::ffi::OsString]) -> crate::Fallible<()> {
            let help = r#"
xtask-clippy

USAGE:
    xtask clippy

FLAGS:
    -h, --help          Prints help information
    -- '...'        Extra arguments to pass to the underlying cargo command
"#
            .trim();

            if args.contains(["-h", "--help"]) {
                println!("{}\n", help);
                return Ok(());
            }

            let cargo = metadata::cargo()?;
            let mut cmd = Command::new(cargo);
            cmd.current_dir(metadata::project_root());
            cmd.args(&["clippy", "--all-targets"]);
            cmd.args(&["--package", "xtask"]);
            cmd.args(&["--package", "lspower"]);
            cmd.args(cargo_args);
            cmd.args(&["--", "-D", "warnings"]);
            cmd.status()?;
            Ok(())
        }

        pub fn doc(mut args: pico_args::Arguments, cargo_args: &[std::ffi::OsString]) -> crate::Fallible<()> {
            let help = r#"
xtask-doc

USAGE:
    xtask doc

FLAGS:
    -h, --help          Prints help information
    -- '...'        Extra arguments to pass to the underlying cargo command
"#
            .trim();

            if args.contains(["-h", "--help"]) {
                println!("{}\n", help);
                return Ok(());
            }

            let cargo = metadata::cargo()?;
            let mut cmd = Command::new(cargo);
            cmd.current_dir(metadata::project_root());
            cmd.args(&["+nightly", "doc"]);
            cmd.args(cargo_args);
            cmd.status()?;
            Ok(())
        }

        pub fn format(mut args: pico_args::Arguments, cargo_args: &[std::ffi::OsString]) -> crate::Fallible<()> {
            let help = r#"
xtask-format

USAGE:
    xtask format

FLAGS:
    -h, --help          Prints help information
    -- '...'        Extra arguments to pass to the underlying cargo command
"#
            .trim();

            if args.contains(["-h", "--help"]) {
                println!("{}\n", help);
                return Ok(());
            }

            let cargo = metadata::cargo()?;
            let mut cmd = Command::new(cargo);
            cmd.current_dir(metadata::project_root());
            cmd.args(&["+nightly", "fmt", "--all"]);
            cmd.args(cargo_args);
            cmd.status()?;
            Ok(())
        }

        pub fn test(mut args: pico_args::Arguments, cargo_args: &[std::ffi::OsString]) -> crate::Fallible<()> {
            let help = r#"
xtask-test

USAGE:
    xtask test

FLAGS:
    -h, --help          Prints help information
    -- '...'        Extra arguments to pass to the underlying cargo command
"#
            .trim();

            if args.contains(["-h", "--help"]) {
                println!("{}\n", help);
                return Ok(());
            }

            let cargo = metadata::cargo()?;
            let mut cmd = Command::new(cargo);
            cmd.current_dir(metadata::project_root());
            cmd.env("RUSTFLAGS", "-Dwarnings");
            cmd.args(&["test", "--examples", "--lib", "--tests"]);
            cmd.args(&["--package", "xtask"]);
            cmd.args(&["--package", "lspower"]);
            cmd.args(cargo_args);
            cmd.status()?;

            Ok(())
        }
    }
}
