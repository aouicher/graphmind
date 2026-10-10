//! The graphmind instruction block in ~/.claude/CLAUDE.md for `graphmind setup` and `uninstall`:
//! install or update it, leave a block the person edited (or a file that can't be read) as is,
//! and remove exactly the block on uninstall.

use super::{home_dir, Outcome};
use colored::Colorize;
use std::fs;

/// Install or update the graphmind block in ~/.claude/CLAUDE.md. A block the person edited, or
/// a CLAUDE.md that can't be read, is left as is; a backup is made only when the file changes.
#[doc(hidden)]
pub fn install_claude_md_block() -> Outcome {
    let claude_md = home_dir().join(".claude").join("CLAUDE.md");
    let left_alone = |reason: String| {
        println!("    {} {reason}", "⊘".yellow());
        Outcome::LeftAlone(reason)
    };

    let content = match fs::read_to_string(&claude_md) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        // Writing now would replace the person's file with just the block.
        Err(e) => return left_alone(format!("can't read {} ({e}); left unchanged", claude_md.display())),
    };
    let block = gm_block_marked();

    let new_content = if let Some(start) = content.find("<!-- GM:START -->") {
        // Replace existing block in place — use char-safe slicing via find (returns byte indices on valid UTF-8 boundaries)
        let end = content.find("<!-- GM:END -->")
            .map(|i| i + "<!-- GM:END -->".len())
            .unwrap_or(content.len());
        if gm_block_edited(&content[start..end]) {
            return left_alone(
                "graphmind block in CLAUDE.md was edited; left as is (delete the block to get the current one)"
                    .to_string(),
            );
        }
        format!("{}{}{}", &content[..start], block, &content[end..])
    } else if let Some(omc_start) = content.find("<!-- OMC:START -->") {
        // Insert before OMC block — highest attention weight position
        format!("{}{}\n\n{}", &content[..omc_start], block, &content[omc_start..])
    } else {
        // No existing block — prepend before other content
        format!("{}\n\n{}", block, content)
    };

    if new_content == content {
        println!("    {} instruction block already current", "✓".green());
        return Outcome::Unchanged;
    }
    if claude_md.exists() {
        if let Err(reason) = graphmind_config::back_up(&claude_md) {
            return left_alone(reason);
        }
    } else if let Some(parent) = claude_md.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return left_alone(format!("can't create {} ({e})", parent.display()));
        }
    }
    match graphmind_config::write_atomic(&claude_md, new_content.as_bytes()) {
        Ok(()) => {
            println!("    {} instruction block installed", "✓".green());
            Outcome::Written
        }
        Err(e) => left_alone(format!("failed to write {} ({e}); left unchanged", claude_md.display())),
    }
}

#[doc(hidden)]
pub fn uninstall_claude_md_block() {
    let claude_md = home_dir().join(".claude").join("CLAUDE.md");
    if !claude_md.exists() {
        return;
    }

    let content = fs::read_to_string(&claude_md).unwrap_or_default();
    let Some(start) = content.find("<!-- GM:START -->") else {
        return;
    };

    // Backup before modifying (timestamped)
    graphmind_config::back_up(&claude_md).ok();

    let end = content.find("<!-- GM:END -->")
        .map(|i| i + "<!-- GM:END -->".len())
        .unwrap_or(content.len());

    let mut new_content = format!("{}{}", &content[..start], &content[end..]);

    // Collapse any run of 3+ consecutive newlines into exactly 2.
    while new_content.contains("\n\n\n") {
        new_content = new_content.replace("\n\n\n", "\n\n");
    }

    match graphmind_config::write_atomic(&claude_md, new_content.as_bytes()) {
        Ok(()) => println!("    {} instruction block removed", "✓".green()),
        Err(e) => println!("    {} failed to write CLAUDE.md: {e}", "✗".red()),
    }
}

fn gm_block() -> String {
    let version = env!("CARGO_PKG_VERSION");
    format!(r#"<!-- GM:START -->
<!-- GM:VERSION:{version} -->

## Mandatory: Code exploration via graphmind

In any graphmind-registered project, ALWAYS use `/gm` as the FIRST step for ANY code exploration, architecture understanding, or symbol lookup. This applies to:
- Answering questions about how something works
- Finding symbols, callers, dependencies, outlines
- Understanding code before making changes
- Exploring architecture or blast radius

**Do NOT** spawn Explore agents, use grep/find/Glob for code structure, or delegate code search to sub-agents until `/gm` has been tried first.

To **read** a symbol's source code, use `/gm-fn <symbol>` — it returns full source + callers + callees in one call. Prefer it over Read when you already know the symbol name.

Only fall back to grep for: string literals, config values, env vars, non-code patterns.

Invoke: `Skill(skill: "gm", args: "<your query>")`

**Auto-memory — mandatory behavior**: You MUST proactively save knowledge to graphmind memory **during** and after every session, without waiting for the user to ask. Save immediately when something is decided or discovered — do not wait for the end of the session.

Categories and commands:
- Architectural decisions → `graphmind memory add "<decision>" --type decision`
- Patterns/conventions → `--type pattern` or `--type convention`
- Bugs found or fixed → `--type bug`
- Critical always-needed facts → add `--priority` (injected every session)
- Cross-project or user-level facts → add `--global`

In a registered project: save without `--global` by default (project-scoped).
Outside a registered project: always use `--global`.

Be selective — only facts useful in a future session. Skip task details and temporary state.

<!-- GM:END -->"#)
}

/// `gm_fingerprint(gm_block_body(..))` of every stock block text graphmind wrote before blocks
/// carried a fingerprint (0.2.77 to 0.3.2, oldest first; from the history of `gm_block`). A block
/// without a fingerprint is replaced only when its whole text is one of these: any other text was
/// written or edited by the person.
const STOCK_BLOCK_FINGERPRINTS: [&str; 7] = [
    "f3ddd91a594a5fd8",
    "6e566843a79d888f",
    "762c74aa1f1dca65",
    "badc817d7e6bb45b",
    "52a140c2072244b5",
    "c5869b2212e82172",
    "cd767b14b681f9cf",
];

/// Lines of a GM block without its marker comments (START, VERSION, HASH, END).
fn gm_block_body(block: &str) -> String {
    block
        .lines()
        .filter(|l| !l.trim_start().starts_with("<!-- GM:"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// FNV-1a: a fingerprint that is stable across Rust versions (DefaultHasher isn't).
fn gm_fingerprint(text: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

/// The GM block as written by this version, with a fingerprint of its text so a later setup can
/// tell whether the person has edited it.
fn gm_block_marked() -> String {
    let block = gm_block();
    let hash = gm_fingerprint(&gm_block_body(&block));
    block.replacen("<!-- GM:START -->", &format!("<!-- GM:START -->\n<!-- GM:HASH:{hash} -->"), 1)
}

/// True when the existing block's text is not what graphmind wrote: it doesn't match its
/// fingerprint or, for a block from before fingerprints, isn't one of the stock texts.
fn gm_block_edited(existing: &str) -> bool {
    let fingerprint = gm_fingerprint(&gm_block_body(existing));
    let recorded = existing
        .lines()
        .find_map(|l| l.trim().strip_prefix("<!-- GM:HASH:")?.strip_suffix(" -->"));
    match recorded {
        Some(recorded) => recorded != fingerprint,
        None => !STOCK_BLOCK_FINGERPRINTS.contains(&fingerprint.as_str()),
    }
}
