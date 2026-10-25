// ABOUTME: Starts pkglyctl and reports terminal or MCP startup failures.
// ABOUTME: Routes command output through the selected CLI or protocol transport.
#[tokio::main]
async fn main() {
    match pkgly_cli::run_from_args().await {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
    }
}
