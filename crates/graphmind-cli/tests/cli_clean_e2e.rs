mod common;
use common::QueryTestEnv;

#[test]
fn clean_removes_graph() {
    let env = QueryTestEnv::new();

    let graph_db = env
        .home
        .path()
        .join(".graphmind")
        .join("graphs")
        .join(&env.slug)
        .join("graph.db");
    assert!(graph_db.exists(), "graph.db should exist before clean");

    env.run_ok(&["clean", &env.slug]);

    assert!(!graph_db.exists(), "graph.db should not exist after clean");
}

#[test]
fn clean_all_flag() {
    let env = QueryTestEnv::new();
    let out = env.run(&["clean", "--all"]);
    assert!(out.status.success(), "clean --all should exit 0, stderr: {}", String::from_utf8_lossy(&out.stderr));
}
