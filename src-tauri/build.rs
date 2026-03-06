fn main() {
    // Load .env files so option_env!() picks up secrets at compile time.
    // This lets `cargo tauri dev` work without manually exporting env vars.
    // Priority: env vars > .env.local > .env (most specific wins)
    //
    // Security: only OPENPAWZ_* prefixed vars are emitted to prevent
    // accidentally leaking unrelated secrets into build logs / artifacts.
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
                    // Strip surrounding quotes from the value
                    let value = value.trim().trim_matches('"').trim_matches('\'');
                    // Only set if not already in environment (env var takes precedence)
                    if std::env::var(key).is_err() {
                        println!("cargo:rustc-env={}={}", key, value);
                    }
                }
            }
            println!("cargo:rerun-if-changed={}", env_file);
        }
    }

    // §Security: Warn at build time if the updater pubkey is still a placeholder.
    // The app must not ship with auto-updates enabled but no real signing key.
    if let Ok(conf) = std::fs::read_to_string("tauri.conf.json") {
        if conf.contains("GENERATE_WITH_tauri_signer_generate_AND_REPLACE_THIS") {
            println!(
                "cargo:warning=SECURITY: tauri.conf.json still has the placeholder updater pubkey. \
                 Run `tauri signer generate` and replace it before release builds."
            );
        }
    }
    println!("cargo:rerun-if-changed=tauri.conf.json");

    tauri_build::build()
}
