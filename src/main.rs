#![feature(io_error_more)]

use std::{
    fs::{copy, create_dir, read_dir, remove_dir, remove_file, rename},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

use clap::Parser;
use console::{Style, Term};
use eyre::{bail, Result, WrapErr};
use fs_extra::dir::{move_dir, CopyOptions};
use once_cell::sync::Lazy;

static PATH_STYLE: Lazy<Style> = Lazy::new(|| Style::new().blue().italic());

/// Simple program to explode a directory
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// The directory to explode
    source: PathBuf,

    /// The output directory
    #[clap(default_value = ".")]
    destination: PathBuf,

    /// Print what is beeing done
    #[clap(short, long)]
    verbose: bool,

    /// Don't do anything
    #[clap(short, long)]
    dry_run: bool,

    /// Overwrite existing files and delete directory even if its not empty
    #[clap(short, long)]
    force: bool,
}

fn main() -> eyre::Result<()> {
    color_eyre::install()?;

    let args = Args::parse();
    let mut term = Term::stdout();

    explode(&mut term, &args)?;

    Ok(())
}

fn explode(term: &mut Term, args: &Args) -> Result<()> {
    move_files(term, args)?;
    remove_source_directory(term, args)?;

    writeln!(
        term,
        "Exploded {} to {}",
        PATH_STYLE.apply_to(args.source.display()),
        PATH_STYLE.apply_to(args.destination.display())
    )?;

    Ok(())
}

fn move_files(term: &mut Term, args: &Args) -> Result<()> {
    if args.verbose {
        writeln!(
            term,
            "Moving files in {} -> {}",
            PATH_STYLE.apply_to(args.source.display()),
            PATH_STYLE.apply_to(args.destination.display())
        )?;
    }

    if !args.source.exists() {
        bail!("Source path {} does not exist", args.source.display())
    }
    if !args.source.is_dir() {
        bail!("Source path {} is not a directory", args.source.display())
    }
    if !args.destination.exists() && !args.dry_run {
        create_dir(&args.destination).wrap_err("Failed to create destination dir")?;
    }
    if args.destination.exists() && !args.destination.is_dir() {
        bail!("Target path {} is not a directory", args.source.display())
    }

    for entry in read_dir(&args.source)
        .wrap_err_with(|| format!("Failed to read directory {}", args.source.display()))?
    {
        let entry = entry.wrap_err("Failed to read directory entry")?;
        let new_path = args.destination.join(entry.file_name());

        move_path(term, args, &entry.path(), &new_path)?;
    }

    Ok(())
}

fn move_path(term: &mut Term, args: &Args, from: &Path, to: &Path) -> Result<()> {
    if to.exists() {
        let what = match from {
            p if p.is_dir() => "Dir",
            p if p.is_file() => "File",
            _ => "Entry",
        };
        if !args.force {
            bail!(
                "{what} {} already exists in {}",
                from.file_name().unwrap().to_string_lossy(),
                args.destination.display()
            );
        }
    }

    if args.verbose {
        writeln!(
            term,
            "Moving {} -> {}",
            PATH_STYLE.apply_to(from.display()),
            PATH_STYLE.apply_to(to.display())
        )?;
    }

    if !args.dry_run {
        move_or_copy(from, to, args.force).wrap_err_with(|| {
            format!(
                "Failed to move or copy {} to {}",
                from.display(),
                to.display()
            )
        })?;
    }

    Ok(())
}

fn move_or_copy(from: &Path, to: &Path, overwrite: bool) -> Result<()> {
    match rename(from, to) {
        Err(err) if err.kind() == ErrorKind::CrossesDevices => {
            if from.is_dir() {
                let mut options = CopyOptions::new();
                options.overwrite = overwrite;
                create_dir(to)?;
                move_dir(from, to, &options)?;
            } else {
                copy(from, to)?;
                remove_file(from)?;
            }
        }
        result => {
            result?;
        }
    };

    Ok(())
}

fn remove_source_directory(term: &mut Term, args: &Args) -> Result<()> {
    if args.verbose {
        writeln!(
            term,
            "Removed {}",
            PATH_STYLE.apply_to(args.source.display())
        )?;
    }

    if !args.dry_run {
        remove_dir(&args.source)
            .wrap_err_with(|| format!("Failed to remove directory {}", args.source.display()))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use assert_fs::{
        assert::PathAssert,
        fixture::{FileTouch, FileWriteStr, PathChild},
        TempDir,
    };
    use clap::IntoApp;
    use predicates::prelude::predicate;

    use super::*;

    #[test]
    fn verify_cli() {
        Args::command().debug_assert()
    }

    #[test]
    fn normal() {
        let work_dir = TempDir::new().unwrap();

        for x in 1..5 {
            work_dir
                .child(format!("src/{x}"))
                .write_str("Paaag")
                .unwrap();
        }

        let args = Args {
            source: work_dir.join("src"),
            destination: work_dir.join("dst"),
            verbose: true,
            dry_run: false,
            force: false,
        };

        let mut term = Term::stdout();

        explode(&mut term, &args).unwrap();

        work_dir.child("src").assert(predicate::path::missing());
        let path = work_dir.child("dst");
        path.assert(predicate::path::exists());
        for x in 1..5 {
            path.child(x.to_string()).assert("Paaag");
        }

        work_dir.close().unwrap();
    }

    #[test]
    fn dry_run() {
        let work_dir = TempDir::new().unwrap();

        for x in 1..5 {
            work_dir
                .child(format!("src/{x}"))
                .write_str("Paaag")
                .unwrap();
        }

        let args = Args {
            source: work_dir.join("src"),
            destination: work_dir.join("dst"),
            verbose: true,
            dry_run: true,
            force: false,
        };

        let mut term = Term::stdout();

        explode(&mut term, &args).unwrap();

        let path = work_dir.child("src");
        path.assert(predicate::path::exists());
        for x in 1..5 {
            path.child(x.to_string()).assert("Paaag");
        }
        work_dir.child("dst").assert(predicate::path::missing());

        work_dir.close().unwrap();
    }

    #[test]
    fn file_in_output_exists() {
        let work_dir = TempDir::new().unwrap();

        for x in 1..5 {
            work_dir.child(format!("src/{x}")).touch().unwrap();
        }

        work_dir.child("dst/3").write_str("foo bar baz").unwrap();

        let args = Args {
            source: work_dir.join("src"),
            destination: work_dir.join("dst"),
            verbose: true,
            dry_run: false,
            force: false,
        };

        let mut term = Term::stdout();

        let result = explode(&mut term, &args);
        assert!(result.is_err());

        if let Err(err) = result {
            assert_eq!(
                err.to_string(),
                format!("File 3 already exists in {}", args.destination.display())
            );
        } else {
            unreachable!()
        }

        work_dir.child("src").assert(predicate::path::exists());
        work_dir.child("dst").assert(predicate::path::exists());
        work_dir.child("dst/3").assert("foo bar baz");

        work_dir.close().unwrap();
    }

    #[test]
    fn file_in_output_exists_overwrite() {
        let work_dir = TempDir::new().unwrap();

        for x in 1..5 {
            work_dir
                .child(format!("src/{x}"))
                .write_str("Paaag")
                .unwrap();
        }

        work_dir.child("dst/3").write_str("FloppaDespair").unwrap();

        let args = Args {
            source: work_dir.join("src"),
            destination: work_dir.join("dst"),
            verbose: true,
            dry_run: false,
            force: true,
        };

        let mut term = Term::stdout();

        explode(&mut term, &args).unwrap();

        work_dir.child("src").assert(predicate::path::missing());
        work_dir.child("dst").assert(predicate::path::exists());
        work_dir.child("dst/3").assert("Paaag");

        work_dir.close().unwrap();
    }
}
