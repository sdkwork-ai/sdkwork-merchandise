use sdkwork_api_merchandise_assembly::assemble_api_router_from_env;
use sdkwork_iam_web_adapter::{
    build_web_framework_builder, iam_web_request_context_resolver_from_env,
};
use sdkwork_web_bootstrap::{infra_public_path_prefixes, ApiModuleRegistry, ComposedApiAssembly};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    tracing::info!("Starting SDKWork Merchandise API Server...");

    let assembly = match assemble_api_router_from_env().await {
        Ok(assembly) => assembly,
        Err(error) => {
            tracing::error!(%error, "merchandise assembly failed");
            std::process::exit(1);
        }
    };

    let framework = build_web_framework_builder(
        iam_web_request_context_resolver_from_env().await,
        assembly.route_manifest.clone(),
        infra_public_path_prefixes(),
    );
    let mut module_registry = ApiModuleRegistry::new();
    module_registry.add_modules(vec![assembly]);
    let app = module_registry
        .try_compose("SDKWork Merchandise API")
        .expect("merchandise API composition failed")
        .into_hosted(framework)
        .router
        .layer(sdkwork_web_bootstrap::application_cors_layer_from_env(
            &["SDKWORK_MERCHANDISE_ENVIRONMENT"],
            &["SDKWORK_CORS_ALLOWED_ORIGINS"],
        ));

    let addr = std::env::var("SDKWORK_MERCHANDISE_APPLICATION_PUBLIC_INGRESS_BIND")
        .unwrap_or_else(|_| "0.0.0.0:18090".to_owned());
    tracing::info!("Merchandise API server listening on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("bind merchandise server");
    axum::serve(listener, app)
        .await
        .expect("serve merchandise server");
}
