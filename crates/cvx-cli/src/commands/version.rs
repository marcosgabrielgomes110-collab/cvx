/// Executes `cvx version`.
pub fn run() {
    println!("cvx {}", env!("CARGO_PKG_VERSION"));
    println!("repository formats: cvx (.cvx, Git-compatible), git (.git)");
    println!("object format: sha1");
    if let Some(commit) = option_env!("CVX_BUILD_COMMIT") {
        println!("commit: {commit}");
    }
}
