use std::process::ExitCode;

use rmcp::ServiceExt;

mod server;

#[tokio::main]
async fn main() -> ExitCode {
    let result = async {
        server::Server::new(
            linter::Registry::default()
                .register::<linter::Layout>()?
                .register::<linter::Filename>()?
                .register::<linter::SharedAffix>()?
                .register::<linter::ForbiddenWords>()?
                .register::<linter_rust::Layers>()?
                .register::<linter_rust::FileLength>()?,
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
