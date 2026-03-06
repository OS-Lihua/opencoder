//! Language → extension and LSP server mappings.

use std::collections::HashMap;

/// Map of file extension → language ID.
pub fn extension_to_language() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    // Rust
    m.insert("rs", "rust");
    // TypeScript/JavaScript
    m.insert("ts", "typescript");
    m.insert("tsx", "typescriptreact");
    m.insert("js", "javascript");
    m.insert("jsx", "javascriptreact");
    m.insert("mjs", "javascript");
    m.insert("cjs", "javascript");
    // Python
    m.insert("py", "python");
    m.insert("pyi", "python");
    // Go
    m.insert("go", "go");
    // Java
    m.insert("java", "java");
    // C/C++
    m.insert("c", "c");
    m.insert("h", "c");
    m.insert("cpp", "cpp");
    m.insert("cc", "cpp");
    m.insert("cxx", "cpp");
    m.insert("hpp", "cpp");
    // C#
    m.insert("cs", "csharp");
    // Ruby
    m.insert("rb", "ruby");
    // PHP
    m.insert("php", "php");
    // Swift
    m.insert("swift", "swift");
    // Kotlin
    m.insert("kt", "kotlin");
    m.insert("kts", "kotlin");
    // Scala
    m.insert("scala", "scala");
    // Lua
    m.insert("lua", "lua");
    // Shell
    m.insert("sh", "shellscript");
    m.insert("bash", "shellscript");
    m.insert("zsh", "shellscript");
    // Config
    m.insert("json", "json");
    m.insert("jsonc", "jsonc");
    m.insert("yaml", "yaml");
    m.insert("yml", "yaml");
    m.insert("toml", "toml");
    m.insert("xml", "xml");
    // Web
    m.insert("html", "html");
    m.insert("htm", "html");
    m.insert("css", "css");
    m.insert("scss", "scss");
    m.insert("less", "less");
    m.insert("vue", "vue");
    m.insert("svelte", "svelte");
    // Other
    m.insert("md", "markdown");
    m.insert("sql", "sql");
    m.insert("graphql", "graphql");
    m.insert("proto", "proto");
    m.insert("zig", "zig");
    m.insert("nim", "nim");
    m.insert("dart", "dart");
    m.insert("ex", "elixir");
    m.insert("exs", "elixir");
    m.insert("erl", "erlang");
    m.insert("hs", "haskell");
    m.insert("ml", "ocaml");
    m.insert("clj", "clojure");
    m
}

/// Known LSP server commands for common languages.
pub fn language_servers() -> HashMap<&'static str, LspServerInfo> {
    let mut m = HashMap::new();
    m.insert(
        "rust",
        LspServerInfo {
            command: "rust-analyzer",
            args: &[],
        },
    );
    m.insert(
        "typescript",
        LspServerInfo {
            command: "typescript-language-server",
            args: &["--stdio"],
        },
    );
    m.insert(
        "javascript",
        LspServerInfo {
            command: "typescript-language-server",
            args: &["--stdio"],
        },
    );
    m.insert(
        "python",
        LspServerInfo {
            command: "pyright-langserver",
            args: &["--stdio"],
        },
    );
    m.insert(
        "go",
        LspServerInfo {
            command: "gopls",
            args: &[],
        },
    );
    m.insert(
        "c",
        LspServerInfo {
            command: "clangd",
            args: &[],
        },
    );
    m.insert(
        "cpp",
        LspServerInfo {
            command: "clangd",
            args: &[],
        },
    );
    m
}

pub struct LspServerInfo {
    pub command: &'static str,
    pub args: &'static [&'static str],
}

/// Get the language ID for a file path.
pub fn language_for_path(path: &std::path::Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?;
    extension_to_language().get(ext).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn known_extensions() {
        let map = extension_to_language();
        assert_eq!(map["rs"], "rust");
        assert_eq!(map["ts"], "typescript");
        assert_eq!(map["py"], "python");
        assert_eq!(map["go"], "go");
    }

    #[test]
    fn language_for_file() {
        assert_eq!(language_for_path(Path::new("main.rs")), Some("rust"));
        assert_eq!(language_for_path(Path::new("index.ts")), Some("typescript"));
        assert_eq!(language_for_path(Path::new("no_ext")), None);
    }
}
