use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=.env");
    println!("cargo:rerun-if-env-changed=SUPABASE_URL");
    println!("cargo:rerun-if-env-changed=SUPABASE_ANON_KEY");

    let manifest_dir = env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_default();
    let env_file = fs::read_to_string(manifest_dir.join(".env")).unwrap_or_default();
    for key in ["SUPABASE_URL", "SUPABASE_ANON_KEY"] {
        let from_process = env::var(key).ok().filter(|value| !value.trim().is_empty());
        let from_file = env_file.lines().find_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (name, value) = line.split_once('=')?;
            if name.trim() != key {
                return None;
            }
            let value = value.trim().trim_matches('"').trim_matches('\'');
            (!value.is_empty()).then(|| value.to_string())
        });

        if let Some(value) = from_process.or(from_file) {
            println!("cargo:rustc-env={key}={value}");
        }
    }
}
