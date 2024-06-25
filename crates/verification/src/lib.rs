use anyhow::Result;
use axum::{extract, http::StatusCode, Json};
use cairo_lang_starknet_classes::contract_class::ContractClass;
use scarb_api::ScarbCommand;
use serde::Deserialize;
use starknet::core::types::{BlockId, BlockTag, ContractClass as CoreContractClass, FieldElement};
use starknet::providers::Provider;
use starknet_api::core::ChainId;
use std::fs;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::{collections::HashMap, fs::File};
use uuid::Uuid;
use walnut_shared::{create_rpc_client, extract_chain_id};

#[derive(Deserialize, Debug)]
pub struct VerificationPayload {
    pub contract_name: String,
    pub contract_address: String,
    pub source_code: HashMap<String, String>,
}

pub async fn verify_handler(
    chain_id: extract::Path<String>,
    Json(payload): Json<VerificationPayload>,
) -> (StatusCode, String) {
    let chain_id = extract_chain_id(chain_id.as_str());
    match verify_wrapper(chain_id, payload).await {
        Ok(_) => (StatusCode::OK, "Contract verified".to_string()),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()),
    }
}

async fn verify_wrapper(chain_id: ChainId, payload: VerificationPayload) -> Result<()> {
    let random_string = Uuid::new_v4().to_string();
    let mut tmp_dir = PathBuf::from("tmp/verification");
    tmp_dir.push(&random_string);

    let res = verify(chain_id, payload, &tmp_dir).await;

    fs::remove_dir_all(&tmp_dir)?;

    res
}

async fn verify(chain_id: ChainId, payload: VerificationPayload, tmp_dir: &PathBuf) -> Result<()> {
    create_files_from_map(payload.source_code, &tmp_dir)?;

    let scarb_config_file = tmp_dir.join("Scarb.toml");
    let manifest = read_manifest(&scarb_config_file)?;

    match manifest.starknet_version {
        (2, 6, _) => {
            let mut cmd = ScarbCommand::new_with_stdio();
            cmd.current_dir(&tmp_dir);
            let relative_path = PathBuf::from("scarb/scarb_v_2_6_3");
            let absolute_path = fs::canonicalize(&relative_path).unwrap();
            cmd.scarb_path(absolute_path);
            cmd.arg("build");
            cmd.run()?;
        }
        _ => {
            return Err(anyhow::anyhow!("Unsupported starknet version"));
        }
    };

    let contract_class_path = tmp_dir.join("target/dev").join(format!(
        "{}_{}.contract_class.json",
        manifest.package_name, payload.contract_name
    ));
    let contract_class_file = File::open(contract_class_path)?;
    let contract_class_reader = BufReader::new(contract_class_file);
    let contract_class: ContractClass = serde_json::from_reader(contract_class_reader)?;

    let provider_client = create_rpc_client(&chain_id);
    let class_from_blockchain = provider_client
        .get_class_at(
            BlockId::Tag(BlockTag::Latest),
            &FieldElement::from_str(payload.contract_address.as_str())?,
        )
        .await?;
    let program_from_blockchain = match class_from_blockchain {
        CoreContractClass::Sierra(flattened_sierra_class) => {
            Ok(flattened_sierra_class.sierra_program)
        }
        _ => Err(anyhow::anyhow!("Contract class is not a Sierra class")),
    }?;

    if contract_class.sierra_program.len() != program_from_blockchain.len() {
        return Err(anyhow::anyhow!(
            "Contract class programs lengths don't match"
        ));
    }

    for (e1, e2) in contract_class
        .sierra_program
        .iter()
        .zip(program_from_blockchain.iter())
    {
        if e1.value.to_string() != e2.to_string() {
            return Err(anyhow::anyhow!("Contract class does not match"));
        }
    }

    Ok(())
}

struct Manifest {
    package_name: String,
    starknet_version: (usize, usize, usize),
}

fn read_manifest(path: &Path) -> Result<Manifest> {
    let contents = fs::read_to_string(path)?;

    // Parse the string as TOML
    let toml = contents.parse::<toml::Value>()?;

    // Navigate to the "dependencies" table and get the "starknet" value
    let starknet_version_str = toml
        .get("dependencies")
        .and_then(|deps| deps.get("starknet"))
        .and_then(toml::Value::as_str);

    let starknet_version = match starknet_version_str {
        None => Err(anyhow::anyhow!("No starknet version found")),
        Some(version) => {
            let version_parts: Vec<usize> =
                version.split('.').map(|s| s.parse().unwrap_or(0)).collect();

            if version_parts.len() != 3 {
                Err(anyhow::anyhow!("Version should have 3 parts"))
            } else {
                Ok((version_parts[0], version_parts[1], version_parts[2]))
            }
        }
    }?;

    let package_name = toml
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(toml::Value::as_str)
        .ok_or(anyhow::anyhow!("No package name found"))?;

    Ok(Manifest {
        package_name: package_name.to_string(),
        starknet_version,
    })
}

pub fn create_files_from_map(
    source_code: HashMap<String, String>,
    dir_path: &PathBuf,
) -> Result<()> {
    for (path, content) in source_code {
        let mut full_path = dir_path.clone();
        full_path.push(path);

        if let Some(dir) = full_path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut file = File::create(&full_path)?;
        file.write_all(content.as_bytes())?;
    }
    Ok(())
}
