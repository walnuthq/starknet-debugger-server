use async_compression::tokio::bufread::GzipDecoder;
use async_tar::Archive;
use futures::StreamExt;
use reqwest::Client;
use semver::Version;
use serde::Deserialize;
use std::io::Read;
use std::path::Path;
use tokio::fs as tokio_fs;
use tokio::fs::File;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio_util::compat::TokioAsyncReadCompatExt;
use regex::Regex;
use tokio::process::Command;

// Struct to deserialize GitHub API release response
#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    browser_download_url: String,
    name: String,
}

async fn get_all_releases(repo: &str) -> Result<Vec<Release>, Box<dyn std::error::Error>> {
    let url = format!("https://api.github.com/repos/{}/releases", repo);
    let client = Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", "rust-app")
        .send()
        .await?;
    let releases: Vec<Release> = response.json().await?;
    Ok(releases)
}

pub async fn download_file(url: &str, output_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let response = Client::new()
        .get(url)
        .header("User-Agent", "rust-app")
        .send()
        .await?
        .error_for_status()?;

    let mut file = File::create(output_path).await?;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
    }

    Ok(())
}

pub async fn extract_tar_gz(archive_path: &Path, output_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open(archive_path).await?;
    let buf_reader = BufReader::new(file);

    let gzip_decoder = GzipDecoder::new(buf_reader);
    let compat_reader = gzip_decoder.compat();
    let mut archive = Archive::new(compat_reader);

    archive.unpack(output_dir).await?;
    Ok(())
}

// call scarb and extract the cairo_version
async fn extract_cairo_version(scarb_path: &str) -> Result<Version, Box<dyn std::error::Error>> {
    let output = Command::new(scarb_path)
        .arg("--version")
        .output()
        .await?;

    let scarb_output = String::from_utf8(output.stdout)?;

    let regex = Regex::new(r"cairo: ([\d\.]+)").unwrap();
    let version_str = regex.captures(&scarb_output)
        .and_then(|caps| caps.get(1))
        .ok_or_else(|| "Failed to find cairo version in scarb output")?
        .as_str();

    let version = Version::parse(version_str)?;

    Ok(version)
}

// Main function to check for updates and download if available
pub async fn check_periodically_scarb_updates() -> Result<(), Box<dyn std::error::Error>> {
    let repo = std::env::var("SCARB_GITHUB_REPO_NAME")
        .expect("Environment variable SCARB_GITHUB_REPO_NAME is not set");
    let file_path = std::env::var("SCARB_LATEST_VERSION_FILE_NAME")
        .expect("Environment variable SCARB_LATEST_VERSION_FILE_NAME is not set");
    // We support here every version above that
    let latest_unsupported_scarb_tag = "2.8.5";
    let mut latest_installed_scarb_tag = Version::parse(latest_unsupported_scarb_tag).unwrap();

    // Check if the version file exists
    if !Path::new(&file_path).exists() {
        println!("Scarb last tag file does not exist. Creating it...");
        File::create(&file_path).await?.write_all(latest_unsupported_scarb_tag.as_bytes()).await?;
    } else {
        let content = tokio_fs::read_to_string(&file_path).await?;
        let content_parsed = content.trim().replace("v", "");
        latest_installed_scarb_tag = Version::parse(content_parsed.as_str())
            .expect("Invalid version in SCARB_LATEST_VERSION_FILE_NAME");
    }

    let mut releases: Vec<(Version, Release)> = get_all_releases(&repo).await?
        .into_iter()
        .filter_map(|release| {
            let clean_version = release.tag_name.trim_start_matches('v');
            Version::parse(clean_version).ok().map(|ver| (ver, release))
        })
        .filter(|(version, _)| *version > latest_installed_scarb_tag)
        .collect();
    // Sort - lowest versions first
    releases.sort_by(|a, b| a.0.cmp(&b.0));

    for (_, release) in releases {
        if let Some(x86_64_asset) = release
            .assets
            .iter()
            .find(|asset| asset.name.ends_with("x86_64-unknown-linux-gnu.tar.gz"))
        {
            let binaries_dir_path_string = std::env::var("BINARIES_SAVE_DIRECTORY_PATH")
                .unwrap_or_else(|_| ".".to_string());
            let output_path_string = format!("{}/{}", binaries_dir_path_string, x86_64_asset.name);
            let tar_gz_output_path = Path::new(&output_path_string);
            println!("Downloading {}", x86_64_asset.browser_download_url);
            download_file(&x86_64_asset.browser_download_url, tar_gz_output_path).await?;
            println!("Downloaded to: {:?}", tar_gz_output_path);

            if x86_64_asset.name.ends_with(".tar.gz") {
                let binaries_dir_path = Path::new(&binaries_dir_path_string);
                println!("Extracting to: {:?}", binaries_dir_path);
                extract_tar_gz(tar_gz_output_path, binaries_dir_path).await?;
                let extracted_tar_gz_folder_path = output_path_string.trim_end_matches(".tar.gz");
                let extracted_binary_path = format!("{}/bin/scarb", extracted_tar_gz_folder_path);
                let cairo_version = extract_cairo_version(extracted_binary_path.as_str()).await?;
                let scarb_tag_name = format!("v{}", cairo_version);
                let extracted_binary_destination_path = format!(
                    "{}/scarb/scarb_cairo_{}",
                    binaries_dir_path_string, scarb_tag_name
                );

                // Move the binary to the destination directory (e.g. binaries/scarb)
                tokio_fs::rename(&extracted_binary_path, &extracted_binary_destination_path).await?;
                // Remove the extracted tar.gz folder
                tokio::fs::remove_dir_all(extracted_tar_gz_folder_path).await?;
                // Save the latest downloaded Scarb version to a file
                // NOTE! We save here the Scarb version, not the tag name (cairo version)
                tokio_fs::write(&file_path, release.tag_name.as_bytes()).await?;
                println!(
                    "Extracted successfully: {}",
                    &extracted_binary_destination_path
                );
            }
            // Remove the tar.gz file
            tokio::fs::remove_file(tar_gz_output_path).await?;

        } else {
            println!("No compatible assets found in the release.");
        }
    }

    Ok(())
}