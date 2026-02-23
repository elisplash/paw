fn main() {
    // Load .env file so option_env!() picks up secrets at compile time.
    // This lets `cargo tauri dev` work without manually exporting env vars.
    if let Ok(contents) = std::fs::read_to_string(".env") {
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim();
                // Only set if not already in environment (env var takes precedence)
                if std::env::var(key).is_err() {
                    println!("cargo:rustc-env={}={}", key, value);
                }
            }
        }
        println!("cargo:rerun-if-changed=.env");
    }

    tauri_build::build()
}
