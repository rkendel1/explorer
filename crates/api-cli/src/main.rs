use api_analyzers::default_analyzers;
use api_compiler::{
    compile_contract, diff_contracts, to_openapi, to_request_collection, validate_contract,
};
use api_core::{ApiMetadata, ApiToolError, HttpMethod, SchemaRegistry};
use api_discovery::{DiscoveryEngine, build_context};
use clap::{Parser, Subcommand};
use std::{fs, net::SocketAddr, path::{Path, PathBuf}};

#[derive(Parser)]
#[command(name = "repo-api")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init {
        #[arg(default_value = ".")]
        repository: PathBuf,
    },
    Scan {
        repository: PathBuf,
    },
    Inspect {
        repository: PathBuf,
    },
    Export {
        #[command(subcommand)]
        command: ExportCommands,
    },
    Mock {
        #[arg(long)]
        contract: Option<PathBuf>,
        repository: Option<PathBuf>,
        #[arg(long, default_value_t = 4010)]
        port: u16,
        #[arg(long, default_value_t = 42)]
        seed: u64,
        #[arg(long, default_value_t = false)]
        stateful: bool,
    },
    Request {
        #[arg(long)]
        repository: PathBuf,
        #[arg(long)]
        endpoint: Option<String>,
        #[arg(long)]
        method: Option<String>,
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        body: Option<String>,
        #[arg(long, default_value = "mock")]
        environment: String,
    },
    Diff {
        #[arg(long)]
        before: PathBuf,
        #[arg(long)]
        after: PathBuf,
    },
}

#[derive(Subcommand)]
enum ExportCommands {
    Openapi {
        repository: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}

async fn scan_contract(repository: &Path) -> anyhow::Result<api_core::ApiContract> {
    api_storage::init_layout(repository)?;
    let context = build_context(repository.to_path_buf())?;

    let mut engine = DiscoveryEngine::default();
    for analyzer in default_analyzers() {
        engine.register(analyzer);
    }
    let discovery = engine
        .discover(context.clone())
        .await
        .map_err(|_| ApiToolError::AnalyzerFailed)?;

    let mut contract = compile_contract(
        ApiMetadata {
            title: format!("Repository API ({})", repository.display()),
            version: "1.0.0".into(),
            repository_root: Some(repository.display().to_string()),
        },
        discovery.endpoint_evidence,
        SchemaRegistry::default(),
        discovery.diagnostics,
        vec![],
    );

    let overrides = api_storage::load_overrides(repository)?;
    api_storage::apply_overrides(&mut contract, &overrides);

    validate_contract(&contract)?;
    api_storage::save_generated_contract(repository, &contract)?;
    api_storage::save_effective_contract(repository, &contract)?;
    let openapi = to_openapi(&contract);
    api_storage::save_openapi(repository, &openapi)?;
    let collection = to_request_collection(&contract);
    api_storage::save_collection(repository, &collection)?;

    Ok(contract)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init { repository } => {
            let base = api_storage::init_layout(&repository)?;
            println!("Initialized {}", base.display());
        }
        Commands::Scan { repository } => {
            let contract = scan_contract(&repository).await?;
            println!("Scanning repository...");
            println!(
                "Discovered:\n  {} endpoints\n  {} schemas",
                contract.endpoints.len(),
                contract.schemas.schemas.len()
            );
            println!(
                "Contract:\n  {}",
                repository
                    .join(".repo-api/contract/effective.json")
                    .display()
            );
        }
        Commands::Inspect { repository } => {
            let contract = scan_contract(&repository).await?;
            for ep in &contract.endpoints {
                println!("{} {}", ep.method.as_str().to_uppercase(), ep.path);
                println!(
                    "Confidence: {:?} — {:.0}%",
                    ep.confidence.level,
                    ep.confidence.score * 100.0
                );
                println!("Responses: {}", ep.responses.len());
                for ev in &ep.evidence {
                    println!("Evidence: {}:{}", ev.file, ev.line_start.unwrap_or(0));
                }
            }
        }
        Commands::Export { command } => match command {
            ExportCommands::Openapi { repository, output } => {
                let contract = scan_contract(&repository).await?;
                let openapi = to_openapi(&contract);
                if let Some(parent) = output.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&output, serde_yaml::to_string(&openapi)?)?;
                println!("OpenAPI written to {}", output.display());
            }
        },
        Commands::Mock {
            contract,
            repository,
            port,
            seed,
            stateful,
        } => {
            let loaded_contract = if let Some(c) = contract {
                serde_json::from_slice::<api_core::ApiContract>(&fs::read(c)?)?
            } else if let Some(repo) = repository {
                scan_contract(&repo).await?
            } else {
                anyhow::bail!("either --contract or repository path is required");
            };
            println!("Repository API Mock Runtime");
            println!(
                "Discovered:\n  {} endpoints\n  {} schemas",
                loaded_contract.endpoints.len(),
                loaded_contract.schemas.schemas.len()
            );
            println!("Server:\n  http://127.0.0.1:{port}");
            println!("OpenAPI:\n  /__api/openapi.json");
            println!("Contract:\n  /__api/contract.json");
            println!("Health:\n  /__api/health");
            api_mock_runtime::start_mock_server(
                loaded_contract,
                SocketAddr::from(([127, 0, 0, 1], port)),
                seed,
                vec![],
                stateful,
            )
            .await?;
        }
        Commands::Request {
            repository,
            endpoint,
            method,
            path,
            body,
            environment,
        } => {
            let envs = api_storage::load_environments(&repository)?;
            let env = envs
                .into_iter()
                .find(|e| e.name == environment)
                .ok_or(api_core::ApiToolError::EnvironmentNotFound)?;
            let body_json = body.and_then(|b| serde_json::from_str(&b).ok());
            if let Some(endpoint) = endpoint {
                let contract = api_storage::load_effective_contract(&repository)?;
                let resp = api_client::execute_endpoint(
                    &repository,
                    &contract,
                    &endpoint,
                    &env,
                    body_json,
                )
                .await?;
                println!("{}", serde_json::to_string_pretty(&resp)?);
            } else {
                let method = method.unwrap_or_else(|| "GET".into()).to_uppercase();
                let method = match method.as_str() {
                    "GET" => HttpMethod::GET,
                    "POST" => HttpMethod::POST,
                    "PUT" => HttpMethod::PUT,
                    "PATCH" => HttpMethod::PATCH,
                    "DELETE" => HttpMethod::DELETE,
                    "OPTIONS" => HttpMethod::OPTIONS,
                    "HEAD" => HttpMethod::HEAD,
                    _ => anyhow::bail!("unsupported method"),
                };
                let path = path.unwrap_or_else(|| "/".into());
                let resp =
                    api_client::execute_direct(&repository, method, &path, &env, body_json).await?;
                println!("{}", serde_json::to_string_pretty(&resp)?);
            }
        }
        Commands::Diff { before, after } => {
            let before_contract =
                serde_json::from_slice::<api_core::ApiContract>(&fs::read(before)?)?;
            let after_contract =
                serde_json::from_slice::<api_core::ApiContract>(&fs::read(after)?)?;
            for (key, kind) in diff_contracts(&before_contract, &after_contract) {
                println!("{key}: {kind:?}");
            }
        }
    }
    Ok(())
}
