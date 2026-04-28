use criterion::{criterion_group, criterion_main, Criterion};
use graphmind_db::builder::{BuildOptions, GraphBuilder};
use graphmind_db::queries::GraphQueries;
use std::path::Path;

fn fixture_project() -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures/sample-project")
        .to_string_lossy()
        .to_string()
}

fn build_fixture() -> (GraphBuilder, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("bench.db");
    let cache_dir = tmp.path().join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();

    let mut builder = GraphBuilder::new(
        db_path.to_str().unwrap(),
        cache_dir.to_str().unwrap(),
    );
    builder.build(
        &fixture_project(),
        &BuildOptions { full: true, ..Default::default() },
    );
    (builder, tmp)
}

fn bench_build(c: &mut Criterion) {
    c.bench_function("build_fixture_project", |b| {
        b.iter(|| {
            let tmp = tempfile::tempdir().unwrap();
            let db_path = tmp.path().join("bench.db");
            let cache_dir = tmp.path().join("cache");
            std::fs::create_dir_all(&cache_dir).unwrap();

            let mut builder = GraphBuilder::new(
                db_path.to_str().unwrap(),
                cache_dir.to_str().unwrap(),
            );
            builder.build(
                &fixture_project(),
                &BuildOptions { full: true, ..Default::default() },
            );
        });
    });
}

fn bench_queries(c: &mut Criterion) {
    let (builder, _tmp) = build_fixture();
    let db = builder.database();

    c.bench_function("find_symbol", |b| {
        b.iter(|| {
            let q = GraphQueries::new(db);
            q.find_symbol("createWallet");
        });
    });

    c.bench_function("fts_search", |b| {
        b.iter(|| {
            let q = GraphQueries::new(db);
            q.search_symbols("wallet*", 20);
        });
    });

    c.bench_function("callers", |b| {
        b.iter(|| {
            let q = GraphQueries::new(db);
            q.callers("createWallet");
        });
    });

    c.bench_function("callees", |b| {
        b.iter(|| {
            let q = GraphQueries::new(db);
            q.callees("createWallet");
        });
    });

    c.bench_function("file_deps", |b| {
        b.iter(|| {
            let q = GraphQueries::new(db);
            q.file_deps("src/routes/wallet.ts");
        });
    });

    c.bench_function("impact_depth_5", |b| {
        b.iter(|| {
            let q = GraphQueries::new(db);
            q.impact("src/utils/validator.ts", 5);
        });
    });

    c.bench_function("top_connected", |b| {
        b.iter(|| {
            let q = GraphQueries::new(db);
            q.top_connected(20);
        });
    });

    c.bench_function("detect_cycles", |b| {
        b.iter(|| {
            let q = GraphQueries::new(db);
            q.detect_cycles();
        });
    });

    c.bench_function("stats", |b| {
        b.iter(|| {
            let q = GraphQueries::new(db);
            q.stats();
        });
    });
}

criterion_group!(benches, bench_build, bench_queries);
criterion_main!(benches);
