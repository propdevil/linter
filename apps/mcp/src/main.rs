use std::process::ExitCode;

use rmcp::ServiceExt;

mod server;

#[tokio::main]
async fn main() -> ExitCode {
    let result = async {
        server::Server::new(
            linter::register(linter::Registry::default())
                .and_then(linter_rust::register)
                .and_then(linter_c::register)
                .and_then(linter_markdown::register)?,
        )
        .serve(rmcp::transport::stdio())
        .await?
        .waiting()
        .await?;
        Ok::<_, Box<dyn std::error::Error>>(())
    }
    .await;
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("MCP server: {error}");
            ExitCode::FAILURE
        }
    }
}
