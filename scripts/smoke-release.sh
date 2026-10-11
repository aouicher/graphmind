#!/usr/bin/env bash
# Smoke test of a published release, run against the real release assets in throwaway HOMEs:
# what's published, a fresh install, the upgrade from the previous release (clean and with
# damaged config), `graphmind update`, the user-facing behaviours, the MCP server and the
# desktop app's disk image. Your own ~/.claude, ~/.graphmind and shell profiles are never touched.
#
# Usage: scripts/smoke-release.sh [VERSION] [PREVIOUS]
#   VERSION   tag to test (default: the latest release)
#   PREVIOUS  tag to upgrade from (default: the release before VERSION)
# Env: KEEP=1 keeps the sandbox for inspection.
# Needs: gh (authenticated), git, jq, curl. Docker (running) adds a Linux run.
set -uo pipefail

REPO="aouicher/graphmind"
TAP_REPO="aouicher/homebrew-graphmind"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
INSTALL_SH="$REPO_ROOT/scripts/install.sh"

# --- reporting ---------------------------------------------------------------------------------

PASS=0 FAIL=0 WARN=0 SKIP=0
FAILURES=() WARNINGS=()
section() { printf '\n\033[1m== %s\033[0m\n' "$*"; }
ok()   { PASS=$((PASS + 1)); printf '  \033[32m✓\033[0m %s\n' "$1"; }
ko()   { FAIL=$((FAIL + 1)); FAILURES+=("$1"); printf '  \033[31m✗ %s\033[0m\n' "$1"; [ -n "${2:-}" ] && printf '%s\n' "$2" | head -12 | sed 's/^/      /'; return 0; }
warn() { WARN=$((WARN + 1)); WARNINGS+=("$1"); printf '  \033[33m! %s\033[0m\n' "$1"; [ -n "${2:-}" ] && printf '%s\n' "$2" | head -6 | sed 's/^/      /'; return 0; }
skip() { SKIP=$((SKIP + 1)); printf '  \033[90m- %s (skipped: %s)\033[0m\n' "$1" "$2"; }
die()  { printf '\033[31merror:\033[0m %s\n' "$*" >&2; exit 2; }

# check NAME CMD...      passes when CMD exits 0
# refuse NAME CMD...     passes when CMD exits non-zero
check()  { local name=$1 out; shift; if out=$("$@" 2>&1); then ok "$name"; else ko "$name" "$out"; fi; }
refuse() { local name=$1 out; shift; if out=$("$@" 2>&1); then ko "$name" "exit 0, expected a failure:${out:+$'\n'$out}"; else ok "$name"; fi; }
# expect NAME "TEXT" OUTPUT    passes when OUTPUT contains TEXT
expect() { if grep -qF -- "$2" <<<"$3"; then ok "$1"; else ko "$1" "expected \"$2\" in:"$'\n'"$3"; fi; }
lacks()  { if grep -qF -- "$2" <<<"$3"; then ko "$1" "unexpected \"$2\" in:"$'\n'"$3"; else ok "$1"; fi; }
# with_timeout SECS CMD...
with_timeout() { perl -e 'alarm shift; exec @ARGV or exit 127' "$@"; }
# A missing file has no fingerprint, so comparing two of them never passes.
fingerprint() { if [ -r "$1" ]; then shasum "$1" | cut -d' ' -f1; elif [ -e "$1" ]; then stat -f '%m %z %p' "$1"; else echo "missing $RANDOM"; fi; }

# --- prerequisites and versions (real HOME: gh needs its login) -------------------------------

for tool in gh git jq curl perl shasum; do command -v "$tool" >/dev/null || die "$tool is required"; done
gh auth status >/dev/null 2>&1 || die "gh is not authenticated (gh auth login)"

VERSION="${1:-$(gh release view --repo "$REPO" --json tagName -q .tagName)}"
if [ -n "${2:-}" ]; then
    PREVIOUS="$2"
else
    PREVIOUS="$(gh release list --repo "$REPO" --limit 30 --json tagName -q '.[].tagName' | grep -A1 -xF "$VERSION" | sed -n 2p)"
fi
[ -n "$VERSION" ] || die "could not determine the release to test"
[ -n "$PREVIOUS" ] || die "could not determine the previous release (pass it as the second argument)"
BARE="${VERSION#v}"

SANDBOX="$(mktemp -d "${TMPDIR:-/tmp}/gm-smoke.XXXXXX")"
SANDBOX="$(cd "$SANDBOX" && pwd -P)"
cleanup() {
    chmod -R u+rwx "$SANDBOX" 2>/dev/null
    if [ -n "${KEEP:-}" ]; then echo "Sandbox kept: $SANDBOX"; else rm -rf "$SANDBOX"; fi
}
trap cleanup EXIT

# Temp files too: graphmind update downloads to a fixed name in the temp folder.
mkdir -p "$SANDBOX/tmp" && export TMPDIR="$SANDBOX/tmp"
case "$(uname -m)" in arm64|aarch64) MAC_ARCH=arm64 ;; *) MAC_ARCH=x64 ;; esac
DL="$SANDBOX/dl"; mkdir -p "$DL"

printf 'graphmind smoke test: %s (upgrade from %s)\nSandbox: %s\n' "$VERSION" "$PREVIOUS" "$SANDBOX"

section "1. Published release ($VERSION)"

ASSETS="$(gh release view "$VERSION" --repo "$REPO" --json assets -q '.assets[].name')"
for asset in graphmind-cli-linux-x64 graphmind-cli-macos-arm64 graphmind-cli-macos-x64 graphmind-cli-windows-x64.exe \
    GraphMind-macos-arm64.dmg GraphMind-macos-x64.dmg \
    GraphMind-macos-aarch64.app.tar.gz GraphMind-macos-aarch64.app.tar.gz.sig \
    GraphMind-macos-x86_64.app.tar.gz GraphMind-macos-x86_64.app.tar.gz.sig latest.json; do
    if grep -qxF "$asset" <<<"$ASSETS"; then ok "asset $asset"; else ko "asset $asset missing from the release"; fi
done

gh release download "$VERSION" --repo "$REPO" -D "$DL" \
    -p latest.json -p graphmind-cli-linux-x64 -p graphmind-cli-macos-arm64 -p graphmind-cli-macos-x64 \
    -p graphmind-cli-windows-x64.exe -p "GraphMind-macos-$MAC_ARCH.dmg" >/dev/null 2>&1 \
    || ko "download of the release assets failed"

if [ -f "$DL/latest.json" ]; then
    expect "updater latest.json announces $BARE" "\"$BARE\"" "$(jq -r .version "$DL/latest.json" | sed 's/^/"/;s/$/"/')"
    for platform in darwin-aarch64 darwin-x86_64; do
        url="$(jq -r --arg p "$platform" '.platforms[$p].url // empty' "$DL/latest.json")"
        sig="$(jq -r --arg p "$platform" '.platforms[$p].signature // empty' "$DL/latest.json")"
        if [ -n "$url" ] && [ -n "$sig" ] && grep -qF "$VERSION" <<<"$url"; then ok "updater entry $platform points at $VERSION, signed"
        else ko "updater entry $platform is missing, unsigned or points elsewhere" "$url"; fi
    done
fi

for pair in "graphmind-cli-macos-arm64:arm64" "graphmind-cli-macos-x64:x86_64" \
    "graphmind-cli-linux-x64:ELF 64-bit.*x86-64" "graphmind-cli-windows-x64.exe:PE32\+ executable.*x86-64"; do
    file="${pair%%:*}" pattern="${pair#*:}"
    [ -f "$DL/$file" ] || continue
    if file "$DL/$file" | grep -qE "$pattern"; then ok "$file is a $(file -b "$DL/$file" | cut -c1-40)…"
    else ko "$file has the wrong format" "$(file -b "$DL/$file")"; fi
done

TAP_FILES="$(gh api "repos/$TAP_REPO/git/trees/HEAD?recursive=1" -q '.tree[].path | select(endswith(".rb"))' 2>/dev/null)"
if [ -z "$TAP_FILES" ]; then
    ko "Homebrew tap $TAP_REPO could not be read"
else
    for rb in $TAP_FILES; do
        content="$(gh api "repos/$TAP_REPO/contents/$rb" -H 'Accept: application/vnd.github.raw' 2>/dev/null)"
        expect "Homebrew $rb is at $BARE" "$BARE" "$(grep -E '^\s*version' <<<"$content")"
    done
fi

section "2. Linux binary"
if command -v docker >/dev/null && docker info >/dev/null 2>&1; then
    chmod +x "$DL/graphmind-cli-linux-x64"
    out="$(docker run --rm --platform linux/amd64 -v "$DL:/dl:ro" debian:stable-slim sh -c \
        'cp /dl/graphmind-cli-linux-x64 /usr/local/bin/graphmind && graphmind --version && mkdir -p /p/src && cd /p \
         && printf "pub fn a() { b() }\npub fn b() {}\n" > src/lib.rs && graphmind init --skip-build >/dev/null 2>&1; graphmind build >/dev/null 2>&1 && graphmind fn a' 2>&1)"
    expect "linux-x64 runs on Debian and reports $BARE" "graphmind $BARE" "$out"
    expect "linux-x64 indexes a project (fn a calls b)" "b" "$(sed -n '2,$p' <<<"$out")"
else
    skip "linux-x64 in Docker" "Docker isn't running"
fi

# --- sandboxed HOMEs ---------------------------------------------------------------------------

TOOLS="$SANDBOX/tools"; mkdir -p "$TOOLS"
ln -s "$(command -v jq)" "$TOOLS/jq"   # the Claude Code hooks need jq
unset CLAUDE_CONFIG_DIR GRAPHMIND_SKIP_NOTICES GRAPHMIND_NO_AUTO_BUILD
export GIT_CONFIG_NOSYSTEM=1

# use_home NAME: switch to $SANDBOX/NAME as HOME, with only system tools and that HOME's
# graphmind on PATH (no `claude`, so nothing reaches a real Claude Code config).
use_home() {
    export HOME="$SANDBOX/$1"
    export GRAPHMIND_INSTALL_DIR="$HOME/bin"
    export PATH="$GRAPHMIND_INSTALL_DIR:$TOOLS:/usr/bin:/bin:/usr/sbin:/sbin"
    mkdir -p "$HOME"
    if [ ! -f "$HOME/.gitconfig" ]; then
        printf '[user]\n\tname = smoke\n\temail = smoke@example.com\n[init]\n\tdefaultBranch = main\n[core]\n\texcludesFile = %s/.gitignore_global\n' "$HOME" > "$HOME/.gitconfig"
        touch "$HOME/.zshrc"
    fi
    cd "$HOME" || die "cannot enter $HOME"
}
install_version() { bash "$INSTALL_SH" "$1" >/dev/null 2>&1 && [ "$(graphmind --version 2>/dev/null)" = "graphmind ${1#v}" ]; }
commit_all() { git -C "$1" add -A && git -C "$1" commit -qm "${2:-init}"; }
# The first command after an update starts `graphmind setup` in the background; wait for it.
wait_auto_setup() {
    local i
    for i in $(seq 1 120); do
        pgrep -f "$GRAPHMIND_INSTALL_DIR/graphmind setup" >/dev/null || return 0
        sleep 0.5
    done
    return 1
}
symbols() { graphmind status --slug "$1" 2>/dev/null | sed -n 's/.*Graph: \([0-9]*\) symbols.*/\1/p'; }

section "3. Fresh install of $VERSION"
use_home fresh
check "install.sh installs $VERSION" install_version "$VERSION"
out="$(graphmind setup 2>&1)"; code=$?
[ $code -eq 0 ] && ok "graphmind setup exits 0" || ko "graphmind setup exited $code" "$out"
expect "setup summary: Setup complete" "Setup complete" "$out"
check "hooks installed and registered in settings.json" jq -e '.hooks.SessionStart and .hooks.UserPromptSubmit and .hooks.PreToolUse' "$HOME/.claude/settings.json"
expect "/gm skill installed" "name: gm" "$(head -3 "$HOME/.claude/skills/graphmind/SKILL.md" 2>/dev/null)"
expect "CLAUDE.md block written by $BARE" "GM:VERSION:$BARE" "$(cat "$HOME/.claude/CLAUDE.md" 2>/dev/null)"
out="$(graphmind setup 2>&1)"
expect "a second setup is a no-op that still says Setup complete" "Setup complete" "$out"

section "4. Upgrade $PREVIOUS → $VERSION (clean)"
use_home upgrade
P="$HOME/projects"; mkdir -p "$P"
if install_version "$PREVIOUS"; then
    ok "install.sh installs $PREVIOUS"
    graphmind setup >/dev/null 2>&1
    # app: a repository with a linked worktree. mono: a monorepo whose subfolder svc is the project.
    mkdir -p "$P/app/src" "$P/mono/svc/src"
    printf 'pub fn hello() { world() }\npub fn world() {}\n' > "$P/app/src/lib.rs"
    git -C "$P/app" init -q && commit_all "$P/app"
    git -C "$P/app" worktree add -q "$P/app-wt" -b wt
    printf 'pub fn serve() {}\n' > "$P/mono/svc/src/lib.rs"
    git -C "$P/mono" init -q && commit_all "$P/mono"
    (cd "$P/app" && graphmind init >/dev/null 2>&1) && (cd "$P/mono/svc" && graphmind init >/dev/null 2>&1) \
        && ok "projects initialised with $PREVIOUS" || ko "graphmind init failed with $PREVIOUS"

    # State an earlier version or the person left in ~/.claude.json: a user-scope entry, a project
    # entry pointing at a binary that moved, a hand-made entry, and settings that aren't graphmind's.
    base="$(cat "$HOME/.claude.json" 2>/dev/null || echo '{}')"
    jq --arg app "$P/app" --arg mono "$P/mono" '
        .numStartups = 42
        | .mcpServers.graphmind = {type: "stdio", command: "/old/place/graphmind", args: ["mcp"]}
        | .mcpServers.other = {type: "stdio", command: "other-server", args: []}
        | .projects[$app].mcpServers.graphmind = {type: "stdio", command: "/old/place/graphmind", args: ["mcp"]}
        | .projects[$app].allowedTools = ["Bash"]
        | .projects[$mono].mcpServers.graphmind = {type: "stdio", command: "/usr/local/bin/my-gm-wrapper", args: []}
    ' <<<"$base" > "$HOME/.claude.json"

    check "install.sh upgrades to $VERSION" install_version "$VERSION"
    # The first command after the update runs setup on its own (no terminal: auto-setup only)
    # when the new version's setup or hooks differ; it leaves a stamp when it does.
    rm -f "$HOME/.graphmind/auto-setup.stamp"
    (cd "$P/app" && graphmind status >/dev/null 2>&1)
    check "auto-setup finishes within a minute" wait_auto_setup
    C="$HOME/.claude.json"; BIN="$GRAPHMIND_INSTALL_DIR/graphmind"
    if [ -f "$HOME/.graphmind/auto-setup.stamp" ]; then
        ok "auto-setup ran after the update"
        expect "CLAUDE.md block updated to $BARE" "GM:VERSION:$BARE" "$(cat "$HOME/.claude/CLAUDE.md")"
    else
        skip "auto-setup after the update" "$PREVIOUS and $VERSION have the same setup and hooks"
        out="$(graphmind setup 2>&1)" || ko "graphmind setup failed" "$out"
    fi
    check "hooks current (they call graphmind which)" grep -q 'graphmind which' "$HOME/.claude/hooks/graphmind-prompt.sh"
    check "MCP: app entry repaired to the installed binary" jq -e --arg k "$P/app" --arg b "$BIN" '.projects[$k].mcpServers.graphmind.command == $b' "$C"
    check "MCP: app's other project settings kept" jq -e --arg k "$P/app" '.projects[$k].allowedTools == ["Bash"]' "$C"
    check "MCP: hand-made entry left alone" jq -e --arg k "$P/mono" '.projects[$k].mcpServers.graphmind.command == "/usr/local/bin/my-gm-wrapper"' "$C"
    check "MCP: no entry for the monorepo subfolder" jq -e --arg k "$P/mono/svc" '.projects[$k].mcpServers.graphmind == null' "$C"
    (cd "$P/app-wt" && graphmind status >/dev/null 2>&1)
    check "MCP: no entry for the worktree" jq -e --arg k "$P/app-wt" '.projects[$k].mcpServers.graphmind == null' "$C"
    check "MCP: user-scope graphmind entry removed" jq -e '.mcpServers.graphmind == null' "$C"
    check "MCP: other servers and settings kept" jq -e '.mcpServers.other.command == "other-server" and .numStartups == 42' "$C"
    out="$(graphmind setup 2>&1)"
    expect "setup after upgrade: Setup complete" "Setup complete" "$out"
    expect "setup reports the hand-made entry" "left as you set it" "$out"
else
    ko "install.sh could not install $PREVIOUS"
fi

section "5. Upgrade $PREVIOUS → $VERSION (damaged config)"
use_home damaged
if install_version "$PREVIOUS"; then
    graphmind setup >/dev/null 2>&1
    mkdir -p "$HOME/p/src" && printf 'pub fn f() {}\n' > "$HOME/p/src/lib.rs" && git -C "$HOME/p" init -q && commit_all "$HOME/p"
    (cd "$HOME/p" && graphmind init >/dev/null 2>&1)
    echo '{ not json' > "$HOME/.claude/settings.json"
    mkdir -p "$HOME/.cursor" && echo '{ not json' > "$HOME/.cursor/mcp.json"
    awk '{ print } /GM:VERSION/ { print "My own note inside the graphmind block." }' "$HOME/.claude/CLAUDE.md" > "$HOME/c.md" && mv "$HOME/c.md" "$HOME/.claude/CLAUDE.md"
    printf '# my profile\n' > "$HOME/.zshrc" && chmod 000 "$HOME/.zshrc"
    WATCHED=(.claude/settings.json .cursor/mcp.json .claude/CLAUDE.md .zshrc)
    BEFORE=()
    for f in "${WATCHED[@]}"; do BEFORE+=("$(fingerprint "$HOME/$f")"); done

    out="$(bash "$INSTALL_SH" "$VERSION" 2>&1)"; code=$?
    [ $code -eq 0 ] && ok "install.sh succeeds with an unreadable shell profile" || ko "install.sh exits $code with an unreadable shell profile" "$(tail -3 <<<"$out")"
    [ "$(graphmind --version 2>/dev/null)" = "graphmind $BARE" ] && ok "binary upgraded to $BARE" || ko "binary not upgraded"
    (cd "$HOME/p" && graphmind status >/dev/null 2>&1)
    wait_auto_setup
    out="$(graphmind setup 2>&1)"
    for i in "${!WATCHED[@]}"; do
        f="${WATCHED[$i]}"
        if [ "$(fingerprint "$HOME/$f")" = "${BEFORE[$i]}" ]; then ok "~/$f left byte for byte"; else ko "~/$f was changed"; fi
    done
    expect "setup summary says not everything was set up" "not everything was set up" "$out"
    expect "setup names the unreadable settings.json" "settings.json" "$out"
    expect "setup keeps the edited CLAUDE.md block" "left as you set it" "$out"
    check "graphmind still works" sh -c "cd '$HOME/p' && graphmind fn f"
    chmod 644 "$HOME/.zshrc"
else
    ko "install.sh could not install $PREVIOUS"
fi

section "6. graphmind update $PREVIOUS → $VERSION"
use_home update
if install_version "$PREVIOUS"; then
    out="$(with_timeout 180 graphmind update 2>&1 </dev/null)"
    v="$(graphmind --version 2>&1)"
    if [ "$v" = "graphmind $BARE" ]; then ok "graphmind update installs $BARE"; else ko "graphmind update left $v" "$out"; fi
else
    ko "install.sh could not install $PREVIOUS"
fi

section "7. Behaviour of $VERSION"
use_home upgrade   # set up by section 4, now on $VERSION with registered projects
[ "$(graphmind --version 2>/dev/null)" = "graphmind $BARE" ] || { use_home fresh; }
X="$HOME/fx"; OUTSIDE="$SANDBOX/outside"
mkdir -p "$X/src/generated" "$X/src/sub" "$X/target" "$OUTSIDE"
printf 'pub fn sym_main() { sym_helper() }\npub fn sym_helper() {}\n' > "$X/src/lib.rs"
for f in src/generated/g src/rootonly rootonly src/a.skip src/keep.skip src/sub/local src/sub/kept src/ign forced target/out src/globalign; do
    printf 'pub fn sym_%s() {}\n' "$(echo "$f" | tr '/.' '__')" > "$X/$f.rs"
done
printf 'target/\n**/generated/**\n/rootonly.rs\n*.skip.rs\n!keep.skip.rs\nforced.rs\n' > "$X/.gitignore"
printf 'local.rs\n' > "$X/src/sub/.gitignore"
printf 'ign.rs\n' > "$X/.ignore"
printf 'globalign.rs\n' > "$HOME/.gitignore_global"
printf '# Smokedoc\n\nNotes.\n' > "$X/README.md"
printf 'not code\n' > "$X/notes.txt"
printf 'pub fn sym_external() {}\n' > "$OUTSIDE/ext.rs"
git -C "$X" init -q && commit_all "$X" && git -C "$X" add -f forced.rs && git -C "$X" commit -qm forced
ln -s "$OUTSIDE/ext.rs" "$X/src/linked.rs"
mkfifo "$X/src/pipe.rs"
cd "$X" || die "cannot enter $X"
out="$(with_timeout 120 graphmind init 2>&1)"; code=$?
[ $code -eq 0 ] && ok "init + build finishes with a named pipe in the project" || ko "init failed or hung (exit $code)" "$out"

indexed()     { graphmind file "$1" >/dev/null 2>&1; }
not_indexed() { ! indexed "$1"; }
for f in src/lib.rs src/rootonly.rs src/keep.skip.rs src/sub/kept.rs README.md; do
    check "indexed: $f" indexed "$f"
done
for f in "src/generated/g.rs:**/ pattern" "rootonly.rs:root-anchored pattern" "src/a.skip.rs:glob" \
    "src/sub/local.rs:nested .gitignore" "src/ign.rs:.ignore file" "src/globalign.rs:global gitignore" \
    "forced.rs:ignored but force-added" "target/out.rs:ignored folder" "src/linked.rs:link to outside the project"; do
    check "not indexed: ${f%%:*} (${f#*:})" not_indexed "${f%%:*}"
done
expect "a file linked from outside isn't in the graph" "No results" "$(graphmind search sym_external 2>&1)"

refuse "file: ../ outside the project is refused" graphmind file ../../../../etc/passwd
refuse "file: absolute path is refused" graphmind file /etc/hosts
refuse "file: a project file that isn't indexed is refused" graphmind file notes.txt
check  "file: an indexed file is served" graphmind file src/lib.rs

expect "search finds markdown without --code-only" "README.md" "$(graphmind search Smokedoc 2>&1)"
lacks  "search --code-only leaves markdown out" "README.md" "$(graphmind search Smokedoc --code-only 2>&1)"
expect "fn shows the callee" "sym_helper" "$(graphmind fn sym_main 2>&1)"

cd "$HOME" || exit
out="$(graphmind which 2>&1)"; code=$?
[ $code -eq 1 ] && [ -z "$out" ] && ok "which outside a project: exit 1, no output" || ko "which outside a project: exit $code" "$out"
expect "which inside a subfolder prints the project" "fx" "$(cd "$X/src" && graphmind which 2>&1)"
for cmd in status map cycles export build; do
    check "$cmd --slug fx from outside the project" graphmind "$cmd" --slug fx
done
out="$(graphmind status --slug nope 2>&1)"; code=$?
[ $code -ne 0 ] && ok "unknown slug fails" || ko "unknown slug exited 0" "$out"
expect "unknown slug says it isn't registered" "is not registered" "$out"
refuse "memory add for an unknown project fails" graphmind memory add "x" --slug nope
(cd "$X" && graphmind unregister nope >/dev/null 2>&1)
expect "unregister <typo> keeps the current project" "fx" "$(graphmind list 2>&1)"

# Unreadable project folder: the build fails and the graph stays.
L="$HOME/locked"; mkdir -p "$L/src" && printf 'pub fn l1() {}\npub fn l2() {}\n' > "$L/src/lib.rs"
git -C "$L" init -q && commit_all "$L" && (cd "$L" && graphmind init >/dev/null 2>&1)
n_before="$(symbols locked)"
chmod 000 "$L"
refuse "build of an unreadable project fails" graphmind build --full --slug locked
chmod 755 "$L"
n_after="$(symbols locked)"
if [ -n "$n_before" ] && [ "$n_before" = "$n_after" ] && [ "$n_before" -gt 0 ]; then ok "graph kept ($n_after symbols)"
else ko "graph changed by the failed build ($n_before → $n_after symbols)"; fi

section "8. Memory"
cd "$X" || exit
id="$(graphmind memory add "smoke fact alpha" --type decision 2>&1 | sed -n 's/.*Memory added: \([0-9a-f-]*\).*/\1/p')"
if [ -n "$id" ]; then
    ok "memory add returns an id"
    check "memory update --content --tags" graphmind memory update "$id" --content "smoke fact beta" --tags one two
    out="$(graphmind memory show "$id" 2>&1)"
    expect "memory show has the new text" "smoke fact beta" "$out"
    expect "memory show has the tags" "two" "$out"
    graphmind memory update "$id" --clear-tags --priority >/dev/null 2>&1
    out="$(graphmind memory show "$id" 2>&1)"
    lacks "memory update --clear-tags removed them" "two" "$out"
    expect "memory update --priority" "smoke fact beta" "$(graphmind memory list --priority 2>&1)"
else
    ko "memory add printed no id"
fi
for i in $(seq 1 20); do graphmind memory add "concurrent entry number $i" --type context >/dev/null 2>&1 & done
wait
n="$(grep -c 'concurrent entry number' "$HOME/.graphmind/memory/fx.jsonl" 2>/dev/null)"
[ "$n" = "20" ] && ok "20 concurrent memory add: 20 entries" || ko "20 concurrent memory add: $n entries"
check "memory file is valid JSONL" jq -e . "$HOME/.graphmind/memory/fx.jsonl"

section "9. Claude Code hooks"
for i in $(seq 1 40); do
    graphmind memory add "priority smoke context $i: $(printf 'lorem ipsum dolor sit amet %.0s' $(seq 1 12))" --type decision --priority >/dev/null 2>&1
done
hook() { (cd "$X" && printf '%s' "$2" | bash "$HOME/.claude/hooks/graphmind-$1.sh" 2>/dev/null); }
out="$(hook session '{"session_id":"s1","source":"startup","cwd":"'"$X"'"}')"
[ ${#out} -lt 10000 ] && ok "SessionStart output under 10,000 chars (${#out})" || ko "SessionStart output is ${#out} chars"
check "SessionStart output is valid JSON" jq -e . <<<"${out:-null}"
out="$(hook prompt '{"session_id":"s2","prompt":"how does sym_main call sym_helper in this project","cwd":"'"$X"'"}')"
[ ${#out} -lt 10000 ] && ok "UserPromptSubmit output under 10,000 chars (${#out})" || ko "UserPromptSubmit output is ${#out} chars"
expect "first prompt of a session gets priority context" "priority smoke context" "$out"
out="$(hook prompt '{"session_id":"s2","prompt":"explain where sym_helper is defined please","cwd":"'"$X"'"}')"
lacks "second prompt of the session doesn't repeat it" "priority smoke context" "$out"
out="$(hook search '{"session_id":"s3","tool_name":"Bash","tool_input":{"command":"cd src && rg sym_main"}}')"
expect "nudge fires for rg through Bash" "graphmind-indexed" "$out"
out="$(hook search '{"session_id":"s3","tool_name":"Bash","tool_input":{"command":"grep -r foo ."}}')"
lacks "nudge fires once per session" "graphmind-indexed" "$out"
out="$(hook search '{"session_id":"s4","tool_name":"Agent","tool_input":{"subagent_type":"Explore","prompt":"x"}}')"
expect "nudge fires for Explore agents" "graphmind-indexed" "$out"
out="$(hook search '{"session_id":"s5","tool_name":"Bash","tool_input":{"command":"ls | grep foo"}}')"
lacks "no nudge for a grep after a pipe" "graphmind-indexed" "$out"
out="$(cd "$HOME" && printf '%s' '{"session_id":"s6","tool_name":"Bash","tool_input":{"command":"rg foo"}}' | bash "$HOME/.claude/hooks/graphmind-search.sh" 2>/dev/null)"
lacks "no nudge outside an indexed project" "graphmind-indexed" "$out"

section "10. MCP server"
mcp() {  # mcp '<tools/call params>'... : one JSON-RPC session, responses by id on stdout
    local i=10 msgs=()
    msgs+=('{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"smoke","version":"1"}}}')
    msgs+=('{"jsonrpc":"2.0","method":"notifications/initialized"}')
    msgs+=('{"jsonrpc":"2.0","id":2,"method":"tools/list"}')
    for p in "$@"; do msgs+=("{\"jsonrpc\":\"2.0\",\"id\":$i,\"method\":\"tools/call\",\"params\":$p}"); i=$((i + 1)); done
    { printf '%s\n' "${msgs[@]}"; sleep 4; } | (cd "$X" && with_timeout 30 graphmind mcp 2>/dev/null)
}
R="$(mcp '{"name":"gm_search","arguments":{"query":"sym_main"}}' \
         '{"name":"gm_status","arguments":{"project":"nope"}}' \
         '{"name":"gm_file","arguments":{"file":"../../../../etc/passwd"}}' \
         '{"name":"gm_file","arguments":{"file":"src/lib.rs"}}' \
         '{"name":"gm_status","arguments":{"project":"fx"}}')"
reply() { jq -c --argjson id "$1" 'select(.id == $id)' <<<"$R"; }
failed() { jq -e '(.error != null) or (.result.isError == true)' <<<"$(reply "$1")" >/dev/null; }
n="$(reply 2 | jq '.result.tools | length' 2>/dev/null)"
[ "${n:-0}" -ge 20 ] && ok "MCP initialize + tools/list ($n tools)" || ko "MCP tools/list returned ${n:-nothing}" "$R"
expect "gm_search finds the symbol" "sym_main" "$(reply 10)"
failed 11 && ok "gm_status on an unknown project is an error" || ko "gm_status on an unknown project answered" "$(reply 11)"
expect "…naming the unknown project" "not registered" "$(reply 11)"
failed 12 && ok "gm_file refuses a path outside the project" || ko "gm_file served a path outside the project" "$(reply 12)"
lacks "…and leaks nothing" "root:" "$(reply 12)"
expect "gm_file serves an indexed file" "sym_main" "$(reply 13)"
expect "gm_status with project=fx" "fx" "$(reply 14)"

section "11. Uninstall"
mkdir -p "$HOME/.claude/skills/mine" && echo '# mine' > "$HOME/.claude/skills/mine/SKILL.md"
chmod 555 "$HOME/.claude/skills"
refuse "uninstall skill reports a skill folder it can't remove" graphmind uninstall skill
chmod 755 "$HOME/.claude/skills"
check "uninstall skill" graphmind uninstall skill
check "/gm skill removed" test ! -e "$HOME/.claude/skills/graphmind"
check "other skills kept" test -f "$HOME/.claude/skills/mine/SKILL.md"
out="$(graphmind uninstall all 2>&1)"; code=$?
[ $code -eq 0 ] && ok "uninstall all exits 0" || ko "uninstall all exited $code" "$out"
C="$HOME/.claude.json"
if [ -f "$C" ]; then
    check "no graphmind MCP entry left in .claude.json (hand-made aside)" jq -e '
        [.projects[]?.mcpServers.graphmind? // empty | select((.command // "") | test("/graphmind$"))] | length == 0' "$C"
    if jq -e '.numStartups == 42' "$C" >/dev/null 2>&1; then
        check "other servers and settings kept" jq -e '.mcpServers.other.command == "other-server"' "$C"
    fi
fi
lacks "no graphmind hook left in settings.json" "graphmind" "$(cat "$HOME/.claude/settings.json" 2>/dev/null)"

section "12. Desktop app ($MAC_ARCH disk image)"
DMG="$DL/GraphMind-macos-$MAC_ARCH.dmg"
if [ "$(uname -s)" = Darwin ] && [ -f "$DMG" ]; then
    MNT="$SANDBOX/mnt"; mkdir -p "$MNT"
    if hdiutil attach -nobrowse -readonly -mountpoint "$MNT" "$DMG" >/dev/null 2>&1; then
        APP="$(find "$MNT" -maxdepth 1 -name '*.app' | head -1)"
        if [ -n "$APP" ]; then
            ok "disk image mounts and holds $(basename "$APP")"
            v="$(defaults read "$APP/Contents/Info" CFBundleShortVersionString 2>/dev/null)"
            [ "$v" = "$BARE" ] && ok "app version $v" || ko "app version is $v, expected $BARE"
            out="$(codesign --verify --deep --strict "$APP" 2>&1)" && ok "app signature valid" || ko "app signature invalid" "$out"
            out="$(spctl --assess --type execute -vv "$APP" 2>&1)" && ok "Gatekeeper accepts the app ($(grep -o 'source=.*' <<<"$out"))" \
                || warn "Gatekeeper rejects the app: users get a warning on first launch" "$out"
        else
            ko "no .app in the disk image"
        fi
        hdiutil detach "$MNT" >/dev/null 2>&1
    else
        ko "disk image doesn't mount"
    fi
else
    skip "desktop disk image" "not on macOS or not downloaded"
fi

# --- summary -----------------------------------------------------------------------------------

printf '\n\033[1m== %s: %d passed, %d failed, %d warnings, %d skipped\033[0m\n' "$VERSION" "$PASS" "$FAIL" "$WARN" "$SKIP"
for f in ${FAILURES[@]+"${FAILURES[@]}"}; do printf '  \033[31m✗\033[0m %s\n' "$f"; done
for w in ${WARNINGS[@]+"${WARNINGS[@]}"}; do printf '  \033[33m!\033[0m %s\n' "$w"; done
[ "$FAIL" -eq 0 ]
