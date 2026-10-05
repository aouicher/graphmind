//! Incremental builds keep call edges from unchanged files into the files that changed.

mod common;
use common::CliTestEnv;
use std::path::{Path, PathBuf};

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let to = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), to).unwrap();
        }
    }
}

fn setup(env: &CliTestEnv) -> PathBuf {
    let project = env.home.path().join("project");
    copy_dir(&env.fixture_path(), &project);
    env.run_ok(&["register", project.to_str().unwrap(), "--slug", "inc"]);
    env.run_ok(&["build", "inc"]);
    project
}

fn callers_of_get_balance(env: &CliTestEnv) -> String {
    let out = env.run_ok(&["fn", "getBalance", "--in", "inc", "--file", "src/services/wallet.ts"]);
    out.lines().skip_while(|l| !l.contains("Callers")).take(3).collect::<Vec<_>>().join("\n")
}

#[test]
fn editing_a_file_keeps_its_callers_from_unchanged_files() {
    let env = CliTestEnv::new();
    let project = setup(&env);
    assert!(callers_of_get_balance(&env).contains("handleGetBalance"));

    // Edit the callee's file only: the caller (src/routes/wallet.ts) is not re-parsed.
    let file = project.join("src/services/wallet.ts");
    let src = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, format!("// edited\n{src}")).unwrap();
    env.run_ok(&["build", "inc"]);

    let callers = callers_of_get_balance(&env);
    assert!(callers.contains("handleGetBalance"), "caller lost after incremental build:\n{callers}");
}

#[test]
fn removing_a_symbol_removes_edges_to_it() {
    let env = CliTestEnv::new();
    let project = setup(&env);

    let file = project.join("src/services/wallet.ts");
    let src = std::fs::read_to_string(&file).unwrap();
    let renamed = src.replace("getBalance(", "getBalanceV2(");
    assert_ne!(src, renamed);
    std::fs::write(&file, renamed).unwrap();
    env.run_ok(&["build", "inc"]);

    let out = env.run(&["fn", "getBalance", "--in", "inc", "--file", "src/services/wallet.ts"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(!text.contains("handleGetBalance"), "edge to a removed symbol survived:\n{text}");
    // The new symbol exists; its caller links up once the caller file changes too.
    let out = env.run_ok(&["fn", "getBalanceV2", "--in", "inc"]);
    assert!(out.contains("getBalanceV2"), "{out}");
}
