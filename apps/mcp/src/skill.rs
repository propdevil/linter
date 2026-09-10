use rmcp::{ErrorData, model::*};

pub(crate) const NAME: &str = "software-design";
pub(crate) const URI: &str = "linter://skills/software-design";
pub(crate) const TEXT: &str = include_str!("../../../skills/software-design/SKILL.md");
const DESCRIPTION: &str =
    "Design and validate software with explicit ownership and linter evidence.";

pub(crate) fn prompt() -> Prompt {
    Prompt::new(NAME, Some(DESCRIPTION), None)
}

pub(crate) fn get(request: GetPromptRequestParams) -> Result<GetPromptResult, ErrorData> {
    if request.name != NAME {
        return Err(ErrorData::invalid_params("unknown prompt", None));
    }
    if request
        .arguments
        .is_some_and(|arguments| !arguments.is_empty())
    {
        return Err(ErrorData::invalid_params(
            "software-design accepts no arguments",
            None,
        ));
    }
    Ok(
        GetPromptResult::new(vec![PromptMessage::new_text(Role::User, TEXT)])
            .with_description(DESCRIPTION),
    )
}

pub(crate) fn page(request: Option<PaginatedRequestParams>) -> Result<(), ErrorData> {
    if request.is_some_and(|request| request.cursor.is_some()) {
        return Err(ErrorData::invalid_params(
            "this catalog has no continuation cursor",
            None,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::{RoleClient, ServiceExt, service::RunningService};

    async fn connect() -> (RunningService<RoleClient, ()>, tokio::task::JoinHandle<()>) {
        let (server_transport, client_transport) = tokio::io::duplex(4096);
        let server = tokio::spawn(async move {
            crate::server::Server::new(linter::Registry::default())
                .serve(server_transport)
                .await
                .unwrap()
                .waiting()
                .await
                .unwrap();
        });
        (().serve(client_transport).await.unwrap(), server)
    }

    #[tokio::test]
    async fn protocol_advertises_skill_and_presets_and_returns_exact_embedded_content() {
        let (client, server) = connect().await;
        let info = client.peer_info().unwrap();
        assert!(info.capabilities.tools.is_some());
        assert!(info.capabilities.resources.is_some());
        assert!(info.capabilities.prompts.is_some());
        let instructions = info.instructions.as_ref().unwrap();
        assert!(instructions.contains(URI));
        assert!(instructions.contains("check"));
        let resources = client.list_all_resources().await.unwrap();
        assert_eq!(resources, crate::resource::list());
        assert_eq!(resources.len(), 4);
        for resource in resources {
            let expected = crate::resource::read(&resource.uri).unwrap();
            let result = client
                .read_resource(ReadResourceRequestParams::new(resource.uri))
                .await
                .unwrap();
            assert_eq!(result.contents, expected.contents);
        }
        let prompts = client.list_all_prompts().await.unwrap();
        assert_eq!(prompts, vec![prompt()]);
        let result = client
            .get_prompt(GetPromptRequestParams::new(NAME))
            .await
            .unwrap();
        assert_eq!(
            result.messages,
            vec![PromptMessage::new_text(Role::User, TEXT)]
        );
        client.cancel().await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn protocol_rejects_unknown_resources_prompts_arguments_and_cursors() {
        let (client, server) = connect().await;
        assert!(
            client
                .read_resource(ReadResourceRequestParams::new("file:///etc/passwd"))
                .await
                .is_err()
        );
        assert!(
            client
                .get_prompt(GetPromptRequestParams::new("unknown"))
                .await
                .is_err()
        );
        let arguments = serde_json::json!({"unexpected": "value"})
            .as_object()
            .unwrap()
            .clone();
        assert!(
            client
                .get_prompt(GetPromptRequestParams::new(NAME).with_arguments(arguments))
                .await
                .is_err()
        );
        let page = serde_json::from_value(serde_json::json!({"cursor": "unknown"})).unwrap();
        assert!(client.list_resources(Some(page)).await.is_err());
        let page = serde_json::from_value(serde_json::json!({"cursor": "unknown"})).unwrap();
        assert!(client.list_prompts(Some(page)).await.is_err());
        assert_eq!(client.list_all_tools().await.unwrap()[0].name, "check");
        client.cancel().await.unwrap();
        server.await.unwrap();
    }
}
