use tokio::process::Command;

pub async fn generate_flamegraph(dir: &str) -> anyhow::Result<String> {
    let mut cmd = Command::new("cargo");
    cmd.arg("flamegraph").current_dir(dir);
    let out = cmd.output().await?;
    
    if out.status.success() {
        Ok("flamegraph.svg generated".to_string())
    } else {
        Ok("Failed to generate flamegraph".to_string())
    }
}
