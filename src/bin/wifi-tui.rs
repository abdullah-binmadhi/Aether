//! Legacy binary alias for Aether.
//! Transparently delegates all command-line arguments to `aether`.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let current_exe = std::env::current_exe().ok();
    let aether_bin = current_exe
        .as_ref()
        .and_then(|p| p.parent())
        .map(|dir| dir.join("aether"))
        .filter(|p| p.exists())
        .unwrap_or_else(|| std::path::PathBuf::from("aether"));

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = std::process::Command::new(&aether_bin).args(&args).exec();
        eprintln!("[!] Failed to exec aether ({aether_bin:?}): {err}");
        std::process::exit(1);
    }
    #[cfg(not(unix))]
    {
        let status = std::process::Command::new(&aether_bin)
            .args(&args)
            .status()
            .expect("Failed to execute aether");
        std::process::exit(status.code().unwrap_or(1));
    }
}
