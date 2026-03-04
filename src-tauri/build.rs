fn main() {
    // Load .env files so option_env!() picks up secrets at compile time.
    // This lets `cargo tauri dev` work without manually exporting env vars.
    // Priority: env vars > .env.local > .env (most specific wins)
    for env_file in &[
        "../.env",       // project root .env
        ".env",          // src-tauri/.env
        "../.env.local", // project root .env.local (gitignored, highest file priority)
        ".env.local",    // src-tauri/.env.local
    ] {
        if let Ok(contents) = std::fs::read_to_string(env_file) {
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
            println!("cargo:rerun-if-changed={}", env_file);
        }
    }

    tauri_build::build()
}
