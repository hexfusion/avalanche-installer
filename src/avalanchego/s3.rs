use std::{
    fs::{self, File},
    io::{self, Error, ErrorKind},
    os::unix::fs::PermissionsExt,
    path::Path,
};

use aws_manager::{self, s3};
use tokio::time::{sleep, Duration};

pub async fn download_avalanche_and_plugins(
    overwrite: bool,
    s3_manager: &s3::Manager,
    s3_bucket: &str,
    source_avalanchego_bin_s3_path: &str,
    target_avalanchego_bin_path: &str,
    source_plugin_dir_s3_prefix: Option<&str>,
    target_plugin_dir: Option<&str>,
) -> io::Result<()> {
    log::info!("downloading avalanchego and plugins in bucket {s3_bucket} (overwrite {overwrite})");
    let mut need_download = !Path::new(target_avalanchego_bin_path).exists();
    if overwrite {
        need_download = true;
    }
    if need_download {
        let tmp_path = random_manager::tmp_path(15, None)?;

        let mut success = false;
        for round in 0..20 {
            log::info!("[ROUND {round}] get_object for {source_avalanchego_bin_s3_path}");

            let res = s3_manager
                .get_object(s3_bucket, source_avalanchego_bin_s3_path, &tmp_path)
                .await;

            if res.is_ok() {
                success = true;
                break;
            }

            let err = res.err().unwrap();
            if err.is_retryable() {
                log::warn!("get_object retriable error: {}", err);
                sleep(Duration::from_secs((round + 1) * 5)).await;
                continue;
            }

            return Err(Error::new(
                ErrorKind::Other,
                format!("get_object failed for non-retriable error {}", err),
            ));
        }
        if !success {
            return Err(Error::new(
                ErrorKind::Other,
                "get_object failed to download with retries",
            ));
        }

        log::info!("successfully downloaded to {tmp_path}");
        {
            let f = File::open(&tmp_path)?;
            f.set_permissions(PermissionsExt::from_mode(0o777))?;
        }
        log::info!("copying {tmp_path} to {target_avalanchego_bin_path}");
        fs::copy(&tmp_path, &target_avalanchego_bin_path)?;
        fs::remove_file(&tmp_path)?;
    } else {
        log::info!("skipping avalanchego downloads")
    }

    if let Some(plugin_dir_s3_prefix) = source_plugin_dir_s3_prefix {
        let target_plugin_dir = target_plugin_dir.unwrap();
        let (mut success, mut objects) = (false, Vec::new());
        for round in 0..20 {
            log::info!("[ROUND {round}] list_objects for {plugin_dir_s3_prefix}");

            let res = s3_manager
                .list_objects(s3_bucket, Some(plugin_dir_s3_prefix))
                .await;

            if res.is_ok() {
                success = true;
                objects = res.unwrap();
                break;
            }

            let err = res.err().unwrap();
            if err.is_retryable() {
                log::warn!("list_objects retriable error: {}", err);
                sleep(Duration::from_secs((round + 1) * 5)).await;
                continue;
            }

            return Err(Error::new(
                ErrorKind::Other,
                format!("list_objects failed for non-retriable error {}", err),
            ));
        }
        if !success {
            return Err(Error::new(
                ErrorKind::Other,
                "list_objects failed to download with retries",
            ));
        }

        log::info!(
            "listed {} plugin objects in {plugin_dir_s3_prefix}",
            objects.len()
        );
        if !Path::new(target_plugin_dir).exists() {
            log::info!("creating '{target_plugin_dir}' for plugin");
            fs::create_dir_all(target_plugin_dir.clone())?;
        } else {
            log::info!("plugin-dir {target_plugin_dir} already exists -- skipping create_dir_all");
        }

        for obj in objects.iter() {
            let s3_key = obj.key().expect("unexpected None s3 object").to_string();
            let s3_file_name = extract_filename(&s3_key);
            if s3_file_name.ends_with("plugin") || s3_file_name.ends_with("plugin/") {
                log::info!("s3 file name is '{}' directory, so skip", s3_file_name);
                continue;
            }

            let target_plugin_bin_file_path = format!("{}/{}", target_plugin_dir, s3_file_name);
            if Path::new(&target_plugin_bin_file_path).exists() {
                if !overwrite {
                    log::info!("{target_plugin_bin_file_path} already exists -- skipping...");
                    continue;
                }
                log::info!("{target_plugin_bin_file_path} already exists but overwriting...");
            }

            log::info!(
                "downloading plugin {} to {}",
                s3_key,
                target_plugin_bin_file_path
            );
            let tmp_path = random_manager::tmp_path(15, None)?;

            let mut success = false;
            for round in 0..20 {
                log::info!("[ROUND {round}] get_object for {s3_key}");

                let res = s3_manager.get_object(s3_bucket, &s3_key, &tmp_path).await;

                if res.is_ok() {
                    success = true;
                    break;
                }

                let err = res.err().unwrap();
                if err.is_retryable() {
                    log::warn!("get_object retriable error: {}", err);
                    sleep(Duration::from_secs((round + 1) * 5)).await;
                    continue;
                }

                return Err(Error::new(
                    ErrorKind::Other,
                    format!("get_object failed for non-retriable error {}", err),
                ));
            }
            if !success {
                return Err(Error::new(
                    ErrorKind::Other,
                    "get_object failed to download with retries",
                ));
            }

            log::info!("successfully downloaded to {tmp_path}");
            {
                let f = File::open(&tmp_path)?;
                f.set_permissions(PermissionsExt::from_mode(0o777))?;
            }
            fs::copy(&tmp_path, &target_plugin_bin_file_path)?;
            fs::remove_file(&tmp_path)?;
        }
    }

    Ok(())
}

/// returns "hello" from "a/b/c/hello.zstd"
fn extract_filename(p: &str) -> String {
    let path = Path::new(p);
    let file_stemp = path.file_stem().unwrap();
    String::from(file_stemp.to_str().unwrap())
}
