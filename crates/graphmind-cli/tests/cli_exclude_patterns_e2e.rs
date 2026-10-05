//! Exclude patterns and .gitignore files use gitignore semantics when building.

mod common;
use common::CliTestEnv;
use std::path::Path;

fn write(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn indexed_files(env: &CliTestEnv) -> String {
    // `export` lists every symbol with its file.
    env.run_ok(&["export", "pat", "--format", "json"])
}

#[test]
fn excludes_and_gitignore_follow_gitignore_rules() {
    let env = CliTestEnv::new();
    let p = env.home.path().join("proj");
    let f = "export function f(): number { return 1; }\n";
    // Kept
    write(&p, "src/app.ts", f);
    write(&p, "src/keep/important.ts", f);
    // Project excludes
    write(&p, "config/prod/secrets.ts", f); // `config/**/secrets.ts`
    write(&p, "deep/a/b/private/key.ts", f); // `**/private`
    write(&p, "vendor/lib.ts", f); // plain name, any depth
    write(&p, "pkg/vendor/lib2.ts", f);
    write(&p, "tools/demo/private/x.ts", f); // path with a slash, as before
    // Nested .gitignore, with a negation
    write(&p, "src/.gitignore", "generated/\n*.gen.ts\n!keep.gen.ts\n");
    write(&p, "src/generated/out.ts", f);
    write(&p, "src/api.gen.ts", f);
    write(&p, "src/keep.gen.ts", f);
    // Root .gitignore with a wildcard path
    write(&p, ".gitignore", "build/**/*.ts\n");
    write(&p, "build/x/y/z.ts", f);

    env.run_ok(&["register", p.to_str().unwrap(), "--slug", "pat"]);
    env.run_ok(&["exclude", "add", "config/**/secrets.ts", "**/private", "vendor", "--in", "pat"]);
    env.run_ok(&["build", "pat"]);
    let out = indexed_files(&env);

    for kept in ["src/app.ts", "src/keep/important.ts", "src/keep.gen.ts"] {
        assert!(out.contains(kept), "{kept} should be indexed:\n{out}");
    }
    for gone in [
        "config/prod/secrets.ts",
        "deep/a/b/private/key.ts",
        "vendor/lib.ts",
        "pkg/vendor/lib2.ts",
        "tools/demo/private/x.ts",
        "src/generated/out.ts",
        "src/api.gen.ts",
        "build/x/y/z.ts",
    ] {
        assert!(!out.contains(gone), "{gone} should be excluded:\n{out}");
    }
}

#[test]
fn adding_an_exclude_removes_already_indexed_files() {
    let env = CliTestEnv::new();
    let p = env.home.path().join("proj");
    write(&p, "src/app.ts", "export function app() {}\n");
    write(&p, "secrets/creds.ts", "export function token() { return 'x'; }\n");
    env.run_ok(&["register", p.to_str().unwrap(), "--slug", "pat"]);
    env.run_ok(&["build", "pat"]);
    assert!(indexed_files(&env).contains("secrets/creds.ts"));

    env.run_ok(&["exclude", "add", "secrets/**", "--in", "pat"]);
    env.run_ok(&["build", "pat"]);
    assert!(!indexed_files(&env).contains("secrets/creds.ts"));
}

#[test]
fn comma_joined_patterns_get_a_warning() {
    let env = CliTestEnv::new();
    let out = env.run(&["exclude", "add", "*.pem,*.key", "--global"]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("not a list"));
}

/// Ignore files above the project don't apply: a dotfiles setup with `*` in `~/.gitignore`
/// (a common way to keep a home-directory git repo clean) must not empty every project.
#[test]
fn ignore_files_above_the_project_are_not_applied() {
    let env = CliTestEnv::new();
    write(env.home.path(), ".gitignore", "*\n");
    write(env.home.path(), ".ignore", "*\n");
    let p = env.home.path().join("proj");
    write(&p, "src/app.ts", "export function app() {}\n");
    write(&p, ".gitignore", "dist/\n");
    write(&p, "dist/out.ts", "export function out() {}\n");

    env.run_ok(&["register", p.to_str().unwrap(), "--slug", "pat"]);
    env.run_ok(&["build", "pat"]);
    let out = indexed_files(&env);
    assert!(out.contains("src/app.ts"), "parent ignore files must not apply:\n{out}");
    assert!(!out.contains("dist/out.ts"), "the project's own .gitignore still applies:\n{out}");
}
