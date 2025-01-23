#[cfg(all(test, feature = "full_tests"))]
mod tests;

mod command;
pub mod compile;
pub mod config;
pub mod ext;
mod logger;
pub mod service;
pub mod signal;

use crate::config::Commands;
use crate::ext::anyhow::{Context, Result};
use crate::ext::PathBufExt;
use crate::logger::GRAY;
use camino::Utf8PathBuf;
use config::{Cli, CommandType, Config};
use ext::fs;
use signal::Interrupt;
use std::env;
use std::path::PathBuf;

pub async fn run(args: Cli) -> Result<()> {
    if let Commands::New(new) = &args.command {
        return new.run().await;
    }

    let command_type = CommandType::from(&args.command);
    let config = resolve_config(args)?;
    execute_command(command_type, config).await?;

    Ok(())
}

pub fn resolve_config(args: Cli) -> Result<Config> {
    let verbose = args.opts().map(|o| o.verbose).unwrap_or(0);
    logger::setup(verbose, &args.log);

    let manifest_path = args
        .manifest_path
        .to_owned()
        .unwrap_or_else(|| Utf8PathBuf::from("Cargo.toml"))
        .resolve_home_dir()
        .context(format!("manifest_path: {:?}", &args.manifest_path))?;
    let mut cwd = Utf8PathBuf::from_path_buf(env::current_dir().unwrap()).unwrap();
    cwd.clean_windows_path();

    let opts = args.opts().unwrap();
    let bin_args = args.bin_args();

    let watch = matches!(args.command, Commands::Watch(_));
    let config = Config::load(opts, &cwd, &manifest_path, watch, bin_args).dot()?;
    env::set_current_dir(&config.working_dir).dot()?;
    log::debug!(
        "Path working dir {}",
        GRAY.paint(config.working_dir.as_str())
    );

    Ok(config)
}

pub async fn execute_command(command: CommandType, config: Config) -> Result<()> {
    if config.working_dir.join("package.json").exists() {
        log::debug!("Path found 'package.json' adding 'node_modules/.bin' to PATH");
        let node_modules = &config.working_dir.join("node_modules");
        if node_modules.exists() {
            match env::var("PATH") {
                Ok(path) => {
                    let mut path_dirs: Vec<PathBuf> = env::split_paths(&path).collect();
                    path_dirs.insert(0, node_modules.join(".bin").into_std_path_buf());
                    // unwrap is safe, because we got the paths from the actual PATH variable
                    env::set_var("PATH", env::join_paths(path_dirs).unwrap());
                }
                Err(_) => log::warn!("Path PATH environment variable not found, ignoring"),
            }
        } else {
            log::warn!(
                "Path 'node_modules' folder not found, please install the required packages first"
            );
            log::warn!("Path continuing without using 'node_modules'");
        }
    }

    let _monitor = Interrupt::run_ctrl_c_monitor();
    match command {
        CommandType::New => unreachable!(),
        CommandType::Build => command::build_all(&config).await,
        CommandType::Serve => command::serve(&config.current_project()?).await,
        CommandType::Test => command::test_all(&config).await,
        CommandType::EndToEnd => command::end2end_all(&config).await,
        CommandType::Watch => command::watch(&config.current_project()?).await,
    }
}
