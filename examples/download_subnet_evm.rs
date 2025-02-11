use std::{fs, io};

use avalanche_installer::subnet_evm::{github, s3 as subnet_evm_s3};
use aws_manager::{self, s3};
use tokio::time::{sleep, Duration};

/// cargo run --example download_subnet_evm
#[tokio::main]
async fn main() -> io::Result<()> {
    // ref. https://github.com/env-logger-rs/env_logger/issues/47
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    let subnet_evm_path = random_manager::tmp_path(10, None).unwrap();
    github::download_latest(None, None, &subnet_evm_path)
        .await
        .unwrap();
    log::info!("subnet_evm path: {}", subnet_evm_path);

    let shared_config = aws_manager::load_config(
        Some(String::from("us-east-1")),
        Some(Duration::from_secs(30)),
    )
    .await
    .unwrap();
    let s3_manager = s3::Manager::new(&shared_config);
    let s3_bucket = format!(
        "installer-{}",
        random_manager::secure_string(10).to_lowercase()
    );

    s3_manager.create_bucket(&s3_bucket).await.unwrap();

    sleep(Duration::from_secs(2)).await;
    let subnet_evm_s3_key = "sub-dir/subnet_evm".to_string();
    s3_manager
        .put_object(&subnet_evm_path, &s3_bucket, &subnet_evm_s3_key)
        .await
        .unwrap();

    sleep(Duration::from_secs(5)).await;
    let target_bin_path = random_manager::tmp_path(15, None)?;
    subnet_evm_s3::download(
        true,
        &s3_manager,
        &s3_bucket,
        &subnet_evm_s3_key,
        &target_bin_path,
    )
    .await
    .unwrap();

    log::info!("removing {target_bin_path}");
    fs::remove_file(&target_bin_path)?;

    s3_manager.delete_objects(&s3_bucket, None).await.unwrap();

    sleep(Duration::from_secs(2)).await;
    s3_manager.delete_bucket(&s3_bucket).await.unwrap();

    Ok(())
}
