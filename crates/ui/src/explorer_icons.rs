//! File / folder icon marks from `assets/explorer-icon/*.svg`.
//!
//! Rendered via [`img`] so each SVG keeps its baked-in colors. GPUI's
//! [`svg`] path always paints as a monochrome alpha mask tinted by
//! `text_color`, which is wrong for these multi-color file-type icons.

use gpui::{IntoElement, ObjectFit, Pixels, SharedString, Styled as _, StyledImage as _, img, px};

const ICON_SIZE: f32 = 18.;

/// Asset path for a file name (basename, with extension).
pub fn file_icon_path(file_name: &str) -> SharedString {
    let lower = file_name.to_ascii_lowercase();
    let stem = icon_for_filename(&lower).unwrap_or_else(|| {
        if lower.ends_with(".d.ts") {
            return "typescript-def";
        }
        let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
        icon_for_extension(ext)
    });
    format!("explorer-icon/{stem}.svg").into()
}

/// Asset path for a directory name (`open` selects the open variant).
pub fn folder_icon_path(folder_name: &str, open: bool) -> SharedString {
    let lower = folder_name.to_ascii_lowercase();
    let stem = folder_icon_stem(&lower);
    let name = if open {
        format!("{stem}_open")
    } else {
        stem.to_string()
    };
    format!("explorer-icon/{name}.svg").into()
}

/// Render an explorer SVG at the standard tree-row size with its native colors.
pub fn explorer_icon(path: impl Into<SharedString>) -> impl IntoElement {
    img(path.into())
        .size(px(ICON_SIZE))
        .flex_shrink_0()
        .object_fit(ObjectFit::Contain)
}

pub fn file_icon(file_name: &str) -> impl IntoElement {
    explorer_icon(file_icon_path(file_name))
}

pub fn folder_icon(folder_name: &str, open: bool) -> impl IntoElement {
    explorer_icon(folder_icon_path(folder_name, open))
}

fn icon_for_filename(name: &str) -> Option<&'static str> {
    Some(match name {
        "cargo.toml" | "cargo.lock" => "rust-config",
        "package.json" => "package-config",
        "package-lock.json" => "package-lock",
        "pnpm-lock.yaml" | "npm-shrinkwrap.json" => "npm-lock",
        "yarn.lock" => "yarn-lock",
        "bun.lock" | "bun.lockb" => "bun-lock",
        "dockerfile"
        | "docker-compose.yml"
        | "docker-compose.yaml"
        | "compose.yml"
        | "compose.yaml" => "docker",
        "makefile" | "gnumakefile" => "makefile",
        "justfile" => "just",
        "readme" | "readme.md" | "readme.mdx" | "readme.txt" => "readme",
        "license" | "license.md" | "license.txt" | "copying" => "license",
        "changelog" | "changelog.md" | "changes" | "changes.md" => "changelog",
        "codeowners" => "codeowners",
        "code_of_conduct.md" | "code-of-conduct.md" => "code-of-conduct",
        ".gitignore" | ".gitattributes" | ".gitmodules" | ".gitkeep" => "git",
        ".eslintrc" | ".eslintrc.js" | ".eslintrc.cjs" | ".eslintrc.json" | "eslint.config.js"
        | "eslint.config.mjs" | "eslint.config.cjs" => "eslint",
        "tsconfig.json" | "tsconfig.base.json" | "jsconfig.json" => "typescript-config",
        "tailwind.config.js"
        | "tailwind.config.ts"
        | "tailwind.config.cjs"
        | "tailwind.config.mjs" => "tailwind",
        "vite.config.js" | "vite.config.ts" | "vite.config.mjs" => "vite",
        "astro.config.mjs" | "astro.config.ts" | "astro.config.js" => "astro-config",
        "next.config.js" | "next.config.mjs" | "next.config.ts" => "next",
        "nuxt.config.js" | "nuxt.config.ts" => "nuxt",
        "go.mod" | "go.sum" => "go-mod",
        ".env" | ".env.local" | ".env.development" | ".env.production" | ".env.example" => "key",
        "todo.md" | "todos.md" => "todo",
        "claude.md" | "agents.md" => "claude",
        _ => return None,
    })
}

fn icon_for_extension(ext: &str) -> &'static str {
    match ext {
        "rs" => "rust",
        "toml" => "toml",
        "json" | "jsonc" | "json5" => "json",
        "js" | "mjs" | "cjs" => "javascript",
        "jsx" => "react",
        "ts" => "typescript",
        "tsx" => "react-typescript",
        "d.ts" => "typescript-def",
        "py" | "pyi" | "pyw" => "python",
        "go" => "go",
        "rb" | "erb" => "ruby",
        "php" => "php",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "swift" => "swift",
        "cs" => "cs",
        "c" => "c",
        "h" => "c-header",
        "cpp" | "cc" | "cxx" | "c++" => "cpp",
        "hpp" | "hh" | "hxx" => "cpp-header",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" | "sass" => "scss",
        "less" | "pcss" => "pcss",
        "md" | "mdown" => "markdown",
        "mdx" => "markdownx",
        "yml" | "yaml" => "yaml",
        "xml" | "xsl" | "xsd" => "xml",
        "svg" => "svg",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "ico" | "bmp" | "avif" => "image",
        "mp4" | "webm" | "mov" | "mkv" | "avi" => "video",
        "mp3" | "wav" | "flac" | "ogg" | "m4a" => "audio",
        "zip" | "tar" | "gz" | "tgz" | "rar" | "7z" | "bz2" => "zip",
        "pdf" => "pdf",
        "csv" => "csv",
        "sql" | "sqlite" | "db" => "database",
        "sh" | "bash" | "zsh" | "fish" | "ksh" => "shell",
        "ps1" | "psm1" | "psd1" => "powershell",
        "lua" => "lua",
        "luau" => "luau",
        "zig" => "zig",
        "nim" => "nim",
        "nix" => "nix",
        "scala" | "sc" => "scala",
        "dart" => "dart",
        "vue" => "vue",
        "svelte" => "svelte",
        "astro" => "astro",
        "tf" | "tfvars" => "terraform",
        "hcl" => "hcl",
        "wasm" => "web-assembly",
        "proto" => "config",
        "graphql" | "gql" => "config",
        "lock" => "lock",
        "txt" | "log" | "text" => "text",
        "woff" | "woff2" | "ttf" | "otf" | "eot" => "font",
        "yml.tpl" => "yaml",
        _ => "_file",
    }
}

fn folder_icon_stem(name: &str) -> &'static str {
    // Strip leading dot for common hidden dirs.
    let name = name.strip_prefix('.').unwrap_or(name);
    match name {
        "src" | "source" | "sources" => "folder_source",
        "lib" | "libs" | "library" | "libraries" => "folder_lib",
        "bin" | "bins" => "folder_bin",
        "dist" | "build" | "out" | "output" | "target" => "folder_dist",
        "node_modules" => "folder_node",
        "docs" | "doc" | "documentation" => "folder_docs",
        "test" | "tests" | "spec" | "specs" | "__tests__" => "folder_test",
        "assets" | "static" | "public" | "resources" => "folder_assets",
        "images" | "img" | "icons" | "media" => "folder_image",
        "styles" | "css" | "scss" | "sass" => "folder_styles",
        "components" | "component" | "ui" => "folder_component",
        "hooks" => "folder_hooks",
        "pages" | "page" | "views" | "screens" => "folder_page",
        "routes" | "router" | "routing" => "folder_routes",
        "server" | "servers" | "backend" => "folder_server",
        "client" | "clients" | "frontend" => "folder_client",
        "api" | "apis" | "service" | "services" => "folder_service",
        "config" | "configs" | "configuration" | "settings" => "folder_config",
        "scripts" | "script" => "folder_script",
        "types" | "typing" | "typings" | "@types" => "folder_types",
        "utils" | "util" | "helpers" | "helper" => "folder_util",
        "temp" | "tmp" | "cache" => "folder_temp",
        "template" | "templates" => "folder_template",
        "layout" | "layouts" => "folder_layout",
        "middleware" => "folder_middleware",
        "model" | "models" => "folder_model",
        "module" | "modules" => "folder_module",
        "package" | "packages" | "pkg" => "folder_package",
        "github" | ".github" => "folder_github",
        "vscode" | ".vscode" => "folder_vscode",
        "storybook" | ".storybook" | "stories" => "folder_storybook",
        "coverage" => "folder_coverage",
        "database" | "db" | "data" => "folder_database",
        "font" | "fonts" => "folder_fonts",
        "video" | "videos" => "folder_video",
        "audio" => "folder_audio",
        "svg" | "svgs" => "folder_svg",
        "web" | "www" | "website" => "folder_web",
        "javascript" | "js" => "folder_javascript",
        "typescript" | "ts" => "folder_typescript",
        "json" => "folder_json",
        "yarn" => "folder_yarn",
        "nuxt" => "folder_nuxt",
        "admin" => "folder_admin",
        "auth" | "authentication" => "folder_auth",
        "error" | "errors" => "folder_error",
        "event" | "events" => "folder_event",
        "function" | "functions" | "fn" | "lambdas" => "folder_function",
        "constant" | "constants" | "consts" => "folder_constant",
        "context" | "contexts" | "providers" | "provider" => "folder_provider",
        "content" => "folder_content",
        "connection" | "connections" => "folder_connection",
        "commands" | "command" => "folder_commands",
        "effects" => "folder_effects",
        "input" | "inputs" => "folder_input",
        "animation" | "animations" => "folder_animation",
        "benchmark" | "benchmarks" | "bench" => "folder_benchmark",
        "builder" | "builders" => "folder_builder",
        "camera" => "folder_camera",
        "changesets" | ".changeset" => "folder_changesets",
        "marketing" => "folder_marketing",
        "git" => "folder_github",
        _ => "_folder",
    }
}

/// Pixel size used for tree icons (for layout alignment).
#[allow(dead_code)]
pub fn icon_size() -> Pixels {
    px(ICON_SIZE)
}
