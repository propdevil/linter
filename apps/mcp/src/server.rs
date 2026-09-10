use std::{path::PathBuf, sync::Arc};

use rmcp::{
    handler::server::wrapper::Parameters,
    model::CallToolResult,
    schemars::{self, JsonSchema},
    tool, tool_router,
};
use serde::Deserialize;
use tokio::sync::Semaphore;

#[derive(Clone)]
pub(crate) struct Server {
    slots: Arc<Semaphore>,
    registry: Arc<linter::Registry>,
}

impl Server {
    pub(crate) fn new(registry: linter::Registry) -> Self {
        Self {
            slots: Arc::new(Semaphore::new(1)),
            registry: Arc::new(registry),
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Check {
    /// Repository directory containing linter.toml.
    root: PathBuf,
}

#[tool_router(server_handler)]
impl Server {
    #[tool(
        description = "Validate saved repository files against linter.toml. Returns registered rule findings with repair instructions; does not edit files. An unconfigured layout status means no layout was checked.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn check(&self, Parameters(input): Parameters<Check>) -> CallToolResult {
        let Ok(permit) = self.slots.clone().try_acquire_owned() else {
            return CallToolResult::structured_error(serde_json::json!({
                "error": "a validation is already running",
                "instruction": "Retry after the current validation finishes."
            }));
        };
        let registry = self.registry.clone();
        let result = tokio::task::spawn_blocking(move || {
            // A cancelled request must not release capacity while its worker still runs.
            let _permit = permit;
            registry.check(&input.root)
        })
        .await;
        match result {
            Ok(Ok(report)) => match serde_json::to_value(report) {
                Ok(value) => CallToolResult::structured(value),
                Err(error) => CallToolResult::structured_error(
                    serde_json::json!({"error": error.to_string()}),
                ),
            },
            Ok(Err(error)) => CallToolResult::structured_error(serde_json::json!({
                "error": error.to_string(),
                "instruction": "Correct the repository path or configuration and run check again."
            })),
            Err(error) => CallToolResult::structured_error(
                serde_json::json!({"error": format!("validation worker failed: {error}")}),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use rmcp::{ServiceExt, model::CallToolRequestParams};
    use serde_json::json;

    use crate::server::Server;

    #[tokio::test]
    async fn protocol_returns_the_library_report_and_configuration_errors() {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("linter.toml"),
            r#"
[[rules.layout]]
target = "rules/*"
files.required = ["tests.rs"]
[[rules.layout]]
target = ["**/*.md"]
allow = false
"#,
        )
        .unwrap();
        fs::create_dir_all(root.path().join("rules/layout")).unwrap();
        fs::write(root.path().join("unapproved.md"), "").unwrap();

        let (server_transport, client_transport) = tokio::io::duplex(4096);
        let server = tokio::spawn(async move {
            Server::new(
                linter::Registry::default()
                    .register::<linter::Layout>()
                    .unwrap()
                    .register::<linter::Filename>()
                    .unwrap()
                    .register::<linter::SharedAffix>()
                    .unwrap()
                    .register::<linter::ForbiddenWords>()
                    .unwrap()
                    .register::<linter_rust::Layers>()
                    .unwrap()
                    .register::<linter_rust::FileLength>()
                    .unwrap(),
            )
            .serve(server_transport)
            .await
            .unwrap()
            .waiting()
            .await
            .unwrap();
        });
        let client = ().serve(client_transport).await.unwrap();
        let tools = client.list_all_tools().await.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "check");
        let request = CallToolRequestParams::new("check")
            .with_arguments(json!({"root": root.path()}).as_object().unwrap().clone());
        let result = client.call_tool(request.clone()).await.unwrap();
        assert_ne!(result.is_error, Some(true));
        assert_eq!(
            result.structured_content,
            Some(
                serde_json::to_value(
                    linter::Registry::default()
                        .register::<linter::Layout>()
                        .unwrap()
                        .register::<linter::Filename>()
                        .unwrap()
                        .register::<linter::SharedAffix>()
                        .unwrap()
                        .register::<linter::ForbiddenWords>()
                        .unwrap()
                        .register::<linter_rust::Layers>()
                        .unwrap()
                        .register::<linter_rust::FileLength>()
                        .unwrap()
                        .check(root.path())
                        .unwrap()
                )
                .unwrap()
            )
        );
        assert!(!root.path().join("rules/layout/tests.rs").exists());
        let report = result.structured_content.unwrap();
        assert_eq!(report["findings"].as_array().unwrap().len(), 2);
        assert_eq!(report["findings"][1]["rule"], "layout");

        fs::write(root.path().join("linter.toml"), "[rules.typo]").unwrap();
        let result = client.call_tool(request).await.unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(
            result.structured_content.unwrap()["error"]
                .as_str()
                .unwrap()
                .contains("unknown rule")
        );
        client.cancel().await.unwrap();
        server.await.unwrap();
    }
}
