pub const DEFAULTS: &[(&str, &str, &str)] = &[
    ("pnpm", "pnpm-lock.yaml", include_str!("./pnpm.plz.toml")),
    ("npm", "package-lock.json", include_str!("./npm.plz.toml")),
    ("uv", "uv.lock", include_str!("./uv.plz.toml")),
    ("rust", "Cargo.toml", include_str!("./rust.plz.toml")),
];
