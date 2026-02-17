use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};

fn main() {
    println!("cargo:rerun-if-changed=static/");

    let mut hasher = DefaultHasher::new();

    let mut entries: Vec<_> = fs::read_dir("static")
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        if path.is_file() {
            let contents = fs::read(&path).unwrap();
            path.file_name().unwrap().to_str().unwrap().hash(&mut hasher);
            contents.hash(&mut hasher);
        }
    }

    let hash = format!("{:x}", hasher.finish());
    println!("cargo:rustc-env=STATIC_HASH={}", &hash[..8]);
}
