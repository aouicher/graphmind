/// Versions that introduced breaking changes to the graph schema or edge kinds.
/// When updating from a version older than any of these, a `build --reset --all` is required.
pub const BREAKING_VERSIONS: &[&str] = &[
    "0.2.134", // edge kind classification (listens/emits/instantiates/spawns)
];

pub fn parse_version(v: &str) -> (u64, u64, u64) {
    let mut p = v.trim_start_matches('v').splitn(3, '.');
    let a = p.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let b = p.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let c = p.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (a, b, c)
}

/// Returns true if upgrading from `from` to `to` crosses a breaking version.
pub fn update_crosses_breaking(from: &str, to: &str) -> bool {
    let from_t = parse_version(from);
    let to_t = parse_version(to);
    BREAKING_VERSIONS.iter().any(|&b| {
        let bt = parse_version(b);
        from_t < bt && bt <= to_t
    })
}
