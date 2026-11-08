//! Syntax colours for fenced code blocks and the text files chat previews: strings, numbers,
//! comments and each language's keywords, picked by discord's language aliases or file extension.

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId};

struct Lang {
    name: &'static str,
    aliases: &'static [&'static str],
    line: &'static [&'static str],
    block: Option<(&'static str, &'static str)>,
    keywords: &'static [&'static str],
    markup: bool,
}

const C_LIKE: &[&str] = &["//"];
const HASH: &[&str] = &["#"];
const DASH: &[&str] = &["--"];

const LANGS: &[Lang] = &[
    Lang {
        name: "Rust",
        aliases: &["rs", "rust"],
        line: C_LIKE,
        block: Some(("/*", "*/")),
        keywords: &["as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where", "while"],
        markup: false,
    },
    Lang {
        name: "JavaScript",
        aliases: &["js", "javascript", "mjs", "cjs", "jsx", "ts", "typescript", "tsx", "mts", "cts", "json", "jsonc"],
        line: C_LIKE,
        block: Some(("/*", "*/")),
        keywords: &["async", "await", "break", "case", "catch", "class", "const", "continue", "default", "else", "export", "extends", "false", "finally", "for", "from", "function", "if", "import", "in", "interface", "let", "new", "null", "of", "return", "static", "super", "switch", "this", "throw", "true", "try", "type", "typeof", "undefined", "var", "while", "yield"],
        markup: false,
    },
    Lang {
        name: "C",
        aliases: &[
            "c", "h", "cpp", "c++", "h++", "hpp", "hh", "hxx", "cxx", "cc", "cs", "c#", "csharp", "java", "jsp", "go", "golang", "kt", "kts", "kotlin", "swift", "php", "dart", "scala",
            "groovy", "gradle", "objectivec", "mm", "objc", "obj-c", "obj-c++", "objective-c++", "arduino", "glsl", "d", "vala", "processing", "pde", "haxe", "hx", "zephir", "zep",
            "thrift", "protobuf", "proto", "capnproto", "capnp", "actionscript", "as", "qml", "qt",
        ],
        line: C_LIKE,
        block: Some(("/*", "*/")),
        keywords: &["auto", "break", "case", "char", "class", "const", "continue", "default", "defer", "do", "double", "else", "enum", "extends", "extern", "false", "final", "float", "for", "func", "fun", "go", "goto", "if", "implements", "import", "int", "interface", "long", "namespace", "new", "nil", "null", "nullptr", "override", "package", "private", "protected", "public", "return", "short", "signed", "sizeof", "static", "struct", "switch", "this", "throw", "throws", "true", "try", "catch", "type", "typedef", "union", "unsigned", "using", "val", "var", "virtual", "void", "while"],
        markup: false,
    },
    Lang {
        name: "Python",
        aliases: &["py", "pyw", "python", "gyp", "ipython", "python-repl"],
        line: HASH,
        block: None,
        keywords: &["and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif", "else", "except", "False", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return", "True", "try", "while", "with", "yield"],
        markup: false,
    },
    Lang {
        name: "Shell",
        aliases: &[
            "sh", "bash", "zsh", "shell", "console", "shellsession", "yml", "yaml", "toml", "ini", "cfg", "conf", "properties", "rb", "ruby", "gemspec", "podspec", "thor", "irb", "ps", "ps1",
            "pwsh", "powershell", "makefile", "mk", "mak", "make", "dockerfile", "docker", "nginx", "nginxconf", "apache", "apacheconf", "perl", "pl", "pm", "r", "elixir", "ex", "exs",
            "tcl", "tk", "crystal", "cr", "julia", "nim", "cmake", "coffeescript", "coffee", "cson", "iced",
        ],
        line: HASH,
        block: None,
        keywords: &["begin", "case", "class", "def", "do", "done", "elif", "else", "elsif", "end", "esac", "export", "fi", "for", "function", "if", "in", "local", "module", "nil", "require", "return", "then", "true", "false", "unless", "until", "while"],
        markup: false,
    },
    Lang {
        name: "SQL",
        aliases: &["sql", "pgsql", "postgres", "postgresql", "n1ql"],
        line: DASH,
        block: Some(("/*", "*/")),
        keywords: &["add", "all", "alter", "and", "as", "asc", "between", "by", "case", "create", "delete", "desc", "distinct", "drop", "else", "end", "exists", "false", "from", "group", "having", "in", "index", "inner", "insert", "into", "is", "join", "left", "like", "limit", "not", "null", "on", "or", "order", "outer", "primary", "right", "select", "set", "table", "then", "true", "union", "update", "values", "when", "where", "with"],
        markup: false,
    },
    Lang {
        name: "Lua",
        aliases: &["lua", "pluto"],
        line: DASH,
        block: Some(("--[[", "]]")),
        keywords: &["and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in", "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while"],
        markup: false,
    },
    Lang {
        name: "Haskell",
        aliases: &["haskell", "hs", "elm"],
        line: DASH,
        block: Some(("{-", "-}")),
        keywords: &["case", "class", "data", "deriving", "do", "else", "if", "import", "in", "infix", "instance", "let", "module", "newtype", "of", "then", "type", "where"],
        markup: false,
    },
    Lang { name: "CSS", aliases: &["css", "scss", "less", "stylus", "styl"], line: &[], block: Some(("/*", "*/")), keywords: &["important"], markup: false },
    Lang { name: "Diff", aliases: &["diff", "patch"], line: &[], block: None, keywords: &[], markup: false },
    Lang { name: "HTML", aliases: &["html", "htm", "xhtml", "xml", "rss", "atom", "xjb", "xsd", "xsl", "plist", "wsf", "svg"], line: &[], block: Some(("<!--", "-->")), keywords: &[], markup: true },
    Lang { name: "Plain text", aliases: &[], line: &[], block: None, keywords: &[], markup: false },
];

pub fn names() -> impl Iterator<Item = &'static str> {
    LANGS.iter().map(|l| l.name)
}

fn by_alias(alias: &str) -> &'static Lang {
    let alias = alias.to_ascii_lowercase();
    LANGS.iter().find(|l| l.aliases.contains(&alias.as_str())).unwrap_or(&LANGS[LANGS.len() - 1])
}

fn lang(file: &str) -> &'static Lang {
    by_alias(file.rsplit_once('.').map_or("", |(_, e)| e))
}

pub fn language(file: &str) -> &'static str {
    lang(file).name
}

struct Colors {
    base: Color32,
    keyword: Color32,
    string: Color32,
    number: Color32,
    comment: Color32,
    tag: Color32,
    attr: Color32,
}

pub fn job(file: &str, forced: Option<&str>, text: &str, font: FontId, base: Color32, comment: Color32, wrap: f32) -> LayoutJob {
    let l = forced.and_then(|f| LANGS.iter().find(|l| l.name == f)).unwrap_or_else(|| lang(file));
    coloured(l, text, font, base, comment, wrap)
}

/// A fenced code block's job: its language by the name after the opening fence, plain when
/// that's empty or nothing here knows it.
pub fn fence(alias: &str, text: &str, font: FontId, base: Color32, comment: Color32) -> LayoutJob {
    coloured(by_alias(alias), text, font, base, comment, f32::INFINITY)
}

fn coloured(l: &Lang, text: &str, font: FontId, base: Color32, comment: Color32, wrap: f32) -> LayoutJob {
    let c = Colors {
        base,
        keyword: Color32::from_rgb(0xff, 0x7b, 0x72),
        string: Color32::from_rgb(0x7e, 0xe7, 0x87),
        number: Color32::from_rgb(0xff, 0xa6, 0x57),
        comment,
        tag: Color32::from_rgb(0xd2, 0xa8, 0xff),
        attr: Color32::from_rgb(0x79, 0xc0, 0xff),
    };
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    let mut put = |s: &str, color: Color32| {
        if !s.is_empty() {
            job.append(s, 0.0, TextFormat { font_id: font.clone(), color, ..Default::default() });
        }
    };
    if l.aliases.is_empty() {
        put(text, c.base);
        return job;
    }
    if l.name == "Diff" {
        for line in text.split_inclusive('\n') {
            let color = match line.as_bytes().first() {
                Some(b'+') => c.string,
                Some(b'-') => c.keyword,
                Some(b'@') => c.attr,
                _ => c.base,
            };
            put(line, color);
        }
        return job;
    }
    let b = text.as_bytes();
    let (mut i, mut plain) = (0, 0);
    macro_rules! emit {
        ($end:expr, $color:expr) => {{
            put(&text[plain..i], c.base);
            put(&text[i..$end], $color);
            i = $end;
            plain = i;
        }};
    }
    while i < b.len() {
        let rest = &text[i..];
        if let Some((open, close)) = l.block.filter(|(o, _)| rest.starts_with(o)) {
            let end = rest[open.len()..].find(close).map_or(text.len(), |e| i + open.len() + e + close.len());
            emit!(end, c.comment);
        } else if l.line.iter().any(|p| rest.starts_with(p)) {
            let end = rest.find('\n').map_or(text.len(), |e| i + e);
            emit!(end, c.comment);
        } else if l.markup && b[i] == b'<' {
            let end = rest.find('>').map_or(text.len(), |e| i + e + 1);
            put(&text[plain..i], c.base);
            markup_tag(&text[i..end], &c, &mut put);
            i = end;
            plain = i;
        } else if (b[i] == b'"' || (b[i] == b'\'' && !l.markup) || (b[i] == b'`' && l.name == "JavaScript")) && !l.markup {
            let q = b[i];
            let mut j = i + 1;
            while j < b.len() && b[j] != q && b[j] != b'\n' {
                j += if b[j] == b'\\' { 2 } else { 1 };
            }
            let end = (j + 1).min(text.len());
            emit!(end, c.string);
        } else if b[i].is_ascii_digit() && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_')) {
            let end = i + rest.bytes().take_while(|x| x.is_ascii_alphanumeric() || *x == b'.' || *x == b'_').count();
            emit!(end, c.number);
        } else if b[i].is_ascii_alphabetic() || b[i] == b'_' {
            let len = rest.bytes().take_while(|x| x.is_ascii_alphanumeric() || *x == b'_').count();
            let word = &rest[..len];
            if l.keywords.contains(&word) || (l.name == "SQL" && l.keywords.contains(&word.to_ascii_lowercase().as_str())) {
                emit!(i + len, c.keyword);
            } else {
                i += len;
            }
        } else {
            i += rest.chars().next().map_or(1, char::len_utf8);
        }
    }
    put(&text[plain..], c.base);
    job
}

fn markup_tag(tag: &str, c: &Colors, put: &mut impl FnMut(&str, Color32)) {
    if tag.starts_with("<!--") {
        return put(tag, c.comment);
    }
    let name_end = tag.find(|ch: char| ch.is_whitespace() || ch == '>').unwrap_or(tag.len());
    put(&tag[..1], c.base);
    put(&tag[1..name_end], c.tag);
    let mut rest = &tag[name_end..];
    while !rest.is_empty() {
        if let Some(q) = rest.find('"').filter(|q| rest[..*q].ends_with('=')) {
            let (before, after) = rest.split_at(q);
            let name_at = before.trim_end_matches('=').rfind(char::is_whitespace).map_or(0, |p| p + 1);
            put(&before[..name_at], c.base);
            put(&before[name_at..before.len() - 1], c.attr);
            put("=", c.base);
            let end = after[1..].find('"').map_or(after.len(), |e| e + 2);
            put(&after[..end], c.string);
            rest = &after[end..];
        } else {
            put(rest, c.base);
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(file: &str, text: &str) -> Vec<(String, Color32)> {
        let j = job(file, None, text, FontId::monospace(14.0), Color32::WHITE, Color32::GRAY, f32::INFINITY);
        j.sections.iter().map(|s| (j.text[s.byte_range.start.0..s.byte_range.end.0].to_string(), s.format.color)).collect()
    }

    #[test]
    fn rust_keywords_strings_numbers_and_comments_are_coloured() {
        let p = parts("a.rs", "let x = \"hi\"; // note\nlet n = 42;");
        let colour = |t: &str| p.iter().find(|(s, _)| s == t).map(|(_, c)| *c);
        let (kw, st, no, cm) = (colour("let"), colour("\"hi\""), colour("42"), colour("// note"));
        assert!(kw.is_some() && kw != Some(Color32::WHITE));
        assert!(st.is_some() && st != kw);
        assert!(no.is_some() && no != kw && no != st);
        assert_eq!(cm, Some(Color32::GRAY));
        assert_eq!(p.iter().map(|(s, _)| s.as_str()).collect::<String>(), "let x = \"hi\"; // note\nlet n = 42;");
    }

    fn fenced(alias: &str, text: &str) -> Vec<(String, Color32)> {
        let j = fence(alias, text, FontId::monospace(14.0), Color32::WHITE, Color32::GRAY);
        j.sections.iter().map(|s| (j.text[s.byte_range.start.0..s.byte_range.end.0].to_string(), s.format.color)).collect()
    }

    #[test]
    fn discords_common_aliases_are_coloured() {
        let word = |a: &str, text: &str, token: &str| fenced(a, text).iter().find(|(s, _)| s == token).map(|(_, c)| *c).filter(|c| *c != Color32::WHITE);
        let cases = [
            ("rs", "let x", "let"), ("rust", "let x", "let"), ("RUST", "let x", "let"), ("js", "const x", "const"), ("javascript", "const x", "const"), ("ts", "const x", "const"),
            ("json", "{\"a\": 1}", "\"a\""), ("py", "def f", "def"), ("python", "def f", "def"), ("sh", "if x", "if"), ("bash", "if x", "if"), ("cpp", "int x", "int"), ("c", "int x", "int"),
            ("cs", "int x", "int"), ("go", "func f", "func"), ("java", "int x", "int"), ("kt", "fun f", "fun"), ("yaml", "a: 1", "1"), ("toml", "a = 1", "1"), ("css", "a { top: 1px }", "1px"),
            ("sql", "SELECT a", "SELECT"), ("lua", "local x", "local"), ("hs", "where x", "where"), ("rb", "def x", "def"),
        ];
        for (alias, text, token) in cases {
            assert!(word(alias, text, token).is_some(), "{alias}");
        }
        assert!(fenced("html", "<a href=\"x\">").iter().any(|(_, c)| *c != Color32::WHITE));
        assert!(fenced("xml", "<a href=\"x\">").iter().any(|(_, c)| *c != Color32::WHITE));
    }

    #[test]
    fn a_diff_colours_by_its_lines() {
        let p = fenced("diff", "@@ -1 +1 @@\n-old\n+new\nsame");
        let colour = |t: &str| p.iter().find(|(s, _)| s == t).map(|(_, c)| *c);
        assert_ne!(colour("-old\n"), colour("+new\n"));
        assert_ne!(colour("@@ -1 +1 @@\n"), Some(Color32::WHITE));
        assert_eq!(colour("same"), Some(Color32::WHITE));
    }

    #[test]
    fn no_language_or_an_unknown_one_stays_plain() {
        for alias in ["", "text", "brainfuck"] {
            assert!(fenced(alias, "let x = 1 // y").iter().all(|(_, c)| *c == Color32::WHITE), "{alias}");
        }
    }

    #[test]
    fn plain_text_and_markup_keep_every_character() {
        for (file, text) in [("a.txt", "just words, 12 \"quoted\" // not a comment"), ("a.html", "<a href=\"x\">hi</a><!-- c -->"), ("a.py", "def f(): # c\n  return 'é'")] {
            assert_eq!(parts(file, text).iter().map(|(s, _)| s.as_str()).collect::<String>(), text, "{file}");
        }
        assert!(parts("a.txt", "while 1 // x").iter().all(|(_, c)| *c == Color32::WHITE));
    }
}
