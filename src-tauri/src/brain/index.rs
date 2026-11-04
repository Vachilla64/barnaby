//! Repo index: file list, symbols, a text repo map, and (for small repos) the full text.

use ignore::WalkBuilder;
use regex::Regex;
use std::path::{Path, PathBuf};

const MAX_FILE_BYTES: u64 = 200 * 1024;
const FULL_TEXT_TOKEN_LIMIT: usize = 400_000;
const SKIP_NAMES: [&str; 6] = ["package-lock.json", "pnpm-lock.yaml", "yarn.lock", "Cargo.lock", "bun.lockb", "poetry.lock"];
const SKIP_DIRS: [&str; 7] = ["node_modules", "target", "dist", "build", ".git", "out", "__pycache__"];
const TEXT_EXTS: [&str; 24] = [
    "ts", "tsx", "js", "jsx", "mjs", "py", "rs", "go", "java", "kt", "cs", "rb", "php", "c", "h", "cpp",
    "html", "css", "json", "md", "toml", "yaml", "yml", "sql",
];

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub rel_path: String,
    pub lines: usize,
    pub symbols: Vec<String>,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct RepoIndex {
    pub root: PathBuf,
    pub files: Vec<FileEntry>,
}

fn symbol_regex(ext: &str) -> Option<Regex> {
    let pattern = match ext {
        "ts" | "tsx" | "js" | "jsx" | "mjs" => {
            r"^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?(?:function\*?|class|interface|type|enum|const|let)\s+([A-Za-z_$][\w$]*)"
        }
        "py" => r"^\s*(?:async\s+)?(?:def|class)\s+(\w+)",
        "rs" => r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(?:fn|struct|enum|trait|mod|type)\s+(\w+)",
        "go" => r"^(?:func\s+(?:\([^)]*\)\s*)?|type\s+)(\w+)",
        _ => return None,
    };
    Regex::new(pattern).ok()
}

pub fn extract_symbols(ext: &str, text: &str) -> Vec<String> {
    let Some(re) = symbol_regex(ext) else { return vec![] };
    text.lines()
        .filter_map(|l| re.captures(l).map(|c| c[1].to_string()))
        .take(60)
        .collect()
}

impl RepoIndex {
    pub fn build(root: &Path) -> RepoIndex {
        let mut files = Vec::new();
        let walker = WalkBuilder::new(root)
            .hidden(true)
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !SKIP_DIRS.contains(&name.as_ref())
            })
            .build();
        for entry in walker.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if SKIP_NAMES.contains(&name.as_str()) || name.ends_with(".min.js") {
                continue;
            }
            let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            if !TEXT_EXTS.contains(&ext.as_str()) {
                continue;
            }
            if entry.metadata().map(|m| m.len() > MAX_FILE_BYTES).unwrap_or(true) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(path) else { continue };
            let rel = path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/");
            files.push(FileEntry {
                lines: text.lines().count(),
                symbols: extract_symbols(&ext, &text),
                rel_path: rel,
                text,
            });
        }
        files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
        RepoIndex { root: root.to_path_buf(), files }
    }

    /// The file tree with each file's symbols, as text.
    pub fn repo_map(&self) -> String {
        let mut out = String::new();
        for f in &self.files {
            out.push_str(&format!("{} ({} lines)", f.rel_path, f.lines));
            if !f.symbols.is_empty() {
                out.push_str(&format!(": {}", f.symbols.join(", ")));
            }
            out.push('\n');
        }
        out
    }

    pub fn estimated_tokens(&self) -> usize {
        self.files.iter().map(|f| f.text.len()).sum::<usize>() / 4
    }

    /// Full source with line numbers, only when the project is small enough.
    pub fn full_text(&self) -> Option<String> {
        if self.estimated_tokens() > FULL_TEXT_TOKEN_LIMIT {
            return None;
        }
        let mut out = String::new();
        for f in &self.files {
            out.push_str(&format!("\n=== {} ===\n", f.rel_path));
            for (i, line) in f.text.lines().enumerate() {
                out.push_str(&format!("{:>4}| {}\n", i + 1, line));
            }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_symbols_per_language() {
        assert_eq!(
            extract_symbols("ts", "export async function login(a) {}\nconst x = 1;\nclass Form {}\n"),
            vec!["login", "x", "Form"]
        );
        assert_eq!(extract_symbols("py", "def a():\n  pass\nclass B:\n  def c(self): pass\n"), vec!["a", "B", "c"]);
        assert_eq!(extract_symbols("rs", "pub fn run() {}\nstruct S;\n"), vec!["run", "S"]);
        assert_eq!(extract_symbols("go", "func (s *S) Handle() {}\ntype T struct{}\n"), vec!["Handle", "T"]);
        assert!(extract_symbols("md", "# hi").is_empty());
    }

    #[test]
    fn builds_index_skipping_junk() {
        let dir = std::env::temp_dir().join("barnaby_index_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("node_modules/x")).unwrap();
        std::fs::write(dir.join("src/auth.ts"), "export function check() {}\n").unwrap();
        std::fs::write(dir.join("node_modules/x/i.js"), "function junk(){}").unwrap();
        std::fs::write(dir.join("pnpm-lock.yaml"), "x: 1").unwrap();
        std::fs::write(dir.join("logo.png"), [0u8, 1, 2]).unwrap();
        let idx = RepoIndex::build(&dir);
        assert_eq!(idx.files.len(), 1);
        assert!(idx.repo_map().contains("src/auth.ts (1 lines): check"));
        assert!(idx.full_text().unwrap().contains("   1| export function check"));
    }
}
